use crate::git::{execute_cancellable, execute_streaming, OutputPolicy, Request, StdoutSink};
use crate::search::{Match, Plan, RepoStatus, RepoTarget, State};
use crate::search_rows::{build_matches, parse_row, Row};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::OnceCell;

const GREP_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_ROW: usize = 2 * 1024 * 1024;

static PERL: OnceCell<bool> = OnceCell::const_new();
static HINT: OnceCell<bool> = OnceCell::const_new();

pub struct Budget {
    claimed: AtomicUsize,
    capped: AtomicBool,
    overall: usize,
}

impl Budget {
    pub fn new(overall: usize) -> Self {
        Self {
            claimed: AtomicUsize::new(0),
            capped: AtomicBool::new(false),
            overall,
        }
    }

    pub fn is_capped(&self) -> bool {
        self.capped.load(Ordering::SeqCst)
    }

    fn claim(&self) -> bool {
        let fits = self.claimed.fetch_add(1, Ordering::SeqCst) < self.overall;
        if !fits {
            self.capped.store(true, Ordering::SeqCst);
        }
        fits
    }
}

pub struct RepoResult {
    pub status: RepoStatus,
    pub matches: Vec<Match>,
}

#[derive(Default)]
struct Parse {
    buffer: Vec<u8>,
    skipping: bool,
    scanned: usize,
    rows: Vec<Row>,
    kept: usize,
    truncated: bool,
    stopped: bool,
}

struct RowSink {
    prefix: Option<String>,
    context: bool,
    per_repo: usize,
    budget: Arc<Budget>,
    state: Mutex<Parse>,
}

impl RowSink {
    fn feed(&self, chunk: &[u8]) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.budget.is_capped() {
            state.truncated = true;
            state.stopped = true;
            return true;
        }
        state.buffer.extend_from_slice(chunk);
        while let Some(at) = state.buffer[state.scanned..]
            .iter()
            .position(|byte| *byte == b'\n')
        {
            let end = state.scanned + at;
            state.scanned = 0;
            let row: Vec<u8> = state.buffer.drain(..=end).collect();
            if std::mem::take(&mut state.skipping) {
                continue;
            }
            if self.accept(&mut state, &row[..row.len() - 1]) {
                return true;
            }
        }
        state.scanned = state.buffer.len();
        if state.buffer.len() > MAX_ROW {
            state.buffer.clear();
            state.scanned = 0;
            state.skipping = true;
            state.truncated = true;
        }
        false
    }

    fn accept(&self, state: &mut Parse, row: &[u8]) -> bool {
        match parse_row(row, self.prefix.as_deref(), self.context) {
            Some(Row::Match(_)) if state.kept >= self.per_repo || !self.budget.claim() => {
                state.truncated = true;
                state.stopped = true;
                true
            }
            Some(parsed) => {
                state.kept += usize::from(matches!(parsed, Row::Match(_)));
                state.rows.push(parsed);
                false
            }
            None => false,
        }
    }
}

pub async fn perl_supported() -> bool {
    *PERL.get_or_init(detect_perl).await
}

async fn detect_perl() -> bool {
    let dir = std::env::temp_dir();
    let Some(dir) = dir.to_str() else {
        return false;
    };
    let args = [
        "-C",
        dir,
        "grep",
        "--no-index",
        "-P",
        "-q",
        "-e",
        "x",
        "--",
        ".skein-missing",
    ];
    let request = Request {
        args: &args,
        context: "Check Perl regex support",
        timeout: Duration::from_secs(10),
        expected: &[0, 1, 128],
        policy: OutputPolicy::Metadata,
    };
    let output = execute_cancellable(request, Arc::new(AtomicBool::new(false))).await;
    output.is_ok_and(|output| matches!(output.code, Some(0 | 1)))
}

async fn per_file_hint() -> bool {
    *HINT.get_or_init(detect_hint).await
}

async fn detect_hint() -> bool {
    let request = Request {
        args: &["--version"],
        context: "Check Git version",
        timeout: Duration::from_secs(10),
        expected: &[0],
        policy: OutputPolicy::Text,
    };
    let Ok(output) = execute_cancellable(request, Arc::new(AtomicBool::new(false))).await else {
        return false;
    };
    version_allows_hint(&String::from_utf8_lossy(&output.stdout))
}

pub fn version_allows_hint(text: &str) -> bool {
    let mut numbers = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u32>().ok());
    matches!((numbers.next(), numbers.next()), (Some(major), Some(minor)) if (major, minor) >= (2, 38))
}

fn build_argv(target: &RepoTarget, plan: &Plan, hint: bool) -> Vec<String> {
    let mut argv: Vec<String> = ["-C", &target.path, "-c", "core.fsmonitor=false"]
        .map(String::from)
        .to_vec();
    argv.extend(["grep", "-z", "-I", "-n", "--column", "--no-color"].map(String::from));
    argv.extend(plan.flags.iter().map(|flag| flag.to_string()));
    if plan.context > 0 {
        argv.push(format!("-C{}", plan.context));
    }
    if plan.untracked {
        argv.extend(["--untracked", "--exclude-standard"].map(String::from));
    }
    if hint {
        argv.extend(["-m".into(), (plan.per_repo + 1).to_string()]);
    }
    argv.extend(["-e".into(), plan.pattern.clone()]);
    argv.extend(target.git_ref.clone());
    argv.push("--".into());
    argv.extend(plan.pathspecs.iter().cloned());
    argv
}

pub async fn search_repo(
    target: &RepoTarget,
    plan: &Plan,
    budget: Arc<Budget>,
    cancel: Arc<AtomicBool>,
) -> RepoResult {
    let sink = Arc::new(RowSink {
        prefix: target.git_ref.as_ref().map(|name| format!("{name}:")),
        context: plan.context > 0,
        per_repo: plan.per_repo,
        budget,
        state: Mutex::default(),
    });
    let argv = build_argv(target, plan, per_file_hint().await);
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let request = Request {
        args: &args,
        context: "Search repository",
        timeout: GREP_TIMEOUT,
        expected: &[0, 1],
        policy: OutputPolicy::Metadata,
    };
    let reader = sink.clone();
    let stream: StdoutSink = Arc::new(move |chunk| reader.feed(chunk));
    let output = execute_streaming(request, cancel.clone(), stream).await;
    let failure = match output {
        Ok(output) if sink_stopped(&sink) || matches!(output.code, Some(0 | 1)) => None,
        Ok(output) => Some(output.last_error()),
        Err(error) => Some(error),
    };
    if let Some(error) = failure {
        let cancelled = cancel.load(Ordering::Relaxed);
        let state = if cancelled {
            State::Cancelled
        } else {
            State::Failed
        };
        return RepoResult {
            status: RepoStatus::without_matches(state, Some(error)),
            matches: Vec::new(),
        };
    }
    let context = plan.context;
    let task = tauri::async_runtime::spawn_blocking(move || finish(&sink, context));
    task.await.unwrap_or_else(|_| RepoResult {
        status: RepoStatus::without_matches(
            State::Failed,
            Some("Search result handling failed".into()),
        ),
        matches: Vec::new(),
    })
}

fn sink_stopped(sink: &RowSink) -> bool {
    sink.state.lock().is_ok_and(|state| state.stopped)
}

fn finish(sink: &RowSink, context: u8) -> RepoResult {
    let mut state = sink
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let leftover = std::mem::take(&mut state.buffer);
    if !leftover.is_empty() && !state.skipping && !state.stopped {
        sink.accept(&mut state, &leftover);
    }
    let matches = build_matches(std::mem::take(&mut state.rows), context);
    let status = RepoStatus {
        state: State::Done,
        matches: matches.len(),
        truncated: state.truncated,
        error: None,
    };
    RepoResult { status, matches }
}
