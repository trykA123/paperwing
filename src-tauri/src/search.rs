use crate::git::{valid_ref, valid_root};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

pub const DEFAULT_PER_REPO: usize = 200;
pub const DEFAULT_OVERALL: usize = 2000;
pub const MAX_PER_REPO: usize = 2000;
pub const MAX_OVERALL: usize = 10_000;
pub const MAX_REPOS: usize = 500;
const MAX_PATTERN: usize = 4096;
const MAX_PATHSPECS: usize = 32;
const MAX_CONTEXT: u8 = 3;

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

pub fn dedupe(repos: Vec<RepoTarget>) -> Vec<RepoTarget> {
    let mut seen = HashSet::new();
    let mut kept = Vec::new();
    for target in repos {
        let canonical = crate::platform::canonical_path(Path::new(&target.path)).map_or_else(
            |_| target.path.clone(),
            |path| path.to_string_lossy().into_owned(),
        );
        if seen.insert((canonical, target.git_ref.clone())) {
            kept.push(target);
        }
    }
    kept
}
