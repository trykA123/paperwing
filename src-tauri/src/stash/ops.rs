use super::git::{conflicted_paths, is_dirty, list_entries, locate, quick, run, top_oid};
use super::{lock_repository, ApplyOutcome, PushOutcome, Restore, StashDiff};
use crate::git::{Captured, OutputPolicy};
use std::time::Duration;

const MAX_MESSAGE: usize = 4096;
const TOO_LARGE: &str = "Stash too large to preview";

pub(crate) async fn push(
    path: &str,
    message: Option<&str>,
    include_untracked: bool,
) -> Result<PushOutcome, String> {
    let _guard = lock_repository(path).await;
    push_locked(path, message, include_untracked).await
}

pub(crate) async fn push_locked(
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
    let context = format!("Stash: {path}");
    run(
        path,
        &args,
        &context,
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

async fn attempt(
    path: &str,
    verb: &str,
    target: &str,
    with_index: bool,
) -> Result<Captured, String> {
    let mut args = vec!["stash", verb];
    if with_index {
        args.push("--index");
    }
    args.push(target);
    let context = format!("Stash {verb}: {path}");
    run(
        path,
        &args,
        &context,
        &[0, 1, 128],
        OutputPolicy::Text,
        Duration::from_secs(120),
    )
    .await
}

pub(crate) async fn restore(path: &str, oid: &str, mode: Restore) -> Result<ApplyOutcome, String> {
    let _guard = lock_repository(path).await;
    let reference = locate(path, oid).await?;
    if !conflicted_paths(path).await?.is_empty() {
        return Err("Resolve the current conflicts first".into());
    }
    let (verb, target) = match mode {
        Restore::Pop => ("pop", reference.as_str()),
        Restore::Apply => ("apply", oid),
    };
    let mut output = attempt(path, verb, target, true).await?;
    let mut index_restored = true;
    let retry = output.code != Some(0)
        && conflicted_paths(path).await?.is_empty();
    if retry {
        index_restored = false;
        output = attempt(path, verb, target, false).await?;
    }
    let stash_kept = list_entries(path)
        .await?
        .iter()
        .any(|entry| entry.oid == oid);
    if output.code == Some(0) {
        return Ok(ApplyOutcome {
            applied: true,
            stash_kept,
            index_restored,
            conflicted: Vec::new(),
            error: None,
        });
    }
    let conflicted = if output.code == Some(1) {
        conflicted_paths(path).await?
    } else {
        Vec::new()
    };
    let error = output.safe(String::from_utf8_lossy(&output.stderr).trim());
    Ok(ApplyOutcome {
        applied: false,
        stash_kept,
        index_restored,
        conflicted,
        error: Some(error),
    })
}

pub(crate) async fn drop_stash(path: &str, oid: &str) -> Result<(), String> {
    let _guard = lock_repository(path).await;
    let reference = locate(path, oid).await?;
    let context = format!("Stash drop: {path}");
    run(
        path,
        &["stash", "drop", &reference],
        &context,
        &[0],
        OutputPolicy::Text,
        Duration::from_secs(45),
    )
    .await?;
    Ok(())
}

async fn summary(path: &str, oid: &str, has_untracked: bool) -> Result<String, String> {
    let context = format!("Stash summary: {path}");
    let stat = quick(
        path,
        &["stash", "show", "--stat", "--no-color", oid],
        &context,
        &[0],
    )
    .await?;
    let mut text = String::from_utf8_lossy(&stat.stdout).into_owned();
    if has_untracked {
        let third = format!("{oid}^3");
        let names = quick(
            path,
            &["ls-tree", "-r", "--name-only", &third],
            &context,
            &[0],
        )
        .await?;
        text.push_str("\nUntracked files:\n");
        text.push_str(&String::from_utf8_lossy(&names.stdout));
    }
    Ok(text)
}

pub(crate) async fn show(path: &str, oid: &str) -> Result<StashDiff, String> {
    locate(path, oid).await?;
    let third = format!("{oid}^3");
    let probe = quick(
        path,
        &["rev-parse", "--verify", "--quiet", &third],
        &format!("Stash untracked probe: {path}"),
        &[0, 1],
    )
    .await?;
    let has_untracked = probe.code == Some(0);
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
    args.push(oid);
    let context = format!("Stash show: {path}");
    let output = match run(
        path,
        &args,
        &context,
        &[0],
        OutputPolicy::Metadata,
        Duration::from_secs(60),
    )
    .await
    {
        Ok(output) => output,
        Err(error) if error.contains("capture limit") => {
            return Ok(StashDiff {
                patch: summary(path, oid, has_untracked).await?,
                truncated: true,
                has_untracked,
                notice: Some(TOO_LARGE.into()),
            });
        }
        Err(error) => return Err(error),
    };
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
        notice: truncated.then(|| TOO_LARGE.into()),
    })
}
