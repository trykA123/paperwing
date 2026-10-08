use crate::finder::{validate, FileMatch, FileMatcher, FinderRequest};
use crate::search::{dedupe, RepoTarget};
use crate::settings::{FinderMatching, SearchFiles, SearchOptions};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub const MATCHES_EVENT: &str = "finder-matches";
pub const DONE_EVENT: &str = "finder-done";
const CHUNK: usize = 128;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MatchesPayload {
    pub id: u64,
    pub sequence: u64,
    pub matches: Vec<FileMatch>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RepoError {
    pub repo: String,
    pub error: String,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub scanned: usize,
    pub matches: usize,
    pub cancelled: bool,
    pub errors: Vec<RepoError>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DonePayload {
    pub id: u64,
    pub summary: Summary,
}

#[derive(Clone, Debug)]
pub enum Outbound {
    Matches(MatchesPayload),
    Done(DonePayload),
}

pub type Emit = Arc<dyn Fn(Outbound) + Send + Sync>;
pub struct Job {
    pub id: u64,
    pub cancel: Arc<AtomicBool>,
    pub send: Emit,
    pub options: SearchOptions,
}

struct Top {
    id: u64,
    limit: usize,
    sequence: u64,
    scanned: usize,
    hits: usize,
    matches: Vec<FileMatch>,
    send: Emit,
}

impl Top {
    fn accept(&mut self, found: Option<FileMatch>) {
        self.scanned += 1;
        if let Some(found) = found {
            self.hits += 1;
            let at = self
                .matches
                .binary_search_by(|other| {
                    found
                        .score
                        .cmp(&other.score)
                        .then_with(|| other.repo.cmp(&found.repo))
                        .then_with(|| other.path.cmp(&found.path))
                })
                .unwrap_or_else(|at| at);
            if at < self.limit {
                self.matches.insert(at, found);
                self.matches.truncate(self.limit);
            }
        }
        if (self.sequence == 0 && !self.matches.is_empty()) || self.scanned.is_multiple_of(CHUNK) {
            self.emit();
        }
    }

    fn emit(&mut self) {
        self.sequence += 1;
        (self.send)(Outbound::Matches(MatchesPayload {
            id: self.id,
            sequence: self.sequence,
            matches: self.matches.clone(),
        }));
    }
}

struct Completion {
    id: u64,
    send: Emit,
    sent: bool,
}

impl Completion {
    fn finish(&mut self, summary: Summary) {
        self.sent = true;
        (self.send)(Outbound::Done(DonePayload {
            id: self.id,
            summary,
        }));
    }
}

impl Drop for Completion {
    fn drop(&mut self) {
        if !self.sent {
            self.finish(Summary {
                cancelled: true,
                errors: vec![RepoError {
                    repo: String::new(),
                    error: "File finder stopped unexpectedly".into(),
                }],
                ..Summary::default()
            });
        }
    }
}

pub async fn run_job(request: FinderRequest, job: Job) -> Result<Summary, String> {
    #[cfg(feature = "benchmark")]
    let _timing = crate::benchmark::Span::new("ipc.files", "find-file");
    let limit = validate(&request)?;
    let mut completion = Completion {
        id: job.id,
        send: job.send.clone(),
        sent: false,
    };
    let repos = repositories(request.repos).await?;
    let top = Arc::new(Mutex::new(Top {
        id: job.id,
        limit,
        sequence: 0,
        scanned: 0,
        hits: 0,
        matches: Vec::new(),
        send: job.send.clone(),
    }));
    let mut errors = Vec::new();
    for target in repos {
        if job.cancel.load(Ordering::Relaxed) {
            break;
        }
        if let Err(error) = search_repo(&target.path, &request.query, &job, top.clone()).await {
            if !job.cancel.load(Ordering::Relaxed) {
                errors.push(RepoError {
                    repo: target.path,
                    error: crate::git::safe(&error),
                });
            }
        }
    }
    let summary = finish_results(&top, &job.cancel, errors)?;
    completion.finish(summary.clone());
    Ok(summary)
}

async fn repositories(paths: Vec<String>) -> Result<Vec<RepoTarget>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        dedupe(
            paths
                .into_iter()
                .map(|path| RepoTarget {
                    path,
                    git_ref: None,
                })
                .collect(),
        )
    })
    .await
    .map_err(|_| "Could not read file finder repository paths".into())
}

fn finish_results(
    top: &Mutex<Top>,
    cancel: &AtomicBool,
    errors: Vec<RepoError>,
) -> Result<Summary, String> {
    let mut top = top
        .lock()
        .map_err(|_| "File finder results are unavailable")?;
    top.emit();
    Ok(Summary {
        scanned: top.scanned,
        matches: top.hits,
        cancelled: cancel.load(Ordering::Relaxed),
        errors,
    })
}

async fn search_repo(
    root: &str,
    query: &str,
    job: &Job,
    top: Arc<Mutex<Top>>,
) -> Result<(), String> {
    let path = root.to_string();
    tauri::async_runtime::spawn_blocking(move || crate::git::valid_root(&path))
        .await
        .map_err(|_| "Finder repository validation failed")??;
    if job.cancel.load(Ordering::Relaxed) {
        return Ok(());
    }
    let untracked = job.options.search_files == SearchFiles::TrackedAndUntracked;
    let files = crate::search_files::list(
        crate::search_files::FilesRequest {
            root,
            pathspecs: &[],
            untracked,
        },
        job.cancel.clone(),
    )
    .await?;
    let (root, query, cancel, matching) = (
        root.to_string(),
        query.to_string(),
        job.cancel.clone(),
        job.options.finder_matching,
    );
    tauri::async_runtime::spawn_blocking(move || {
        scan(Scan {
            root,
            query,
            files,
            cancel,
            matching,
            top,
            untracked,
        })
    })
    .await
    .map_err(|_| "File finder worker stopped unexpectedly")?
}

struct Scan {
    root: String,
    query: String,
    files: Vec<std::path::PathBuf>,
    cancel: Arc<AtomicBool>,
    matching: FinderMatching,
    top: Arc<Mutex<Top>>,
    untracked: bool,
}

fn scan(scan: Scan) -> Result<(), String> {
    let Scan {
        root,
        query,
        files,
        cancel,
        matching,
        top,
        untracked,
    } = scan;
    crate::search_walk::visit(
        crate::search_walk::WalkRequest {
            root: &root,
            files,
            cancel: &cancel,
            exclude_ignored: untracked,
        },
        || {
            let mut matcher = FileMatcher::new(&query, matching);
            let root = &root;
            let top = &top;
            let cancel = &cancel;
            Box::new(move |path| {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let path = path
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/");
                let found = matcher.find(root, path);
                let mut top = top.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                if !cancel.load(Ordering::Relaxed) {
                    top.accept(found);
                }
            })
        },
    )
}

#[cfg(test)]
#[path = "finder_tests.rs"]
mod tests;
