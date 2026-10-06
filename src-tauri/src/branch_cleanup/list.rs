use super::resolve::{short, text, Target};
use super::{LocalCandidate, MergedBranches, RemoteCandidate};
use std::collections::HashSet;

const FIELD: char = '\0';
const LOCAL_FORMAT: &str = "%(refname)%00%(objectname)%00%(upstream:short)%00%(upstream:track)%00%(worktreepath)%00%(committerdate:unix)%00%(contents:subject)%00%(symref)";
const REMOTE_FORMAT: &str =
    "%(refname)%00%(objectname)%00%(committerdate:unix)%00%(contents:subject)";

async fn merged_refs(path: &str, base: &str, pattern: &str) -> Result<HashSet<String>, String> {
    let merged = format!("--merged={base}");
    let out = text(
        path,
        &["for-each-ref", &merged, "--format=%(refname)", pattern],
        "Merged branches",
        &[0],
    )
    .await?
    .unwrap_or_default();
    Ok(out.lines().map(str::to_string).collect())
}

async fn records(
    path: &str,
    pattern: &str,
    format: &str,
    extra: &[&str],
) -> Result<Vec<Vec<String>>, String> {
    let format = format!("--format={format}");
    let mut args = vec!["for-each-ref", &format];
    args.extend_from_slice(extra);
    args.push(pattern);
    let out = text(path, &args, "Branch list", &[0])
        .await?
        .unwrap_or_default();
    Ok(out
        .lines()
        .map(|line| line.split(FIELD).map(str::to_string).collect())
        .collect())
}

fn field(row: &[String], index: usize) -> String {
    row.get(index).cloned().unwrap_or_default()
}

async fn local_candidates(target: &Target) -> Result<Vec<LocalCandidate>, String> {
    let path = &target.path;
    let merged = merged_refs(path, &target.base.reference, "refs/heads").await?;
    let rows = records(path, "refs/heads", LOCAL_FORMAT, &[]).await?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let reference = field(row, 0);
            let name = short(&reference).to_string();
            if target.protected.contains(&name) {
                return None;
            }
            if !field(row, 7).is_empty() {
                return None;
            }
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
        })
        .collect())
}

async fn remote_candidates(target: &Target) -> Result<Vec<RemoteCandidate>, String> {
    let (Some(remote), Some(base)) = (&target.remote, &target.remote_base) else {
        return Ok(Vec::new());
    };
    let prefix = format!("refs/remotes/{remote}/");
    let merged = format!("--merged={}", base.reference);
    let rows = records(&target.path, &prefix, REMOTE_FORMAT, &[&merged]).await?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let reference = field(row, 0);
            let name = reference.strip_prefix(&prefix)?.to_string();
            if name == "HEAD" || target.protected.contains(&name) {
                return None;
            }
            Some(RemoteCandidate {
                oid: field(row, 1),
                last_commit: field(row, 2).parse().unwrap_or(0),
                subject: field(row, 3),
                name,
            })
        })
        .collect())
}

pub(super) async fn build(target: Target) -> Result<MergedBranches, String> {
    let local = local_candidates(&target).await?;
    let remote_branches = remote_candidates(&target).await?;
    Ok(MergedBranches {
        base: short(&target.base.reference).to_string(),
        base_name: target.base.name,
        remote: target.remote,
        remote_base: target
            .remote_base
            .map(|base| short(&base.reference).to_string()),
        current: target.current,
        local,
        remote_branches,
    })
}
