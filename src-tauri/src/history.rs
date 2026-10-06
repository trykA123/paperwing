use crate::git::{execute, valid_ref, valid_root, Captured, OutputPolicy, Request};
use serde::Serialize;
use std::time::Duration;

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;
const BELOW_BASE: usize = 5;
const SUBJECT_CHARS: usize = 200;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryCommit {
    sha: String,
    short: String,
    subject: String,
    author: String,
    date: String,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum HistoryKind {
    Tracking,
    NoUpstream,
    UpstreamGone,
    Detached,
    Unborn,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryHistory {
    kind: HistoryKind,
    branch: Option<String>,
    upstream: Option<String>,
    uncommitted: usize,
    local: Vec<HistoryCommit>,
    local_total: usize,
    origin: Vec<HistoryCommit>,
    origin_total: usize,
    base: Option<HistoryCommit>,
    below: Vec<HistoryCommit>,
}

async fn git(path: &str, args: &[&str], expected: &[i32]) -> Result<Captured, String> {
    let mut argv = vec![
        "--no-optional-locks",
        "-C",
        path,
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.quotepath=false",
        "-c",
        "log.showSignature=false",
    ];
    argv.extend_from_slice(args);
    let context = format!("History: {path}");
    let output = execute(
        Request {
            args: &argv,
            context: &context,
            expected,
            policy: OutputPolicy::Text,
            timeout: Duration::from_secs(45),
        },
        None,
    )
    .await?;
    if !output.code.is_some_and(|code| expected.contains(&code)) {
        return Err(output.last_error());
    }
    Ok(output)
}

fn text(output: &Captured) -> String {
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn parse_commits(output: &Captured) -> Vec<HistoryCommit> {
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter_map(|record| {
            let record = String::from_utf8_lossy(record);
            let mut fields = record.trim_start_matches('\n').splitn(4, '\u{1f}');
            let (sha, author, date, subject) = (
                fields.next()?,
                fields.next()?,
                fields.next()?,
                fields.next()?,
            );
            if sha.is_empty() {
                return None;
            }
            Some(HistoryCommit {
                sha: sha.to_string(),
                short: sha.chars().take(8).collect(),
                subject: output.safe(&subject.chars().take(SUBJECT_CHARS).collect::<String>()),
                author: output.safe(author),
                date: date.to_string(),
            })
        })
        .collect()
}

async fn log(path: &str, revisions: &[&str], take: usize) -> Result<Vec<HistoryCommit>, String> {
    let take = take.to_string();
    let mut args = vec![
        "log",
        "-z",
        "--topo-order",
        "--no-color",
        "--format=%H%x1f%an%x1f%aI%x1f%s",
        "-n",
        &take,
    ];
    args.extend_from_slice(revisions);
    args.push("--");
    Ok(parse_commits(&git(path, &args, &[0]).await?))
}

async fn count(path: &str, revisions: &[&str]) -> Result<usize, String> {
    let mut args = vec!["rev-list", "--count"];
    args.extend_from_slice(revisions);
    args.push("--");
    Ok(text(&git(path, &args, &[0]).await?).parse().unwrap_or(0))
}

async fn uncommitted(path: &str) -> Result<usize, String> {
    let output = git(
        path,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--no-renames",
            "--untracked-files=normal",
        ],
        &[0],
    )
    .await?;
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .count())
}

enum Upstream {
    Live { full: String, label: String },
    Gone(String),
    Absent,
}

async fn configured_upstream(path: &str, branch: &str) -> Result<Option<String>, String> {
    let merge = git(
        path,
        &["config", "--get", &format!("branch.{branch}.merge")],
        &[0, 1],
    )
    .await?;
    if merge.code != Some(0) {
        return Ok(None);
    }
    let remote = git(
        path,
        &["config", "--get", &format!("branch.{branch}.remote")],
        &[0, 1],
    )
    .await?;
    let (merge, remote) = (text(&merge), text(&remote));
    let short = merge.strip_prefix("refs/heads/").unwrap_or(&merge);
    let label = if remote.is_empty() || remote == "." {
        short.to_string()
    } else {
        format!("{remote}/{short}")
    };
    Ok(Some(label))
}

async fn upstream_of(path: &str, branch: &str) -> Result<Upstream, String> {
    let output = git(
        path,
        &["rev-parse", "--symbolic-full-name", "@{upstream}"],
        &[0, 128],
    )
    .await?;
    if output.code != Some(0) {
        return Ok(match configured_upstream(path, branch).await? {
            Some(label) => Upstream::Gone(output.safe(&label)),
            None => Upstream::Absent,
        });
    }
    let full = text(&output);
    valid_ref(&full)?;
    let short = full
        .strip_prefix("refs/remotes/")
        .or_else(|| full.strip_prefix("refs/heads/"))
        .unwrap_or(&full);
    Ok(Upstream::Live {
        label: output.safe(short),
        full,
    })
}

async fn fill_tracking(
    path: &str,
    upstream: &str,
    limit: usize,
    history: &mut RepositoryHistory,
) -> Result<(), String> {
    let (only_local, only_origin) = (["HEAD", &format!("^{upstream}")], [upstream, "^HEAD"]);
    history.local_total = count(path, &only_local).await?;
    history.origin_total = count(path, &only_origin).await?;
    history.local = log(path, &only_local, limit).await?;
    history.origin = log(path, &only_origin, limit).await?;
    let base = git(path, &["merge-base", "HEAD", upstream], &[0, 1]).await?;
    if base.code != Some(0) {
        return Ok(());
    }
    let mut chain = log(path, &[&text(&base)], BELOW_BASE + 1)
        .await?
        .into_iter();
    history.base = chain.next();
    history.below = chain.collect();
    Ok(())
}

async fn fill_local_only(
    path: &str,
    limit: usize,
    history: &mut RepositoryHistory,
) -> Result<(), String> {
    let unpublished = ["HEAD", "--not", "--remotes"];
    history.local_total = count(path, &unpublished).await?;
    history.local = log(path, &unpublished, limit).await?;
    Ok(())
}

async fn has_commit(path: &str) -> Result<bool, String> {
    Ok(git(
        path,
        &["rev-parse", "--verify", "-q", "HEAD^{commit}"],
        &[0, 1],
    )
    .await?
    .code
        == Some(0))
}

async fn blank_history(path: &str, branch: Option<String>) -> Result<RepositoryHistory, String> {
    Ok(RepositoryHistory {
        kind: HistoryKind::Unborn,
        branch,
        upstream: None,
        uncommitted: uncommitted(path).await?,
        local: Vec::new(),
        local_total: 0,
        origin: Vec::new(),
        origin_total: 0,
        base: None,
        below: Vec::new(),
    })
}

async fn fill_history(
    path: &str,
    limit: usize,
    upstream: Upstream,
    history: &mut RepositoryHistory,
) -> Result<(), String> {
    match upstream {
        Upstream::Live { full, label } => {
            history.kind = HistoryKind::Tracking;
            history.upstream = Some(label);
            fill_tracking(path, &full, limit, history).await
        }
        Upstream::Gone(label) => {
            history.kind = HistoryKind::UpstreamGone;
            history.upstream = Some(label);
            fill_local_only(path, limit, history).await
        }
        Upstream::Absent => {
            history.kind = if history.branch.is_some() {
                HistoryKind::NoUpstream
            } else {
                HistoryKind::Detached
            };
            fill_local_only(path, limit, history).await
        }
    }
}

pub(crate) async fn read_history(
    path: &str,
    limit: Option<usize>,
) -> Result<RepositoryHistory, String> {
    valid_root(path)?;
    let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let head = git(path, &["symbolic-ref", "--short", "-q", "HEAD"], &[0, 1]).await?;
    let branch = (head.code == Some(0)).then(|| head.safe(&text(&head)));
    let mut history = blank_history(path, branch.clone()).await?;
    if !has_commit(path).await? {
        return Ok(history);
    }
    let upstream = match &branch {
        Some(name) => upstream_of(path, name).await?,
        None => Upstream::Absent,
    };
    fill_history(path, limit, upstream, &mut history).await?;
    Ok(history)
}

#[tauri::command]
pub async fn repository_history(
    path: String,
    limit: Option<usize>,
) -> Result<RepositoryHistory, String> {
    read_history(&path, limit).await
}

#[cfg(test)]
mod tests;
