use super::{run, run_with_env};
use crate::git::OutputPolicy;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(super) async fn refuse_filters(path: &str, file: &str) -> Result<(), String> {
    let output = run(
        path,
        &[
            "check-attr",
            "-z",
            "filter",
            "ident",
            "working-tree-encoding",
            "--",
            file,
        ],
        "Check partial staging filters",
        &[0],
        OutputPolicy::Metadata,
        None,
        Duration::from_secs(45),
    )
    .await?;
    let fields: Vec<_> = output.stdout.split(|byte| *byte == 0).collect();
    for chunk in fields.as_chunks::<3>().0 {
        let (name, value) = (chunk[1], chunk[2]);
        if value != b"unspecified" && value != b"unset" {
            let name_str = String::from_utf8_lossy(name);
            return Err(format!(
                "Files with a {name_str} attribute do not support hunk staging or discard"
            ));
        }
    }
    Ok(())
}

async fn text_and_eol_attributes(path: &str, file: &str) -> (String, String) {
    let output = run(
        path,
        &["check-attr", "-z", "text", "eol", "--", file],
        "Check text and eol attributes",
        &[0],
        OutputPolicy::Metadata,
        None,
        Duration::from_secs(45),
    )
    .await;
    let mut text = "unspecified".to_string();
    let mut eol = "unspecified".to_string();
    if let Ok(output) = output {
        let fields: Vec<_> = output.stdout.split(|byte| *byte == 0).collect();
        for chunk in fields.as_chunks::<3>().0 {
            match chunk[1] {
                b"text" => text = String::from_utf8_lossy(chunk[2]).to_string(),
                b"eol" => eol = String::from_utf8_lossy(chunk[2]).to_string(),
                _ => {}
            }
        }
    }
    (text, eol)
}

async fn repo_objects_dir(path: &str) -> Result<PathBuf, String> {
    let output = run(
        path,
        &["rev-parse", "--git-path", "objects"],
        "Find repository objects directory",
        &[0],
        OutputPolicy::Metadata,
        None,
        Duration::from_secs(45),
    )
    .await?;
    let raw = std::str::from_utf8(&output.stdout)
        .map_err(|_| "Invalid git objects path")?
        .trim();
    let objects_path = PathBuf::from(raw);
    let absolute = if objects_path.is_absolute() {
        objects_path
    } else {
        Path::new(path).join(objects_path)
    };
    Ok(absolute)
}

#[cfg(not(test))]
static TEMP_ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

#[cfg(not(test))]
pub(crate) fn configure_temp_root(path: PathBuf) {
    let _ = TEMP_ROOT.set(path);
}

fn temp_root() -> PathBuf {
    #[cfg(test)]
    {
        crate::test_support::tmp_root()
    }
    #[cfg(not(test))]
    {
        TEMP_ROOT
            .get()
            .cloned()
            .expect("App data temp root not configured")
    }
}

fn quote_alternate(path: &str) -> String {
    let mut quoted = String::with_capacity(path.len() + 2);
    quoted.push('"');
    for ch in path.chars() {
        match ch {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

struct TempObjectDir {
    path: PathBuf,
}

impl TempObjectDir {
    fn new() -> Result<Self, String> {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let pid = std::process::id();
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = temp_root().join(format!("skein-obj-{pid}-{time}-{id}"));
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Could not create temporary object directory: {e}"))?;
        Ok(Self { path: dir })
    }
}

impl Drop for TempObjectDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

pub(super) async fn clean(
    path: &str,
    file: &str,
    bytes: &[u8],
    index_bytes: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    let repo_objects = repo_objects_dir(path).await?;
    let repo_objects_str = repo_objects
        .to_str()
        .ok_or("Invalid repository objects path")?;
    let quoted_repo_objects = quote_alternate(repo_objects_str);
    let temp_dir = TempObjectDir::new()?;
    let temp_dir_str = temp_dir
        .path
        .to_str()
        .ok_or("Invalid temporary object path")?;

    let envs = [
        ("GIT_OBJECT_DIRECTORY", temp_dir_str),
        ("GIT_ALTERNATE_OBJECT_DIRECTORIES", &quoted_repo_objects),
    ];

    let use_no_filters = if let Some(index) = index_bytes {
        if index.contains(&b'\r') {
            let (text, eol) = text_and_eol_attributes(path, file).await;
            text == "auto" || (text == "unspecified" && eol == "unspecified")
        } else {
            false
        }
    } else {
        false
    };

    let file_arg = format!("--path={file}");
    let mut args = vec!["hash-object", "-w", "--stdin"];
    if use_no_filters {
        args.push("--no-filters");
    } else {
        args.push(&file_arg);
    }

    let output = run_with_env(
        path,
        &args,
        "Convert partial staging content",
        &[0],
        OutputPolicy::Metadata,
        Some(bytes),
        Duration::from_secs(45),
        &envs,
    )
    .await?;

    let oid = std::str::from_utf8(&output.stdout)
        .map_err(|_| "Invalid converted blob id")?
        .trim();
    if !crate::object_id::valid(oid) {
        return Err("Invalid converted blob id".into());
    }

    let cat_output = run_with_env(
        path,
        &["cat-file", "-p", oid],
        "Read converted blob",
        &[0],
        OutputPolicy::Metadata,
        None,
        Duration::from_secs(45),
        &envs,
    )
    .await?;

    Ok(cat_output.stdout)
}

pub(super) async fn smudge(path: &str, file: &str, spec: &str) -> Result<Vec<u8>, String> {
    let file_arg = format!("--path={file}");
    let output = run(
        path,
        &["cat-file", "--filters", &file_arg, spec],
        "Restore working-tree content",
        &[0],
        OutputPolicy::Metadata,
        None,
        Duration::from_secs(45),
    )
    .await?;
    if output.stdout.len() > crate::paths::CONTENT_LIMIT {
        return Err("The restored file is too large".into());
    }
    Ok(output.stdout)
}

pub(crate) fn sweep_stale_temp_dirs(temp_root: &Path) {
    let Ok(entries) = std::fs::read_dir(temp_root) else {
        return;
    };
    let cutoff = std::time::Duration::from_secs(3600);
    let now = std::time::SystemTime::now();

    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if !name_str.starts_with("skein-obj-") {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }

        let is_stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|mtime| now.duration_since(mtime).ok())
            .is_some_and(|elapsed| elapsed >= cutoff);

        if is_stale {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}
