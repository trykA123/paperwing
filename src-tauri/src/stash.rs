mod parse;
mod switch;
#[cfg(test)]
mod tests;

use crate::git::{execute_cancellable, valid_root, Captured, OutputPolicy, Request};
use serde::Serialize;
use std::sync::{atomic::AtomicBool, Arc};
use std::time::Duration;

pub use parse::StashEntry;
pub use switch::SwitchOutcome;

const MAX_MESSAGE: usize = 4096;
const MAX_CONFLICTS: usize = 500;

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PushOutcome {
    pub stashed: Option<String>,
    pub nothing_to_stash: bool,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOutcome {
    pub applied: bool,
    pub stash_kept: bool,
    pub conflicted: Vec<String>,
    pub error: Option<String>,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StashDiff {
    pub patch: String,
    pub truncated: bool,
    pub has_untracked: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Restore {
    Apply,
    Pop,
}

pub(crate) async fn run(
    path: &str,
    args: &[&str],
    context: &str,
    expected: &[i32],
    policy: OutputPolicy,
    timeout: Duration,
) -> Result<Captured, String> {
    let mut argv = vec![
        "-C",
        path,
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.quotepath=false",
    ];
    argv.extend_from_slice(args);
    let output = execute_cancellable(
        Request {
            args: &argv,
            context,
            expected,
            policy,
            timeout,
        },
        Arc::new(AtomicBool::new(false)),
    )
    .await?;
    if !output.code.is_some_and(|code| expected.contains(&code)) {
        return Err(output.last_error());
    }
    Ok(output)
}

pub(crate) async fn quick(
    path: &str,
    args: &[&str],
    context: &str,
    expected: &[i32],
) -> Result<Captured, String> {
    run(
        path,
        args,
        context,
        expected,
        OutputPolicy::Text,
        Duration::from_secs(45),
    )
    .await
}

fn idle_check() -> Result<(), String> {
    if crate::clone::busy() {
        return Err("A clone, fetch or pull is running; try again when it finishes".into());
    }
    Ok(())
}

fn valid_oid(oid: &str) -> Result<(), String> {
    if matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("Invalid stash id".into())
    }
}

pub(crate) async fn list_entries(path: &str) -> Result<Vec<StashEntry>, String> {
    let output = quick(
        path,
        &["stash", "list", "--format=%gd%x09%H%x09%ct%x09%gs"],
        &format!("Stash list: {path}"),
        &[0],
    )
    .await?;
    Ok(parse::parse_list(
        &output.safe(&String::from_utf8_lossy(&output.stdout)),
    ))
}

async fn locate(path: &str, oid: &str) -> Result<String, String> {
    valid_oid(oid)?;
    let entries = list_entries(path).await?;
    let entry = entries
        .iter()
        .find(|entry| entry.oid == oid)
        .ok_or("That stash no longer exists; refresh the list")?;
    let resolved = quick(
        path,
        &["rev-parse", "--verify", "--quiet", &entry.reference],
        &format!("Stash probe: {path}"),
        &[0, 1],
    )
    .await?;
    if String::from_utf8_lossy(&resolved.stdout).trim() != oid {
        return Err("The stash list changed; refresh and try again".into());
    }
    Ok(entry.reference.clone())
}

async fn top_oid(path: &str) -> Result<Option<String>, String> {
    let output = quick(
        path,
        &["rev-parse", "--verify", "--quiet", "refs/stash"],
        &format!("Stash top: {path}"),
        &[0, 1],
    )
    .await?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((output.code == Some(0) && !text.is_empty()).then_some(text))
}

async fn is_dirty(path: &str, include_untracked: bool) -> Result<bool, String> {
    let mode = if include_untracked {
        "--untracked-files=normal"
    } else {
        "--untracked-files=no"
    };
    let output = quick(
        path,
        &["status", "--porcelain", mode],
        &format!("Dirty probe: {path}"),
        &[0],
    )
    .await?;
    Ok(!output.stdout.is_empty())
}

pub(crate) async fn push(
    path: &str,
    message: Option<&str>,
    include_untracked: bool,
) -> Result<PushOutcome, String> {
    let message = message.map(str::trim).filter(|text| !text.is_empty());
    if message.is_some_and(|text| text.len() > MAX_MESSAGE || text.contains('\0')) {
        return Err("The stash message is too long or invalid".into());
    }
    if !is_dirty(path, include_untracked).await? {
        return Ok(PushOutcome {
            stashed: None,
            nothing_to_stash: true,
        });
    }
    let before = top_oid(path).await?;
    let mut args = vec!["stash", "push"];
    if include_untracked {
        args.push("--include-untracked");
    }
    if let Some(text) = message {
        args.extend(["-m", text]);
    }
    run(
        path,
        &args,
        &format!("Stash: {path}"),
        &[0],
        OutputPolicy::Text,
        Duration::from_secs(120),
    )
    .await?;
    let after = top_oid(path).await?;
    if after == before {
        return Ok(PushOutcome {
            stashed: None,
            nothing_to_stash: true,
        });
    }
    Ok(PushOutcome {
        stashed: after,
        nothing_to_stash: false,
    })
}

async fn conflicted_paths(path: &str) -> Result<Vec<String>, String> {
    let output = quick(
        path,
        &["diff", "--name-only", "--diff-filter=U", "-z"],
        &format!("Conflicts: {path}"),
        &[0],
    )
    .await?;
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .take(MAX_CONFLICTS)
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect())
}

pub(crate) async fn restore(path: &str, oid: &str, mode: Restore) -> Result<ApplyOutcome, String> {
    let reference = locate(path, oid).await?;
    let verb = if mode == Restore::Pop { "pop" } else { "apply" };
    let output = run(
        path,
        &["stash", verb, "--index", &reference],
        &format!("Stash {verb}: {path}"),
        &[0, 1, 128],
        OutputPolicy::Text,
        Duration::from_secs(120),
    )
    .await?;
    let stash_kept = list_entries(path)
        .await?
        .iter()
        .any(|entry| entry.oid == oid);
    if output.code == Some(0) {
        return Ok(ApplyOutcome {
            applied: true,
            stash_kept,
            conflicted: Vec::new(),
            error: None,
        });
    }
    let conflicted = conflicted_paths(path).await?;
    let error = conflicted
        .is_empty()
        .then(|| output.safe(String::from_utf8_lossy(&output.stderr).trim()));
    Ok(ApplyOutcome {
        applied: false,
        stash_kept,
        conflicted,
        error,
    })
}

pub(crate) async fn drop_stash(path: &str, oid: &str) -> Result<(), String> {
    let reference = locate(path, oid).await?;
    run(
        path,
        &["stash", "drop", &reference],
        &format!("Stash drop: {path}"),
        &[0],
        OutputPolicy::Text,
        Duration::from_secs(45),
    )
    .await?;
    Ok(())
}

pub(crate) async fn show(path: &str, oid: &str) -> Result<StashDiff, String> {
    let reference = locate(path, oid).await?;
    let third = format!("{oid}^3");
    let has_untracked = quick(
        path,
        &["rev-parse", "--verify", "--quiet", &third],
        &format!("Stash untracked probe: {path}"),
        &[0, 1],
    )
    .await?
    .code
        == Some(0);
    let mut args = vec![
        "stash",
        "show",
        "-p",
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
    ];
    if has_untracked {
        args.push("--include-untracked");
    }
    args.push(&reference);
    let output = run(
        path,
        &args,
        &format!("Stash show: {path}"),
        &[0],
        OutputPolicy::Metadata,
        Duration::from_secs(60),
    )
    .await?;
    let truncated = output.stdout.len() > crate::paths::CONTENT_LIMIT;
    let end = if truncated {
        crate::paths::CONTENT_LIMIT
    } else {
        output.stdout.len()
    };
    Ok(StashDiff {
        patch: String::from_utf8_lossy(&output.stdout[..end]).into_owned(),
        truncated,
        has_untracked,
    })
}

#[tauri::command]
pub async fn stash_list(path: String) -> Result<Vec<StashEntry>, String> {
    valid_root(&path)?;
    list_entries(&path).await
}

#[tauri::command]
pub async fn stash_push(
    path: String,
    message: Option<String>,
    include_untracked: bool,
) -> Result<PushOutcome, String> {
    valid_root(&path)?;
    idle_check()?;
    push(&path, message.as_deref(), include_untracked).await
}

#[tauri::command]
pub async fn stash_apply(path: String, oid: String) -> Result<ApplyOutcome, String> {
    valid_root(&path)?;
    idle_check()?;
    restore(&path, &oid, Restore::Apply).await
}

#[tauri::command]
pub async fn stash_pop(path: String, oid: String) -> Result<ApplyOutcome, String> {
    valid_root(&path)?;
    idle_check()?;
    restore(&path, &oid, Restore::Pop).await
}

#[tauri::command]
pub async fn stash_drop(path: String, oid: String) -> Result<(), String> {
    valid_root(&path)?;
    idle_check()?;
    drop_stash(&path, &oid).await
}

#[tauri::command]
pub async fn stash_show(path: String, oid: String) -> Result<StashDiff, String> {
    valid_root(&path)?;
    show(&path, &oid).await
}

#[tauri::command]
pub async fn switch_with_stash(path: String, branch: String) -> Result<SwitchOutcome, String> {
    valid_root(&path)?;
    idle_check()?;
    switch::switch_with_stash(&path, &branch).await
}
