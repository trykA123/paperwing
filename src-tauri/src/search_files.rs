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
        while let Some(end) = self.pending.iter().position(|byte| *byte == 0) {
            let row: Vec<u8> = self.pending.drain(..=end).collect();
            if let Err(error) = self.accept(&row[..end]) {
                self.error = Some(error);
                return true;
            }
        }
        false
    }

    fn accept(&mut self, row: &[u8]) -> Result<(), String> {
        let at = row
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or("Invalid Git index listing")?;
        let metadata = std::str::from_utf8(&row[..at]).map_err(|_| "Invalid Git index entry")?;
        let mode = metadata
            .split_whitespace()
            .next()
            .ok_or("Missing Git file mode")?;
        if matches!(mode, "120000" | "160000") {
            return Ok(());
        }
        let path = relative_path(&row[at + 1..])?;
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

pub async fn tracked(
    path: &str,
    pathspecs: &[String],
    cancel: Arc<AtomicBool>,
) -> Result<Vec<PathBuf>, String> {
    let mut args = vec!["ls-files", "--cached", "--stage", "-z", "--"];
    args.extend(pathspecs.iter().map(String::as_str));
    let args = RepoGit::at(path).no_optional_locks().argv(&args);
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
