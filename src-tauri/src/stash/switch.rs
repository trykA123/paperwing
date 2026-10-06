use super::{is_dirty, push, quick};
use crate::git::valid_ref;
use serde::Serialize;

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SwitchOutcome {
    pub stashed: Option<String>,
    pub switched: bool,
    pub error: Option<String>,
}

fn failed(stashed: Option<String>, error: String) -> SwitchOutcome {
    SwitchOutcome {
        stashed,
        switched: false,
        error: Some(error),
    }
}

async fn branch_exists(path: &str, branch: &str) -> Result<bool, String> {
    let local = format!("refs/heads/{branch}");
    let found = quick(
        path,
        &["rev-parse", "--verify", "--quiet", &local],
        &format!("Branch probe: {path}"),
        &[0, 1],
    )
    .await?;
    if found.code == Some(0) {
        return Ok(true);
    }
    let remote = format!("refs/remotes/*/{branch}");
    let listed = quick(
        path,
        &["for-each-ref", "--count=1", "--format=%(refname)", &remote],
        &format!("Remote branch probe: {path}"),
        &[0],
    )
    .await?;
    Ok(!listed.stdout.is_empty())
}

pub async fn switch_with_stash(path: &str, branch: &str) -> Result<SwitchOutcome, String> {
    valid_ref(branch).map_err(|_| "Invalid branch name".to_string())?;
    if !branch_exists(path, branch).await? {
        return Err(format!("There is no branch named {branch}"));
    }
    let current = quick(
        path,
        &["symbolic-ref", "--short", "-q", "HEAD"],
        &format!("Current branch: {path}"),
        &[0, 1],
    )
    .await?;
    if String::from_utf8_lossy(&current.stdout).trim() == branch {
        return Ok(SwitchOutcome {
            stashed: None,
            switched: true,
            error: None,
        });
    }
    let mut stashed = None;
    if is_dirty(path, true).await? {
        let message = format!("PaperWing: before switching to {branch}");
        match push(path, Some(&message), true).await {
            Ok(outcome) => stashed = outcome.stashed,
            Err(error) => return Ok(failed(None, error)),
        }
    }
    match quick(path, &["switch", branch], &format!("Switch: {path}"), &[0]).await {
        Ok(_) => Ok(SwitchOutcome {
            stashed,
            switched: true,
            error: None,
        }),
        Err(error) => Ok(failed(stashed, error)),
    }
}
