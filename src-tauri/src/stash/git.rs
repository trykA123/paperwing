use super::parse::{self, StashEntry};
use super::valid_oid;
use crate::git::repo_command::RepoGit;
use crate::git::{execute, Captured, OutputPolicy, Request};
use std::time::Duration;

pub(crate) async fn run(
    path: &str,
    args: &[&str],
    context: &str,
    expected: &[i32],
    policy: OutputPolicy,
    timeout: Duration,
) -> Result<Captured, String> {
    let argv = RepoGit::at(path).argv(args);
    let output = execute(
        Request {
            args: &argv,
            context,
            expected,
            policy,
            timeout,
        },
        None,
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

pub(crate) async fn locate(path: &str, oid: &str) -> Result<String, String> {
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

pub(crate) async fn top_oid(path: &str) -> Result<Option<String>, String> {
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

pub(crate) async fn is_dirty(path: &str, include_untracked: bool) -> Result<bool, String> {
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

pub(crate) async fn conflicted_paths(path: &str) -> Result<Vec<String>, String> {
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
        .take(500)
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect())
}
