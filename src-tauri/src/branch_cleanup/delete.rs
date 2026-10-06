use super::resolve::{short, text, valid_branch, valid_oid, Target};
use crate::commit::{quick, run};
use crate::git::OutputPolicy;
use std::time::Duration;

pub(super) async fn is_ancestor(path: &str, commit: &str, base: &str) -> Result<bool, String> {
    let output = quick(
        path,
        &["merge-base", "--is-ancestor", commit, base],
        &format!("Merged check: {path}"),
        &[0, 1, 128],
    )
    .await?;
    Ok(output.code == Some(0))
}

async fn checked_out_elsewhere(path: &str, name: &str) -> Result<bool, String> {
    let reference = format!("refs/heads/{name}");
    let rows = text(
        path,
        &[
            "for-each-ref",
            "--format=%(refname)%00%(worktreepath)",
            "refs/heads",
        ],
        "Worktrees",
        &[0],
    )
    .await?
    .unwrap_or_default();
    Ok(rows.lines().any(|row| {
        row.split_once('\0')
            .is_some_and(|(found, tree)| found == reference && !tree.is_empty())
    }))
}

pub(super) async fn delete_local_one(target: &Target, name: &str, oid: &str) -> Result<(), String> {
    let path = &target.path;
    valid_branch(name)?;
    if target.current.as_deref() == Some(name) {
        return Err(format!("{name} is the current branch"));
    }
    if target.protected.contains(name) {
        return Err(format!("{name} is protected"));
    }
    let reference = format!("refs/heads/{name}");
    if text(
        path,
        &["symbolic-ref", "-q", &reference],
        "Symbolic ref",
        &[0, 1],
    )
    .await?
    .is_some()
    {
        return Err(format!("{name} is a symbolic ref"));
    }
    let tip = text(
        path,
        &["rev-parse", "--verify", "--quiet", &reference],
        "Branch tip",
        &[0, 1],
    )
    .await?
    .ok_or_else(|| format!("There is no local branch named {name}"))?;
    if !valid_oid(oid) || tip != oid {
        return Err(format!("{name} changed since it was listed"));
    }
    if checked_out_elsewhere(path, name).await? {
        return Err(format!("{name} is checked out in a worktree"));
    }
    if !is_ancestor(path, &reference, &target.base.reference).await? {
        return Err(format!(
            "{name} is not merged into {}",
            short(&target.base.reference)
        ));
    }
    run(
        path,
        &["update-ref", "--no-deref", "-d", &reference, oid],
        &format!("Delete local branch: {path}"),
        &[0],
        OutputPolicy::Text,
        None,
        Duration::from_secs(45),
    )
    .await?;
    let section = format!("branch.{name}");
    quick(
        path,
        &["config", "--remove-section", &section],
        &format!("Branch config: {path}"),
        &[0, 128],
    )
    .await
    .map(|_| ())
}
