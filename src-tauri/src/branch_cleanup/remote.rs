use super::delete::is_ancestor;
use super::resolve::{short, valid_branch, valid_oid, Target};
use super::BranchOutcome;
use crate::commit::run;
use crate::git::OutputPolicy;
use std::collections::HashMap;
use std::time::Duration;

async fn verdict(target: &Target, base: &str, name: &str, oid: &str) -> Result<(), String> {
    valid_branch(name)?;
    if target.protected.contains(name) {
        return Err(format!("{name} is protected"));
    }
    if !valid_oid(oid) {
        return Err(format!("{name} needs its expected commit"));
    }
    if !is_ancestor(&target.path, oid, base).await? {
        return Err(format!("{name} is not merged into {}", short(base)));
    }
    Ok(())
}

fn porcelain_error(remote: &str, name: &str, summary: &str) -> String {
    if summary.contains("stale info") {
        return format!("{name} changed on {remote} since it was listed; refresh");
    }
    match summary.split_once("] (") {
        Some((_, reason)) => reason.trim_end_matches(')').to_string(),
        None => format!("{name} was rejected by {remote}: {}", summary.trim()),
    }
}

fn parse_porcelain(remote: &str, stdout: &str) -> HashMap<String, Result<(), String>> {
    let mut results = HashMap::new();
    for line in stdout.lines() {
        let mut parts = line.splitn(3, '\t');
        let (Some(flag), Some(spec), Some(summary)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let Some(name) = spec
            .rsplit_once(':')
            .and_then(|(_, to)| to.strip_prefix("refs/heads/"))
        else {
            continue;
        };
        let outcome = match flag {
            "-" => Ok(()),
            _ => Err(porcelain_error(remote, name, summary)),
        };
        results.insert(name.to_string(), outcome);
    }
    results
}

async fn push_deletes(
    target: &Target,
    remote: &str,
    pending: &[(&str, &str)],
) -> HashMap<String, Result<(), String>> {
    let path = &target.path;
    let leases: Vec<String> = pending
        .iter()
        .map(|(name, oid)| format!("--force-with-lease=refs/heads/{name}:{oid}"))
        .collect();
    let refs: Vec<String> = pending
        .iter()
        .map(|(name, _)| format!("refs/heads/{name}"))
        .collect();
    let mut args = vec!["push", "--porcelain", remote];
    args.extend(leases.iter().map(String::as_str));
    args.push("--delete");
    args.extend(refs.iter().map(String::as_str));
    let pushed = run(
        path,
        &args,
        &format!("Delete remote branches: {path}"),
        &[0, 1],
        OutputPolicy::Text,
        None,
        Duration::from_secs(180),
    )
    .await;
    match pushed {
        Ok(output) => {
            let mut parsed = parse_porcelain(remote, &String::from_utf8_lossy(&output.stdout));
            let fallback = output.last_error();
            for (name, _) in pending {
                parsed
                    .entry(name.to_string())
                    .or_insert_with(|| Err(fallback.clone()));
            }
            parsed
        }
        Err(error) => pending
            .iter()
            .map(|(name, _)| (name.to_string(), Err(error.clone())))
            .collect(),
    }
}

pub(super) async fn delete_batch(
    target: &Target,
    names: &[String],
    expected: &[String],
) -> Result<Vec<BranchOutcome>, String> {
    let remote = target.remote.as_deref().ok_or("Choose a remote")?;
    let base = target.remote_base.as_ref().ok_or("Choose a remote base")?;
    let mut checked: Vec<Result<(), String>> = Vec::with_capacity(names.len());
    for (index, (name, oid)) in names.iter().zip(expected).enumerate() {
        let result = if names[..index].contains(name) {
            Err(format!("{name} is listed twice"))
        } else {
            verdict(target, &base.reference, name, oid).await
        };
        checked.push(result);
    }
    let pending: Vec<(&str, &str)> = names
        .iter()
        .zip(expected)
        .zip(&checked)
        .filter(|(_, verdict)| verdict.is_ok())
        .map(|((name, oid), _)| (name.as_str(), oid.as_str()))
        .collect();
    let mut pushed = if pending.is_empty() {
        HashMap::new()
    } else {
        push_deletes(target, remote, &pending).await
    };
    Ok(names
        .iter()
        .zip(checked)
        .map(|(name, verdict)| {
            let result = match verdict {
                Ok(()) => pushed
                    .remove(name)
                    .unwrap_or_else(|| Err(format!("{name} was not reported by {remote}"))),
                Err(error) => Err(error),
            };
            BranchOutcome {
                name: name.clone(),
                deleted: result.is_ok(),
                error: result.err(),
            }
        })
        .collect())
}
