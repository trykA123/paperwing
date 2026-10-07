use super::{patch, run, snapshot};
use crate::git::OutputPolicy;
use serde::Deserialize;
use std::time::Duration;

pub(crate) static ACTIONS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HunkRequest {
    pub file: String,
    pub orig_path: Option<String>,
    pub area: String,
    pub content_hash: String,
    pub hunks: Vec<patch::Selection>,
}

pub(crate) fn build(
    snapshot: &snapshot::Snapshot,
    request: &HunkRequest,
    reverse: bool,
) -> Result<patch::Built, String> {
    patch::Diff::new(
        snapshot.before.as_deref().unwrap_or_default(),
        snapshot.after.as_deref().unwrap_or_default(),
    )?
    .build(
        patch::Paths {
            old: &snapshot.old_path,
            new: &request.file,
            before: snapshot.before.is_some(),
            after: snapshot.after.is_some(),
            mode: &snapshot.mode,
        },
        &request.hunks,
        reverse,
    )
}

async fn apply(path: &str, request: HunkRequest, reverse: bool) -> Result<(), String> {
    super::idle_check()?;
    if (reverse && request.area != "staged")
        || (!reverse && !matches!(request.area.as_str(), "unstaged" | "untracked"))
    {
        return Err("Stage working changes; unstage staged changes".into());
    }
    let _action = ACTIONS.lock().await;
    let snapshot = snapshot::read(
        path,
        &request.file,
        request.orig_path.as_deref(),
        &request.area,
    )
    .await?;
    snapshot::require_hash(&snapshot, &request.content_hash)?;
    let built = build(&snapshot, &request, reverse)?;
    let mut args = vec!["apply", "--cached", "--whitespace=nowarn"];
    if reverse {
        args.push("--reverse");
    }
    args.push("--check");
    run(
        path,
        &args,
        "Check selected patch",
        &[0],
        OutputPolicy::Text,
        Some(&built.patch),
        Duration::from_secs(60),
    )
    .await?;
    let current = snapshot::read(
        path,
        &request.file,
        request.orig_path.as_deref(),
        &request.area,
    )
    .await?;
    snapshot::require_hash(&current, &request.content_hash)?;
    super::idle_check()?;
    args.pop();
    run(
        path,
        &args,
        "Apply selected patch",
        &[0],
        OutputPolicy::Text,
        Some(&built.patch),
        Duration::from_secs(60),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn stage_hunks(path: String, request: HunkRequest) -> Result<(), String> {
    apply(&path, request, false).await
}

#[tauri::command]
pub async fn unstage_hunks(path: String, request: HunkRequest) -> Result<(), String> {
    apply(&path, request, true).await
}

#[cfg(test)]
mod tests;
