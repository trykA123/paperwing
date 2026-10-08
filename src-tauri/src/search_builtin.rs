use crate::search::{Plan, RepoStatus, RepoTarget, State};
use crate::search_engine::{Budget, RepoResult, RepoSearch, SearchEngine};
use crate::search_pattern::matcher;
use crate::search_rows::{build_matches, window, Row};
use grep_matcher::Matcher;
use grep_regex::RegexMatcher;
use grep_searcher::{Searcher, SearcherBuilder, Sink, SinkContext, SinkMatch};
use std::io::{self, BufRead, BufReader, Read};
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct BuiltIn;

impl SearchEngine for BuiltIn {
    fn search<'a>(
        &'a self,
        search: RepoSearch<'a>,
    ) -> Pin<Box<dyn std::future::Future<Output = RepoResult> + Send + 'a>> {
        Box::pin(async move {
            let (files, attributes) = match list_files(&search).await {
                Ok(files) => files,
                Err(error) => return failed(error, &search.cancel),
            };
            if attributes {
                let mut result = crate::search_engine::GitGrep.search(search).await;
                result.status.engine_note =
                    Some("Using Git grep: Git attributes require Git grep.".into());
                return result;
            }
            let target = search.target.clone();
            let plan = search.plan.clone();
            let cancel = search.cancel.clone();
            let budget = search.budget;
            tauri::async_runtime::spawn_blocking(move || {
                scan(Scan {
                    target: &target,
                    plan: &plan,
                    files,
                    budget,
                    cancel: &cancel,
                })
            })
            .await
            .unwrap_or_else(|_| {
                failed(
                    "Built-in search stopped unexpectedly".into(),
                    &search.cancel,
                )
            })
        })
    }
}

async fn list_files(search: &RepoSearch<'_>) -> Result<(Vec<std::path::PathBuf>, bool), String> {
    let files = crate::search_files::list(
        crate::search_files::FilesRequest {
            root: &search.target.path,
            pathspecs: &search.plan.pathspecs,
            untracked: search.plan.untracked,
        },
        search.cancel.clone(),
    )
    .await?;
    let root = search.target.path.clone();
    let cancel = search.cancel.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let attributes = crate::search_attributes::requires_git(Path::new(&root), &files, &cancel);
        (files, attributes)
    })
    .await
    .map_err(|_| "Could not inspect search attributes".into())
}

fn failed(error: String, cancel: &AtomicBool) -> RepoResult {
    let state = if cancel.load(Ordering::Relaxed) {
        State::Cancelled
    } else {
        State::Failed
    };
    RepoResult {
        status: RepoStatus::without_matches(state, Some(crate::git::safe(&error))),
        matches: Vec::new(),
    }
}

struct Scan<'a> {
    target: &'a RepoTarget,
    plan: &'a Plan,
    files: Vec<std::path::PathBuf>,
    budget: Arc<Budget>,
    cancel: &'a AtomicBool,
}

fn scan(search: Scan<'_>) -> RepoResult {
    let Scan {
        target,
        plan,
        files,
        budget,
        cancel,
    } = search;
    let files = if plan.untracked {
        match crate::search_walk::collect(&target.path, files, cancel) {
            Ok(files) => files,
            Err(error) => return failed(error, cancel),
        }
    } else {
        files
    };
    let matcher = match matcher(plan) {
        Ok(matcher) => matcher,
        Err(error) => return failed(error, cancel),
    };
    let mut sink = Results {
        plan,
        matcher: &matcher,
        budget: &budget,
        cancel,
        path: String::new(),
        rows: Vec::new(),
        kept: 0,
        truncated: false,
        stopped: false,
    };
    let root = match crate::platform::canonical_path(Path::new(&target.path)) {
        Ok(root) => root,
        Err(error) => return failed(error.to_string(), cancel),
    };
    for relative in files {
        if cancel.load(Ordering::Relaxed) || sink.stopped {
            break;
        }
        let full = root.join(&relative);
        if !plain_file(&full, &root) {
            continue;
        }
        sink.path = relative
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/");
        if let Err(error) = scan_file(&full, &mut sink) {
            return failed(error.to_string(), cancel);
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return failed("Search cancelled".into(), cancel);
    }
    let matches = build_matches(sink.rows, plan.context);
    RepoResult {
        status: RepoStatus {
            state: State::Done,
            matches: matches.len(),
            truncated: sink.truncated || budget.is_capped(),
            error: None,
            engine_note: None,
        },
        matches,
    }
}

fn plain_file(path: &Path, root: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
        && crate::platform::canonical_path(path).is_ok_and(|path| path.starts_with(root))
}

fn scan_file(path: &Path, sink: &mut Results<'_>) -> io::Result<()> {
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::with_capacity(8000, file);
    if reader.fill_buf()?.contains(&0) {
        return Ok(());
    }
    scan_reader(reader, sink)
}

fn scan_reader(reader: impl Read, sink: &mut Results<'_>) -> io::Result<()> {
    let reader = Cancellable {
        reader,
        cancel: sink.cancel,
        budget: sink.budget,
    };
    let mut searcher = SearcherBuilder::new()
        .line_number(true)
        .before_context(usize::from(sink.plan.context))
        .after_context(usize::from(sink.plan.context))
        .build();
    searcher.search_reader(sink.matcher, reader, sink)
}

struct Cancellable<'a, R> {
    reader: R,
    cancel: &'a AtomicBool,
    budget: &'a Budget,
}

impl<R: Read> Read for Cancellable<'_, R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) || self.budget.is_capped() {
            return Ok(0);
        }
        self.reader.read(bytes)
    }
}

struct Results<'a> {
    plan: &'a Plan,
    matcher: &'a RegexMatcher,
    budget: &'a Budget,
    cancel: &'a AtomicBool,
    path: String,
    rows: Vec<Row>,
    kept: usize,
    truncated: bool,
    stopped: bool,
}

fn line(bytes: &[u8]) -> &[u8] {
    bytes.strip_suffix(b"\n").unwrap_or(bytes)
}

impl Sink for Results<'_> {
    type Error = io::Error;

    fn matched(&mut self, _: &Searcher, found: &SinkMatch<'_>) -> io::Result<bool> {
        if self.cancel.load(Ordering::Relaxed) {
            return Ok(false);
        }
        if found.bytes().len() > 2 * 1024 * 1024 {
            self.truncated = true;
            return Ok(true);
        }
        if self.kept >= self.plan.per_repo || !self.budget.claim() {
            self.truncated = true;
            self.stopped = true;
            return Ok(false);
        }
        let raw = line(found.bytes());
        let column = self
            .matcher
            .find(raw)
            .map_err(io::Error::other)?
            .map_or(1, |found| found.start() + 1);
        let (text, column) = window(raw, column);
        self.rows.push(Row::Match(crate::search::Match {
            path: self.path.clone(),
            line: found.line_number().unwrap_or(1) as u32,
            column,
            text,
            context: Vec::new(),
        }));
        self.kept += 1;
        Ok(true)
    }

    fn context(&mut self, _: &Searcher, found: &SinkContext<'_>) -> io::Result<bool> {
        if self.cancel.load(Ordering::Relaxed) {
            return Ok(false);
        }
        self.rows.push(Row::Context {
            path: self.path.clone(),
            line: found.line_number().unwrap_or(1) as u32,
            text: window(line(found.bytes()), 1).0,
        });
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CancellingReader<'a> {
        cancel: &'a AtomicBool,
        bytes: &'a [u8],
        read: bool,
    }

    impl Read for CancellingReader<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            assert!(!self.read, "cancelled search must not read again");
            self.read = true;
            self.cancel.store(true, Ordering::Relaxed);
            self.bytes.read(buffer)
        }
    }

    #[test]
    fn cancelling_during_a_file_read_stops_the_searcher() {
        let cancel = AtomicBool::new(false);
        let plan = crate::search::plan(&crate::search::SearchRequest {
            pattern: "hit".into(),
            repos: vec![RepoTarget {
                path: "/unused".into(),
                git_ref: None,
            }],
            ..Default::default()
        })
        .unwrap();
        let matcher = matcher(&plan).unwrap();
        let budget = Budget::new(plan.overall);
        let mut sink = Results {
            plan: &plan,
            matcher: &matcher,
            budget: &budget,
            cancel: &cancel,
            path: "a.txt".into(),
            rows: Vec::new(),
            kept: 0,
            truncated: false,
            stopped: false,
        };
        scan_reader(
            CancellingReader {
                cancel: &cancel,
                bytes: b"hit\nhit\n",
                read: false,
            },
            &mut sink,
        )
        .unwrap();
        assert!(sink.rows.is_empty());
        assert_eq!(
            failed("Search cancelled".into(), &cancel).status.state,
            State::Cancelled
        );
    }
}
