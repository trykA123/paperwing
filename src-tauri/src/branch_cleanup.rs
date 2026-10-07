mod delete;
mod list;
#[allow(dead_code)]
mod remote;
mod resolve;

use crate::commit::idle_check;
use crate::git::valid_root;
use resolve::Target;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

const MAX_BRANCHES: usize = 500;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LocalCandidate {
    name: String,
    oid: String,
    merged: bool,
    upstream: Option<String>,
    upstream_gone: bool,
    in_worktree: bool,
    last_commit: i64,
    subject: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCandidate {
    name: String,
    oid: String,
    last_commit: i64,
    subject: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MergedBranches {
    base: String,
    base_name: String,
    remote: Option<String>,
    remote_base: Option<String>,
    current: Option<String>,
    local: Vec<LocalCandidate>,
    remote_branches: Vec<RemoteCandidate>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BranchOutcome {
    name: String,
    deleted: bool,
    error: Option<String>,
}

static LOCKS: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();

fn repo_lock(path: &str) -> Arc<tokio::sync::Mutex<()>> {
    let mut locks = LOCKS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    locks.entry(path.to_string()).or_default().clone()
}

fn check_batch(names: &[String], expected: &[String]) -> Result<(), String> {
    if names.is_empty() {
        return Err("Select at least one branch".into());
    }
    if names.len() > MAX_BRANCHES {
        return Err("Too many branches in one operation".into());
    }
    if names.len() != expected.len() {
        return Err("Every branch needs its expected commit".into());
    }
    Ok(())
}

/// Lists local branches (merged flag against the base) and merged remote branches. Squash merges are not detected.
#[tauri::command]
pub async fn merged_branches(
    path: String,
    base: Option<String>,
    remote: Option<String>,
) -> Result<MergedBranches, String> {
    valid_root(&path)?;
    list::build(Target::listing(&path, base, remote).await?).await
}

/// Deletes local branches that still match their expected tip and are still merged into the base. Never forces.
#[tauri::command]
pub async fn delete_merged_branches(
    path: String,
    names: Vec<String>,
    expected: Vec<String>,
    base: Option<String>,
) -> Result<Vec<BranchOutcome>, String> {
    valid_root(&path)?;
    idle_check()?;
    check_batch(&names, &expected)?;
    let lock = repo_lock(&path);
    let _guard = lock.lock().await;
    let target = Target::local(&path, base).await?;
    let mut results = Vec::with_capacity(names.len());
    for (name, oid) in names.iter().zip(&expected) {
        let result = delete::delete_local_one(&target, name, oid).await;
        results.push(BranchOutcome {
            name: name.clone(),
            deleted: result.is_ok(),
            error: result.err(),
        });
    }
    Ok(results)
}

/// Deletes merged branches on one remote in one leased push. Protected names are refused.
#[tauri::command]
#[allow(dead_code)]
pub async fn delete_remote_branches(
    path: String,
    remote: String,
    names: Vec<String>,
    expected: Vec<String>,
    base: Option<String>,
) -> Result<Vec<BranchOutcome>, String> {
    valid_root(&path)?;
    idle_check()?;
    check_batch(&names, &expected)?;
    let lock = repo_lock(&path);
    let _guard = lock.lock().await;
    let target = Target::remote(&path, remote, base).await?;
    remote::delete_batch(&target, &names, &expected).await
}

#[cfg(test)]
mod tests;
