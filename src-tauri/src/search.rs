use crate::git::{execute_cancellable, valid_ref, valid_root, OutputPolicy, Request};
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

pub const DEFAULT_PER_REPO: usize = 500;
pub const DEFAULT_OVERALL: usize = 5000;
pub const MAX_PER_REPO: usize = 5000;
pub const MAX_OVERALL: usize = 50_000;
pub const MAX_REPOS: usize = 500;
const MAX_PATTERN: usize = 4096;
const MAX_PATHSPECS: usize = 32;
const MAX_CONTEXT: u8 = 3;
const MAX_LINE_CHARS: usize = 400;
const GREP_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Deserialize, Clone, Copy, Default, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    Fixed,
    Basic,
    Perl,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RepoTarget {
    pub path: String,
    pub git_ref: Option<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchRequest {
    pub repos: Vec<RepoTarget>,
    pub pattern: String,
    pub mode: Mode,
    pub ignore_case: bool,
    pub whole_word: bool,
    pub pathspecs: Vec<String>,
    pub untracked: bool,
    pub context: u8,
    pub max_per_repo: Option<usize>,
    pub max_overall: Option<usize>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextLine {
    pub line: u32,
    pub text: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    pub repo: String,
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub text: String,
    pub context: Vec<ContextLine>,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum State {
    Done,
    Skipped,
    Cancelled,
    Failed,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RepoStatus {
    pub state: State,
    pub matches: usize,
    pub truncated: bool,
    pub error: Option<String>,
}

impl RepoStatus {
    pub fn without_matches(state: State, error: Option<String>) -> Self {
        Self {
            state,
            matches: 0,
            truncated: false,
            error,
        }
    }
}

pub struct Plan {
    pub pattern: String,
    pub flags: Vec<&'static str>,
    pub pathspecs: Vec<String>,
    pub untracked: bool,
    pub context: u8,
    pub per_repo: usize,
    pub overall: usize,
}

pub fn plan(request: &SearchRequest) -> Result<Plan, String> {
    if request.repos.is_empty() || request.repos.len() > MAX_REPOS {
        return Err(format!("Choose between 1 and {MAX_REPOS} repositories"));
    }
    let pattern = &request.pattern;
    if pattern.is_empty() || pattern.len() > MAX_PATTERN {
        return Err(format!("Pattern must be 1 to {MAX_PATTERN} bytes"));
    }
    if pattern.chars().any(|c| c == '\0' || c == '\n' || c == '\r') {
        return Err("Pattern cannot contain line breaks or NUL".into());
    }
    if request.context > MAX_CONTEXT {
        return Err(format!("Context is limited to {MAX_CONTEXT} lines"));
    }
    validate_pathspecs(&request.pathspecs)?;
    let per_repo = request.max_per_repo.unwrap_or(DEFAULT_PER_REPO);
    let overall = request.max_overall.unwrap_or(DEFAULT_OVERALL);
    if !(1..=MAX_PER_REPO).contains(&per_repo) || !(1..=MAX_OVERALL).contains(&overall) {
        return Err("Result limits are out of range".into());
    }
    Ok(Plan {
        pattern: pattern.clone(),
        flags: flags(request),
        pathspecs: request.pathspecs.clone(),
        untracked: request.untracked,
        context: request.context,
        per_repo,
        overall,
    })
}

fn validate_pathspecs(pathspecs: &[String]) -> Result<(), String> {
    if pathspecs.len() > MAX_PATHSPECS {
        return Err(format!(
            "At most {MAX_PATHSPECS} path filters are supported"
        ));
    }
    for spec in pathspecs {
        let invalid = spec.is_empty()
            || spec.len() > 512
            || spec.starts_with(['-', ':'])
            || spec.chars().any(|c| c.is_control());
        if invalid {
            return Err(format!("Invalid path filter {spec:?}"));
        }
    }
    Ok(())
}

fn flags(request: &SearchRequest) -> Vec<&'static str> {
    let mut flags = vec![match request.mode {
        Mode::Fixed => "-F",
        Mode::Basic => "-G",
        Mode::Perl => "-P",
    }];
    if request.ignore_case {
        flags.push("-i");
    }
    if request.whole_word {
        flags.push("-w");
    }
    flags
}

pub fn validate_target(target: &RepoTarget, plan: &Plan) -> Result<(), String> {
    valid_root(&target.path)?;
    let Some(name) = &target.git_ref else {
        return Ok(());
    };
    if plan.untracked {
        return Err("Untracked files exist only in the working tree".into());
    }
    if name == "HEAD" {
        return Err("Choose a branch or tag; HEAD is not accepted".into());
    }
    valid_ref(name)
}

pub async fn perl_supported() -> bool {
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
        ".paperwing-missing",
    ];
    let request = Request {
        args: &args,
        context: "Check Perl regex support",
        timeout: Duration::from_secs(10),
        expected: &[0, 1, 128],
        policy: OutputPolicy::Metadata,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    execute_cancellable(request, cancel)
        .await
        .is_ok_and(|output| matches!(output.code, Some(0 | 1)))
}

pub struct RepoResult {
    pub status: RepoStatus,
    pub matches: Vec<Match>,
}

pub async fn search_repo(
    target: &RepoTarget,
    plan: &Plan,
    allowance: usize,
    cancel: Arc<AtomicBool>,
) -> RepoResult {
    let limit = allowance.min(plan.per_repo);
    match run_grep(target, plan, limit, cancel.clone()).await {
        Ok((matches, truncated)) => RepoResult {
            status: RepoStatus {
                state: State::Done,
                matches: matches.len(),
                truncated,
                error: None,
            },
            matches,
        },
        Err(error) => {
            let cancelled = cancel.load(std::sync::atomic::Ordering::Relaxed);
            let state = if cancelled {
                State::Cancelled
            } else {
                State::Failed
            };
            RepoResult {
                status: RepoStatus::without_matches(state, Some(describe(&error))),
                matches: Vec::new(),
            }
        }
    }
}

fn describe(error: &str) -> String {
    if error.contains("capture limit") {
        return "Too many matches to read; narrow the pattern or add a path filter".into();
    }
    error.to_string()
}

async fn run_grep(
    target: &RepoTarget,
    plan: &Plan,
    limit: usize,
    cancel: Arc<AtomicBool>,
) -> Result<(Vec<Match>, bool), String> {
    let argv = build_argv(target, plan, limit);
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let request = Request {
        args: &args,
        context: "Search repository",
        timeout: GREP_TIMEOUT,
        expected: &[0, 1],
        policy: OutputPolicy::Metadata,
    };
    let output = execute_cancellable(request, cancel).await?;
    if !matches!(output.code, Some(0 | 1)) {
        return Err(output.last_error());
    }
    let prefix = target.git_ref.as_ref().map(|name| format!("{name}:"));
    let records = parse_records(&output.stdout, prefix.as_deref());
    Ok(assemble(&target.path, records, plan.context, limit))
}

fn build_argv(target: &RepoTarget, plan: &Plan, limit: usize) -> Vec<String> {
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
    argv.extend(["-m".into(), (limit + 1).to_string()]);
    argv.extend(["-e".into(), plan.pattern.clone()]);
    argv.extend(target.git_ref.clone());
    argv.push("--".into());
    argv.extend(plan.pathspecs.iter().cloned());
    argv
}

enum Record {
    Match {
        path: String,
        line: u32,
        column: usize,
        text: Vec<u8>,
    },
    Context {
        path: String,
        line: u32,
        text: Vec<u8>,
    },
}

fn parse_records(stdout: &[u8], prefix: Option<&str>) -> Vec<Record> {
    stdout
        .split(|byte| *byte == b'\n')
        .filter_map(|row| parse_row(row, prefix))
        .collect()
}

fn parse_row(row: &[u8], prefix: Option<&str>) -> Option<Record> {
    let mut parts = row.splitn(4, |byte| *byte == 0);
    let path = String::from_utf8_lossy(parts.next()?).into_owned();
    let path = match prefix {
        Some(prefix) => path.strip_prefix(prefix).unwrap_or(&path).to_string(),
        None => path,
    };
    let line: u32 = std::str::from_utf8(parts.next()?).ok()?.parse().ok()?;
    let third = parts.next()?;
    match parts.next() {
        Some(text) => {
            let column = std::str::from_utf8(third).ok()?.parse().ok()?;
            Some(Record::Match {
                path,
                line,
                column,
                text: text.to_vec(),
            })
        }
        None => Some(Record::Context {
            path,
            line,
            text: third.to_vec(),
        }),
    }
}

fn clip(text: &[u8]) -> String {
    let text = String::from_utf8_lossy(text);
    let text = text.trim_end_matches('\r');
    match text.char_indices().nth(MAX_LINE_CHARS) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

fn char_column(text: &[u8], column: usize) -> u32 {
    let end = column.saturating_sub(1).min(text.len());
    let chars = String::from_utf8_lossy(&text[..end]).chars().count();
    u32::try_from(chars + 1).unwrap_or(u32::MAX)
}

fn assemble(repo: &str, records: Vec<Record>, context: u8, limit: usize) -> (Vec<Match>, bool) {
    let mut matches = Vec::new();
    let mut truncated = false;
    for (index, record) in records.iter().enumerate() {
        let Record::Match {
            path,
            line,
            column,
            text,
        } = record
        else {
            continue;
        };
        if matches.len() >= limit {
            truncated = true;
            break;
        }
        matches.push(Match {
            repo: repo.to_string(),
            path: path.clone(),
            line: *line,
            column: char_column(text, *column),
            text: clip(text),
            context: gather(&records, index, context),
        });
    }
    (matches, truncated)
}

fn gather(records: &[Record], index: usize, context: u8) -> Vec<ContextLine> {
    let Record::Match { path, line, .. } = &records[index] else {
        return Vec::new();
    };
    let near = |other: u32| other != *line && other.abs_diff(*line) <= u32::from(context);
    let around = records.iter().filter_map(|record| match record {
        Record::Context {
            path: other,
            line,
            text,
        } if other == path && near(*line) => Some(ContextLine {
            line: *line,
            text: clip(text),
        }),
        _ => None,
    });
    around.collect()
}
