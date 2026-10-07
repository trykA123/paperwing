use super::{repository_paths, round};
use crate::git;
use crate::settings::Settings;
use serde::Serialize;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;

pub(super) const REPO_PREFIX: &str = "repo-";

const CANCELLED: &str = "Diagnostics collection cancelled";
const FAILED: &str = "Diagnostics scale collection failed";

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScaleData {
    pub set_count: u64,
    pub repos_per_set: Vec<u64>,
    pub repos: Vec<ScaleRepo>,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScaleRepo {
    pub repo: String,
    pub tracked_file_count: u64,
    pub ref_count: u64,
    pub loose_object_bytes: u64,
    pub packed_object_bytes: u64,
    pub worktree: bool,
}

pub async fn collect(
    settings: &Settings,
    cancellation: Arc<AtomicBool>,
    progress: Arc<dyn Fn(u64, u64) + Send + Sync>,
) -> Result<ScaleData, &'static str> {
    let set_count = settings
        .workspace
        .get("sets")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);
    let mut candidates = repository_paths::candidates(settings);
    randomize(&mut candidates);
    let total = (candidates.len() as u64).saturating_mul(3);
    let mut completed = 0;
    let mut counts_by_set = vec![0_u64; set_count];
    let mut repos = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.into_iter().enumerate() {
        if cancellation.load(Ordering::Relaxed) {
            return Err(CANCELLED);
        }
        let tracked_file_count = count_streamed(
            &candidate.path,
            &["ls-files", "-z"],
            0,
            cancellation.clone(),
        )
        .await?;
        completed += 1;
        progress(completed, total);
        let ref_count = count_streamed(
            &candidate.path,
            &["for-each-ref", "--format=x"],
            b'\n',
            cancellation.clone(),
        )
        .await?;
        completed += 1;
        progress(completed, total);
        let (loose_object_bytes, packed_object_bytes) =
            object_sizes(&candidate.path, cancellation.clone()).await?;
        completed += 1;
        progress(completed, total);
        if let Some(set_count) = counts_by_set.get_mut(candidate.set_index) {
            *set_count += 1;
        }
        repos.push(ScaleRepo {
            repo: format!("{REPO_PREFIX}{}", index + 1),
            tracked_file_count: round::count(tracked_file_count),
            ref_count: round::count(ref_count),
            loose_object_bytes: round::count(loose_object_bytes),
            packed_object_bytes: round::count(packed_object_bytes),
            worktree: candidate.path.join(".git").is_file(),
        });
    }
    randomize(&mut counts_by_set);
    Ok(ScaleData {
        set_count: round::count(set_count as u64),
        repos_per_set: counts_by_set.into_iter().map(round::count).collect(),
        repos,
    })
}

async fn count_streamed(
    repo: &Path,
    command: &[&str],
    delimiter: u8,
    cancellation: Arc<AtomicBool>,
) -> Result<u64, &'static str> {
    if cancellation.load(Ordering::Relaxed) {
        return Err(CANCELLED);
    }
    let path = repo.to_string_lossy();
    let mut args = vec!["-C", path.as_ref()];
    args.extend_from_slice(command);
    let count = Arc::new(AtomicU64::new(0));
    let sink_count = count.clone();
    let sink_cancel = cancellation.clone();
    let sink: git::StdoutSink = Arc::new(move |bytes| {
        sink_count.fetch_add(
            bytes.iter().filter(|byte| **byte == delimiter).count() as u64,
            Ordering::Relaxed,
        );
        sink_cancel.load(Ordering::Relaxed)
    });
    let request = git::Request {
        args: &args,
        context: "diagnostics-scale",
        timeout: Duration::from_secs(120),
        expected: &[0],
        policy: git::OutputPolicy::Metadata,
    };
    let result = git::execute_streaming(request, cancellation.clone(), sink)
        .await
        .map_err(|_| FAILED)?;
    if cancellation.load(Ordering::Relaxed) {
        return Err(CANCELLED);
    }
    if result.code != Some(0) {
        return Err(FAILED);
    }
    Ok(count.load(Ordering::Relaxed))
}

async fn object_sizes(
    repo: &Path,
    cancellation: Arc<AtomicBool>,
) -> Result<(u64, u64), &'static str> {
    if cancellation.load(Ordering::Relaxed) {
        return Err(CANCELLED);
    }
    let path = repo.to_string_lossy();
    let args = ["-C", path.as_ref(), "count-objects", "-v"];
    let request = git::Request {
        args: &args,
        context: "diagnostics-scale",
        timeout: Duration::from_secs(120),
        expected: &[0],
        policy: git::OutputPolicy::Metadata,
    };
    let result = git::execute_cancellable(request, cancellation.clone())
        .await
        .map_err(|_| FAILED)?;
    if cancellation.load(Ordering::Relaxed) {
        return Err(CANCELLED);
    }
    if result.code != Some(0) {
        return Err(FAILED);
    }
    let text = std::str::from_utf8(&result.stdout).map_err(|_| FAILED)?;
    let mut loose_kib = None;
    let mut packed_kib = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let Ok(value) = value.trim().parse::<u64>() else {
            continue;
        };
        match key {
            "size" => loose_kib = Some(value),
            "size-pack" => packed_kib = Some(value),
            _ => {}
        }
    }
    Ok((
        loose_kib.ok_or(FAILED)?.saturating_mul(1024),
        packed_kib.ok_or(FAILED)?.saturating_mul(1024),
    ))
}

pub fn registered_paths(settings: &Settings) -> Vec<PathBuf> {
    repository_paths::registered_candidates(settings)
        .into_iter()
        .map(|candidate| candidate.path)
        .collect()
}

fn randomize<T: Hash>(items: &mut Vec<T>) {
    let random = RandomState::new();
    let mut keyed: Vec<_> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let mut hasher = random.build_hasher();
            index.hash(&mut hasher);
            item.hash(&mut hasher);
            (hasher.finish(), index)
        })
        .collect();
    keyed.sort_unstable_by_key(|(key, _)| *key);
    let mut values: Vec<Option<T>> = std::mem::take(items).into_iter().map(Some).collect();
    for (_, source) in keyed {
        if let Some(value) = values[source].take() {
            items.push(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{atomic::AtomicBool, Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    async fn run_git(repo: &Path, command: &[&str]) {
        let path = repo.to_string_lossy();
        let mut args = vec!["-C", path.as_ref()];
        args.extend_from_slice(command);
        let request = git::Request {
            args: &args,
            context: "diagnostics-scale-fixture",
            timeout: Duration::from_secs(30),
            expected: &[0],
            policy: git::OutputPolicy::Metadata,
        };
        let result = git::execute(request, None).await.unwrap();
        assert_eq!(result.code, Some(0));
    }

    #[tokio::test]
    async fn scale_counts_fixture_data_and_exports_only_repo_indices() {
        let _serial = crate::test_support::serial().await;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let fixture = crate::test_support::tmp_root().join(format!("diagnostics-scale-{stamp}"));
        let repo = fixture.join("acme-secret-repo");
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init", "--initial-branch=main"]).await;
        run_git(&repo, &["config", "user.name", "Fixture User"]).await;
        run_git(&repo, &["config", "user.email", "fixture@example.test"]).await;
        std::fs::write(repo.join("private-file.txt"), "fixture").unwrap();
        run_git(&repo, &["add", "private-file.txt"]).await;
        run_git(&repo, &["commit", "-m", "fixture"]).await;
        let settings = Settings {
            sources: Vec::new(),
            workspace: serde_json::json!({
                "root": fixture,
                "layout": "flat",
                "sets": [{"id":"set-id", "name":"ClientCorp", "items":[{
                    "id":"repo-id", "name":"acme-secret-repo", "folder":"acme-secret-repo",
                    "org":"AcmeOwner", "url":"https://github.example/AcmeOwner/acme-secret-repo.git",
                    "ref":{"type":"branch","name":"main"}
                }]}]
            }),
        };
        let progress = Arc::new(Mutex::new(Vec::new()));
        let progress_sink = progress.clone();
        let cancellation = Arc::new(AtomicBool::new(false));
        let scale = collect(
            &settings,
            cancellation,
            Arc::new(move |completed, total| {
                progress_sink.lock().unwrap().push((completed, total));
            }),
        )
        .await
        .unwrap();
        assert_eq!(scale.set_count, 1);
        assert_eq!(scale.repos_per_set, [1]);
        assert_eq!(scale.repos.len(), 1);
        assert_eq!(scale.repos[0].tracked_file_count, 1);
        assert_eq!(scale.repos[0].ref_count, 1);
        assert_eq!(scale.repos[0].repo, "repo-1");
        assert!(!scale.repos[0].worktree);
        assert_eq!(
            progress.lock().unwrap().as_slice(),
            [(1, 3), (2, 3), (3, 3)]
        );
        let cancellation = Arc::new(AtomicBool::new(false));
        let cancelled = cancellation.clone();
        let result = collect(
            &settings,
            cancellation,
            Arc::new(move |completed, _| {
                if completed == 1 {
                    cancelled.store(true, Ordering::Relaxed);
                }
            }),
        )
        .await;
        assert_eq!(result.err(), Some(CANCELLED));
        let document = serde_json::to_value(&scale).unwrap();
        let mut strings = Vec::new();
        collect_strings(&document, &mut strings);
        assert_eq!(strings, ["repo-1"]);
        std::fs::remove_dir_all(fixture).unwrap();
    }

    fn collect_strings<'a>(value: &'a serde_json::Value, result: &mut Vec<&'a str>) {
        match value {
            serde_json::Value::String(value) => result.push(value),
            serde_json::Value::Array(values) => values
                .iter()
                .for_each(|value| collect_strings(value, result)),
            serde_json::Value::Object(values) => values
                .values()
                .for_each(|value| collect_strings(value, result)),
            _ => {}
        }
    }
}
