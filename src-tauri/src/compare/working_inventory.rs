use super::{
    inventory::{fingerprint, Entry, Kind, Resolved},
    object_id::ObjectFormat,
    Job, Problem,
};
use crate::paths;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

type Index = Arc<HashMap<String, (String, String)>>;

pub(super) async fn read(
    records: Vec<String>,
    index: Index,
    resolved: &Resolved,
    job: &Job,
) -> Result<BTreeMap<String, Entry>, Problem> {
    let mut entries = BTreeMap::new();
    job.check()?;
    let mut tasks = tokio::task::JoinSet::new();
    for paths in records.chunks(records.len().div_ceil(4).max(1)) {
        let (paths, index, safe, job) = (
            paths.to_vec(),
            index.clone(),
            resolved.safe.clone(),
            job.clone(),
        );
        let worker = Worker {
            index,
            safe,
            format: resolved.object_format,
            job,
        };
        tasks.spawn_blocking(move || read_chunk(paths, worker));
    }
    let mut failure = None;
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(files)) => entries.extend(files),
            Ok(Err(problem)) => failure = Some(problem),
            Err(_) => failure = Some(Problem::new("unavailable", "Working inventory task failed")),
        }
    }
    if let Some(problem) = failure {
        return Err(problem);
    }
    Ok(entries)
}

struct Worker {
    index: Index,
    safe: paths::ReadRoot,
    format: ObjectFormat,
    job: Job,
}

fn read_chunk(records: Vec<String>, worker: Worker) -> Result<BTreeMap<String, Entry>, Problem> {
    let mut cache = paths::ReadCache::default();
    let mut entries = BTreeMap::new();
    for path in records {
        worker.job.check()?;
        if entries.contains_key(&path) {
            continue;
        }
        if let Some((path, entry)) = worker.read_entry(path, &mut cache) {
            entries.insert(path, entry);
        }
    }
    Ok(entries)
}

impl Worker {
    fn read_entry(&self, path: String, cache: &mut paths::ReadCache) -> Option<(String, Entry)> {
        if path.ends_with('/')
            || self
                .index
                .get(&path)
                .is_some_and(|(mode, _)| mode == "160000")
        {
            return self.read_gitlink(path, cache);
        }
        let result = self.safe.read_cached(&path, cache);
        #[cfg(test)]
        if let Some(started) = &self.job.inventory_started {
            started.notify_one();
        }
        let entry = match result {
            Ok(Some(file)) => self.file_entry(&path, file),
            Ok(None) => return None,
            Err(reason) => failed_entry(reason),
        };
        Some((path, entry))
    }

    fn read_gitlink(&self, path: String, cache: &mut paths::ReadCache) -> Option<(String, Entry)> {
        let untracked = path.ends_with('/');
        let path = path.trim_end_matches('/').to_owned();
        let resolved = self.safe.resolve_cached(&path, false, cache);
        let reason = resolved.as_ref().err().cloned();
        if !untracked && !resolved.is_ok_and(|path| path.exists()) && reason.is_none() {
            return None;
        }
        let oid = if untracked {
            None
        } else {
            Some(self.index[&path].1.clone())
        };
        Some((
            path,
            Entry {
                kind: Kind::Gitlink,
                blob_id: oid.clone(),
                oid,
                size: None,
                mode: "160000".into(),
                modified_ms: None,
                reason,
                fingerprint: None,
                source: if untracked {
                    "untrackedRepository"
                } else {
                    "indexGitlink"
                }
                .into(),
            },
        ))
    }

    fn file_entry(&self, path: &str, file: paths::FileBytes) -> Entry {
        let link = file.symlink
            || self
                .index
                .get(path)
                .is_some_and(|(mode, _)| mode == "120000");
        let mode = if link {
            "120000".into()
        } else {
            self.index
                .get(path)
                .map(|(mode, _)| mode.clone())
                .unwrap_or_else(|| "100644".into())
        };
        let blob_id = self.format.blob(&file.bytes);
        let oid = self
            .index
            .get(path)
            .filter(|(_, oid)| *oid == blob_id)
            .map(|(_, oid)| oid.clone());
        Entry {
            kind: if link { Kind::Symlink } else { Kind::File },
            oid,
            blob_id: Some(blob_id),
            size: Some(file.bytes.len() as u64),
            mode,
            modified_ms: file.modified_ms,
            reason: None,
            fingerprint: Some(fingerprint(&file.bytes)),
            source: "workingTree".into(),
        }
    }
}

fn failed_entry(reason: String) -> Entry {
    Entry {
        kind: Kind::File,
        oid: None,
        blob_id: None,
        size: None,
        mode: "100644".into(),
        modified_ms: None,
        reason: Some(reason),
        fingerprint: None,
        source: "workingTree".into(),
    }
}
