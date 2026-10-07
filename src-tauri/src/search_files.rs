use crate::git::repo_command::RepoGit;
use crate::git::{execute_streaming, OutputPolicy, Request, StdoutSink};
use std::collections::BTreeSet;
use std::path::{Component, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
struct Listing {
    pending: Vec<u8>,
    paths: BTreeSet<PathBuf>,
    error: Option<String>,
}

impl Listing {
    fn feed(&mut self, bytes: &[u8]) -> bool {
        self.pending.extend_from_slice(bytes);
        let Some(end) = self.pending.iter().rposition(|byte| *byte == 0) else {
            return false;
        };
        let rows: Vec<u8> = self.pending.drain(..=end).collect();
        for row in rows[..end].split(|byte| *byte == 0) {
            if let Err(error) = self.accept(row) {
                self.error = Some(error);
                return true;
            }
        }
        false
    }

    fn accept(&mut self, row: &[u8]) -> Result<(), String> {
        if row.get(1) != Some(&b' ') {
            return Err("Invalid Git file listing tag".into());
        }
        let untracked = row[0] == b'?';
        let row = &row[2..];
        let path = if row.is_empty() {
            return Err("Empty Git file listing row".into());
        } else if !untracked {
            let at = row
                .iter()
                .position(|byte| *byte == b'\t')
                .ok_or("Invalid Git index listing")?;
            let metadata =
                std::str::from_utf8(&row[..at]).map_err(|_| "Invalid Git index entry")?;
            let mode = metadata
                .split_whitespace()
                .next()
                .ok_or("Missing Git file mode")?;
            if matches!(mode, "120000" | "160000") {
                return Ok(());
            }
            relative_path(&row[at + 1..])?
        } else {
            relative_path(row)?
        };
        self.paths.insert(path);
        Ok(())
    }
}

fn relative_path(bytes: &[u8]) -> Result<PathBuf, String> {
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_vec(bytes.to_vec()))
    };
    #[cfg(not(unix))]
    let path = PathBuf::from(std::str::from_utf8(bytes).map_err(|_| "Git path is not UTF-8")?);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("Git returned an invalid relative path".into());
    }
    Ok(path)
}

pub struct FilesRequest<'a> {
    pub root: &'a str,
    pub pathspecs: &'a [String],
    pub untracked: bool,
}

pub async fn list(
    request: FilesRequest<'_>,
    cancel: Arc<AtomicBool>,
) -> Result<Vec<PathBuf>, String> {
    let mut args = vec!["ls-files", "--cached", "--stage", "-t", "-z"];
    if request.untracked {
        args.extend(["--others", "--exclude-standard"]);
    }
    args.push("--");
    args.extend(request.pathspecs.iter().map(String::as_str));
    let args = RepoGit::at(request.root).no_optional_locks().argv(&args);
    let listing = Arc::new(Mutex::new(Listing::default()));
    let state = listing.clone();
    let sink: StdoutSink = Arc::new(move |bytes| {
        state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .feed(bytes)
    });
    let output = execute_streaming(
        Request {
            args: &args,
            context: "Read search file list",
            timeout: Duration::from_secs(120),
            expected: &[0],
            policy: OutputPolicy::Metadata,
        },
        cancel,
        sink,
    )
    .await?;
    if output.code != Some(0) {
        return Err(output.last_error());
    }
    let mut listing = listing
        .lock()
        .map_err(|_| "Search file listing is unavailable")?;
    if let Some(error) = listing.error.take() {
        return Err(error);
    }
    if !listing.pending.is_empty() {
        return Err("Git returned an incomplete search file list".into());
    }
    Ok(std::mem::take(&mut listing.paths).into_iter().collect())
}
