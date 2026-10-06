use super::git::{is_dirty, quick};
use super::lock_repository;
use super::ops::push_locked;
use crate::git::valid_ref;
use serde::Serialize;

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SwitchOutcome {
    pub stashed: Option<String>,
    pub switched: bool,
    pub error: Option<String>,
}

enum Target {
    Local,
    Remote(String),
}

fn failed(stashed: Option<String>, error: String) -> SwitchOutcome {
    SwitchOutcome {
        stashed,
        switched: false,
        error: Some(error),
    }
}

async fn resolve_target(path: &str, branch: &str) -> Result<Target, String> {
    let local = format!("refs/heads/{branch}");
    let found = quick(
        path,
        &["rev-parse", "--verify", "--quiet", &local],
        &format!("Branch probe: {path}"),
        &[0, 1],
    )
    .await?;
    if found.code == Some(0) {
        return Ok(Target::Local);
    }
    let pattern = format!("refs/remotes/*/{branch}");
    let listed = quick(
        path,
        &["for-each-ref", "--format=%(refname)", &pattern],
        &format!("Remote branch probe: {path}"),
        &[0],
    )
    .await?;
    let suffix = format!("/{branch}");
    let text = String::from_utf8_lossy(&listed.stdout).into_owned();
    let matches: Vec<&str> = text
        .lines()
        .filter_map(|line| {
            line.strip_prefix("refs/remotes/")?
                .strip_suffix(suffix.as_str())
        })
        .filter(|remote| !remote.contains('/'))
        .collect();
    match matches[..] {
        [] => Err(format!("There is no branch named {branch}")),
        [remote] => Ok(Target::Remote(format!("{remote}/{branch}"))),
        _ => Err(format!(
            "Several remotes have a branch named {branch}; switch from a terminal to choose one"
        )),
    }
}

pub async fn switch_with_stash(path: &str, branch: &str) -> Result<SwitchOutcome, String> {
    valid_ref(branch).map_err(|_| "Invalid branch name".to_string())?;
    if branch.eq_ignore_ascii_case("head") {
        return Err("Invalid branch name".into());
    }
    let _guard = lock_repository(path).await;
    let target = resolve_target(path, branch).await?;
    let current = quick(
        path,
        &["symbolic-ref", "--short", "-q", "HEAD"],
        &format!("Current branch: {path}"),
        &[0, 1],
    )
    .await?;
    if matches!(target, Target::Local) && String::from_utf8_lossy(&current.stdout).trim() == branch
    {
        return Ok(SwitchOutcome {
            stashed: None,
            switched: true,
            error: None,
        });
    }
    let mut stashed = None;
    if is_dirty(path, true).await? {
        let message = format!("Skein: before switching to {branch}");
        match push_locked(path, Some(&message), true).await {
            Ok(outcome) => stashed = outcome.stashed,
            Err(error) => return Ok(failed(None, error)),
        }
    }
    let remote_name;
    let args: Vec<&str> = match &target {
        Target::Local => vec!["switch", branch],
        Target::Remote(name) => {
            remote_name = name.clone();
            vec!["switch", "--track", &remote_name]
        }
    };
    match quick(path, &args, &format!("Switch: {path}"), &[0]).await {
        Ok(_) => Ok(SwitchOutcome {
            stashed,
            switched: true,
            error: None,
        }),
        Err(error) => Ok(failed(stashed, error)),
    }
}
