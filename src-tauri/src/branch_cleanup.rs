use crate::commit::{delete_branch, idle_check, quick, run};
use crate::git::{valid_ref, valid_root, OutputPolicy};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

const MAX_BRANCHES: usize = 500;
const FIELD: char = '\0';

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

struct Base {
    reference: String,
    name: String,
}

static LOCKS: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();

fn repo_lock(path: &str) -> Arc<tokio::sync::Mutex<()>> {
    let mut locks = LOCKS.get_or_init(Default::default).lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    locks.entry(path.to_string()).or_default().clone()
}

async fn text(path: &str, args: &[&str], context: &str, expected: &[i32]) -> Result<Option<String>, String> {
    let output = quick(path, args, &format!("{context}: {path}"), expected).await?;
    if output.code != Some(0) { return Ok(None); }
    Ok(Some(String::from_utf8_lossy(&output.stdout).trim().to_string()))
}

async fn ref_exists(path: &str, reference: &str) -> Result<bool, String> {
    let output = quick(path, &["show-ref", "--verify", "--quiet", reference], &format!("Ref probe: {path}"), &[0, 1]).await?;
    Ok(output.code == Some(0))
}

async fn remote_names(path: &str) -> Result<Vec<String>, String> {
    let out = text(path, &["remote"], "Remotes", &[0]).await?.unwrap_or_default();
    Ok(out.lines().map(|line| line.trim().to_string()).filter(|line| !line.is_empty()).collect())
}

fn valid_branch(name: &str) -> Result<(), String> {
    valid_ref(name).map_err(|_| format!("Invalid branch name: {name}"))?;
    if name.eq_ignore_ascii_case("head") || name.starts_with("refs/") { return Err(format!("Invalid branch name: {name}")); }
    Ok(())
}

async fn choose_remote(path: &str, requested: Option<String>, required: bool) -> Result<Option<String>, String> {
    let remotes = remote_names(path).await?;
    if let Some(name) = requested.map(|value| value.trim().to_string()).filter(|value| !value.is_empty()) {
        valid_ref(&name).map_err(|_| "Invalid remote name".to_string())?;
        if !remotes.contains(&name) { return Err(format!("There is no remote named {name}")); }
        return Ok(Some(name));
    }
    let chosen = match remotes.as_slice() {
        [only] => Some(only.clone()),
        many if many.iter().any(|name| name == "origin") => Some("origin".to_string()),
        _ => None,
    };
    if chosen.is_none() && required { return Err("Choose a remote".into()); }
    Ok(chosen)
}

fn base_name(reference: &str) -> String {
    if let Some(rest) = reference.strip_prefix("refs/heads/") { return rest.to_string(); }
    let rest = reference.strip_prefix("refs/remotes/").unwrap_or(reference);
    rest.split_once('/').map_or(rest, |(_, name)| name).to_string()
}

async fn remote_head(path: &str, remote: &str) -> Result<Option<String>, String> {
    let head = format!("refs/remotes/{remote}/HEAD");
    let target = text(path, &["symbolic-ref", "-q", &head], "Remote HEAD", &[0, 1]).await?;
    Ok(target.filter(|value| value.starts_with(&format!("refs/remotes/{remote}/"))))
}

async fn resolve_base(path: &str, base: Option<String>, remote: Option<&str>) -> Result<Base, String> {
    let requested = base.map(|value| value.trim().to_string()).filter(|value| !value.is_empty());
    if let Some(name) = requested {
        valid_branch(&name)?;
        for candidate in [format!("refs/heads/{name}"), format!("refs/remotes/{name}")] {
            if ref_exists(path, &candidate).await? { return Ok(Base { name: base_name(&candidate), reference: candidate }); }
        }
        return Err(format!("Base branch {name} was not found"));
    }
    let mut candidates = Vec::new();
    if let Some(remote) = remote { candidates.extend(remote_head(path, remote).await?); }
    candidates.extend(["refs/heads/main".to_string(), "refs/heads/master".to_string()]);
    for candidate in candidates {
        if ref_exists(path, &candidate).await? { return Ok(Base { name: base_name(&candidate), reference: candidate }); }
    }
    Err("No base branch was found; choose one".into())
}

fn short(reference: &str) -> &str {
    reference.strip_prefix("refs/heads/").or_else(|| reference.strip_prefix("refs/remotes/")).unwrap_or(reference)
}

async fn merged_refs(path: &str, base: &str, pattern: &str) -> Result<HashSet<String>, String> {
    let merged = format!("--merged={base}");
    let out = text(path, &["for-each-ref", &merged, "--format=%(refname)", pattern], "Merged branches", &[0]).await?.unwrap_or_default();
    Ok(out.lines().map(str::to_string).collect())
}

async fn records(path: &str, pattern: &str, format: &str, extra: &[&str]) -> Result<Vec<Vec<String>>, String> {
    let format = format!("--format={format}");
    let mut args = vec!["for-each-ref", &format];
    args.extend_from_slice(extra);
    args.push(pattern);
    let out = text(path, &args, "Branch list", &[0]).await?.unwrap_or_default();
    Ok(out.lines().map(|line| line.split(FIELD).map(str::to_string).collect()).collect())
}

fn field(row: &[String], index: usize) -> String {
    row.get(index).cloned().unwrap_or_default()
}

async fn current_branch(path: &str) -> Result<Option<String>, String> {
    text(path, &["symbolic-ref", "--short", "-q", "HEAD"], "Current branch", &[0, 1]).await
}

/// Lists local branches (merged flag against the base) and merged remote branches. Squash merges are not detected.
#[tauri::command]
pub async fn merged_branches(path: String, base: Option<String>, remote: Option<String>) -> Result<MergedBranches, String> {
    valid_root(&path)?;
    let remote = choose_remote(&path, remote, false).await?;
    let base = resolve_base(&path, base, remote.as_deref()).await?;
    let current = current_branch(&path).await?;
    let merged = merged_refs(&path, &base.reference, "refs/heads").await?;
    let rows = records(&path, "refs/heads", "%(refname)%00%(objectname)%00%(upstream:short)%00%(upstream:track)%00%(worktreepath)%00%(committerdate:unix)%00%(contents:subject)", &[]).await?;
    let local = rows.iter().filter_map(|row| {
        let reference = field(row, 0);
        let name = short(&reference).to_string();
        if Some(&name) == current.as_ref() || name == base.name { return None; }
        let upstream = Some(field(row, 2)).filter(|value| !value.is_empty());
        Some(LocalCandidate {
            merged: merged.contains(&reference),
            oid: field(row, 1),
            upstream_gone: upstream.is_some() && field(row, 3).contains("gone"),
            upstream,
            in_worktree: !field(row, 4).is_empty(),
            last_commit: field(row, 5).parse().unwrap_or(0),
            subject: field(row, 6),
            name,
        })
    }).collect();
    let remote_branches = match &remote {
        Some(remote) => remote_candidates(&path, remote, &base).await?,
        None => Vec::new(),
    };
    Ok(MergedBranches { base: short(&base.reference).to_string(), base_name: base.name, remote, current, local, remote_branches })
}

async fn remote_candidates(path: &str, remote: &str, base: &Base) -> Result<Vec<RemoteCandidate>, String> {
    let prefix = format!("refs/remotes/{remote}/");
    let head = remote_head(path, remote).await?.map(|target| base_name(&target));
    let rows = records(path, &prefix, "%(refname)%00%(objectname)%00%(committerdate:unix)%00%(contents:subject)", &[&format!("--merged={}", base.reference)]).await?;
    Ok(rows.iter().filter_map(|row| {
        let reference = field(row, 0);
        let name = reference.strip_prefix(&prefix)?.to_string();
        if name == "HEAD" || name == base.name || Some(&name) == head.as_ref() { return None; }
        Some(RemoteCandidate { oid: field(row, 1), last_commit: field(row, 2).parse().unwrap_or(0), subject: field(row, 3), name })
    }).collect())
}

fn check_batch(names: &[String], expected: &[String]) -> Result<(), String> {
    if names.is_empty() { return Err("Select at least one branch".into()); }
    if names.len() > MAX_BRANCHES { return Err("Too many branches in one operation".into()); }
    if names.len() != expected.len() { return Err("Every branch needs its expected commit".into()); }
    Ok(())
}

fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn outcome(name: &str, result: Result<(), String>) -> BranchOutcome {
    BranchOutcome { name: name.to_string(), deleted: result.is_ok(), error: result.err() }
}

async fn is_ancestor(path: &str, commit: &str, base: &str) -> Result<bool, String> {
    let output = quick(path, &["merge-base", "--is-ancestor", commit, base], &format!("Merged check: {path}"), &[0, 1, 128]).await?;
    Ok(output.code == Some(0))
}

async fn delete_local_one(path: &str, name: &str, oid: &str, base: &Base, current: Option<&str>) -> Result<(), String> {
    valid_branch(name)?;
    if name == base.name { return Err(format!("{name} is the base branch")); }
    if current == Some(name) { return Err(format!("{name} is the current branch")); }
    let reference = format!("refs/heads/{name}");
    let tip = text(path, &["rev-parse", "--verify", "--quiet", &reference], "Branch tip", &[0, 1]).await?.ok_or_else(|| format!("There is no local branch named {name}"))?;
    if !valid_oid(oid) || tip != oid { return Err(format!("{name} changed since it was listed"));  }
    if !is_ancestor(path, &reference, &base.reference).await? { return Err(format!("{name} is not merged into {}", short(&base.reference))); }
    delete_branch(path.to_string(), name.to_string(), false).await.map(|_| ())
}

/// Deletes local branches that still match their expected tip and are still merged into the base. Never forces.
#[tauri::command]
pub async fn delete_merged_branches(path: String, names: Vec<String>, expected: Vec<String>, base: Option<String>) -> Result<Vec<BranchOutcome>, String> {
    valid_root(&path)?;
    idle_check()?;
    check_batch(&names, &expected)?;
    let lock = repo_lock(&path);
    let _guard = lock.lock().await;
    let remote = choose_remote(&path, None, false).await?;
    let base = resolve_base(&path, base, remote.as_deref()).await?;
    let current = current_branch(&path).await?;
    let mut results = Vec::with_capacity(names.len());
    for (name, oid) in names.iter().zip(&expected) {
        results.push(outcome(name, delete_local_one(&path, name, oid, &base, current.as_deref()).await));
    }
    Ok(results)
}

async fn delete_remote_one(path: &str, remote: &str, name: &str, oid: &str, base: &Base, protected: &HashSet<String>) -> Result<(), String> {
    valid_branch(name)?;
    if protected.contains(name) { return Err(format!("{name} is protected")); }
    if !valid_oid(oid) { return Err(format!("{name} needs its expected commit")); }
    if !is_ancestor(path, oid, &base.reference).await? { return Err(format!("{name} is not merged into {}", short(&base.reference))); }
    let lease = format!("--force-with-lease=refs/heads/{name}:{oid}");
    let reference = format!("refs/heads/{name}");
    run(path, &["push", remote, &lease, "--delete", &reference], &format!("Delete remote branch: {path}"), &[0], OutputPolicy::Text, None, Duration::from_secs(180)).await.map(|_| ())
}

/// Deletes merged branches on one remote. Each deletion is leased on the expected commit; protected names are refused.
#[tauri::command]
pub async fn delete_remote_branches(path: String, remote: String, names: Vec<String>, expected: Vec<String>, base: Option<String>) -> Result<Vec<BranchOutcome>, String> {
    valid_root(&path)?;
    idle_check()?;
    check_batch(&names, &expected)?;
    let lock = repo_lock(&path);
    let _guard = lock.lock().await;
    let remote = choose_remote(&path, Some(remote), true).await?.ok_or("Choose a remote")?;
    let base = resolve_base(&path, base, Some(&remote)).await?;
    let mut protected: HashSet<String> = ["main", "master"].iter().map(|name| name.to_string()).collect();
    protected.insert(base.name.clone());
    protected.extend(remote_head(&path, &remote).await?.map(|target| base_name(&target)));
    let mut results = Vec::with_capacity(names.len());
    for (name, oid) in names.iter().zip(&expected) {
        results.push(outcome(name, delete_remote_one(&path, &remote, name, oid, &base, &protected).await));
    }
    Ok(results)
}

#[cfg(test)]
mod tests;
