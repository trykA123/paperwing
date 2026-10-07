use super::{run, run_with_env};
use crate::git::OutputPolicy;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(super) async fn refuse_filters(path: &str, file: &str) -> Result<(), String> {
    let output = run(
        path,
        &["check-attr", "-z", "filter", "--", file],
        "Check partial staging filters",
        &[0],
        OutputPolicy::Metadata,
        None,
        Duration::from_secs(45),
    )
    .await?;
    let fields: Vec<_> = output.stdout.split(|byte| *byte == 0).collect();
    match fields.as_slice() {
        [_, b"filter", b"unspecified" | b"unset", b""] => Ok(()),
        [_, b"filter", _, b""] => {
            Err("Files with a filter attribute do not support hunk staging or discard".into())
        }
        _ => Err("Could not read the file's filter attribute".into()),
    }
}

async fn text_attribute(path: &str, file: &str) -> Result<String, String> {
    let output = run(
        path,
        &["check-attr", "-z", "text", "--", file],
        "Check text attribute",
        &[0],
        OutputPolicy::Metadata,
        None,
        Duration::from_secs(45),
    )
    .await?;
    let fields: Vec<_> = output.stdout.split(|byte| *byte == 0).collect();
    match fields.as_slice() {
        [_, b"text", value, b""] => Ok(String::from_utf8_lossy(value).into_owned()),
        _ => Ok("unspecified".into()),
    }
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

fn temp_root() -> PathBuf {
    std::env::var_os("SKEIN_TEST_TMP")
        .or_else(|| std::env::var_os("PAPERWING_TEST_TMP"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
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
    let temp_dir = TempObjectDir::new()?;
    let temp_dir_str = temp_dir
        .path
        .to_str()
        .ok_or("Invalid temporary object path")?;

    let envs = [
        ("GIT_OBJECT_DIRECTORY", temp_dir_str),
        ("GIT_ALTERNATE_OBJECT_DIRECTORIES", repo_objects_str),
    ];

    let no_autocrlf = if let Some(index) = index_bytes {
        if index.contains(&b'\r') {
            let attr = text_attribute(path, file)
                .await
                .unwrap_or_else(|_| "unspecified".into());
            attr == "unspecified" || attr == "auto"
        } else {
            false
        }
    } else {
        false
    };

    let file_arg = format!("--path={file}");
    let mut args = Vec::new();
    if no_autocrlf {
        args.push("-c");
        args.push("core.autocrlf=false");
    }
    args.extend_from_slice(&["hash-object", "-w", "--stdin", &file_arg]);

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
