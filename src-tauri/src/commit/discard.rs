use super::{content, snapshot, stage};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub(crate) struct DiscardWrite {
    pub root: PathBuf,
    pub file: String,
    pub expected: Option<Vec<u8>>,
    pub bytes: Vec<u8>,
    pub index_state: snapshot::IndexState,
}

#[derive(Debug)]
pub(crate) struct DiscardRecovery {
    pub id: String,
    pub warning: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscardFile {
    pub file: String,
    pub orig_path: Option<String>,
    pub content_hash: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardOutcome {
    pub file: String,
    pub state: &'static str,
    pub recovery_id: Option<String>,
    pub message: String,
    pub warning: Option<String>,
}

async fn restore(app: tauri::AppHandle, write: DiscardWrite) -> Result<DiscardRecovery, String> {
    #[cfg(windows)]
    return crate::files::discard_restore(app, write).await;
    #[cfg(target_os = "linux")]
    return crate::linux_files::discard_restore(app, write).await;
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (app, write);
        Err(crate::platform::unavailable_reason("recovery"))
    }
}

pub(crate) enum DiscardPlan {
    Restore(DiscardWrite),
    Trash(DiscardWrite),
}

pub(crate) async fn prepare_file(path: &str, request: &DiscardFile) -> Result<DiscardPlan, String> {
    let snapshot = read_file(path, request).await?;
    let mut write = DiscardWrite {
        root: path.into(),
        file: request.file.clone(),
        expected: snapshot.working,
        bytes: Vec::new(),
        index_state: snapshot.index_state,
    };
    if snapshot.index.is_some() {
        write.bytes = content::smudge(path, &request.file, &format!(":0:{}", request.file)).await?;
        if write.expected.as_deref() == Some(&write.bytes) {
            return Err("File has no working changes to discard".into());
        }
        return Ok(DiscardPlan::Restore(write));
    }
    if !snapshot.intent_to_add {
        let changes = super::repo_changes(path.into()).await?;
        if !changes
            .files
            .iter()
            .any(|file| file.path == request.file && file.kind == "untracked")
        {
            return Err("File has no index version to restore; refresh the diff".into());
        }
    }
    if write.expected.is_none() {
        return Err("Untracked file is missing".into());
    }
    Ok(DiscardPlan::Trash(write))
}

async fn discard_file(
    app: tauri::AppHandle,
    path: &str,
    request: &DiscardFile,
) -> Result<DiscardOutcome, String> {
    match prepare_file(path, request).await? {
        DiscardPlan::Restore(write) => {
            let recovery = restore(app, write).await?;
            Ok(DiscardOutcome {
                file: request.file.clone(),
                state: "discarded",
                recovery_id: Some(recovery.id),
                message: "Changes discarded; undo in Recovery".into(),
                warning: recovery.warning,
            })
        }
        DiscardPlan::Trash(write) => {
            let message = crate::trash::trash_untracked(app, write).await?;
            Ok(DiscardOutcome {
                file: request.file.clone(),
                state: "trashed",
                recovery_id: None,
                message,
                warning: None,
            })
        }
    }
}

async fn read_file(path: &str, request: &DiscardFile) -> Result<snapshot::Snapshot, String> {
    let snapshot = snapshot::read(
        path,
        &request.file,
        request.orig_path.as_deref(),
        "unstaged",
    )
    .await?;
    snapshot::require_hash(&snapshot, &request.content_hash)?;
    Ok(snapshot)
}

#[tauri::command]
pub async fn discard_files(
    app: tauri::AppHandle,
    path: String,
    files: Vec<DiscardFile>,
    confirmed: bool,
) -> Result<Vec<DiscardOutcome>, String> {
    if !confirmed {
        return Err("Discard requires explicit confirmation naming every file".into());
    }
    if files.is_empty() || files.len() > super::MAX_FILES {
        return Err("Select between 1 and 2000 files".into());
    }
    crate::git::valid_root(&path)?;
    let mut seen = std::collections::HashSet::new();
    for file in &files {
        crate::paths::relative(&file.file)?;
        if !seen.insert(&file.file) {
            return Err("Duplicate discard file".into());
        }
    }
    let _action = stage::ACTIONS.lock().await;
    super::idle_check()?;
    let mut results = Vec::new();
    for file in files {
        let result = discard_file(app.clone(), &path, &file)
            .await
            .unwrap_or_else(|message| DiscardOutcome {
                file: file.file,
                state: "failed",
                recovery_id: None,
                message,
                warning: None,
            });
        results.push(result);
    }
    Ok(results)
}

#[tauri::command]
pub async fn discard_hunk(
    app: tauri::AppHandle,
    path: String,
    request: stage::HunkRequest,
    confirmed: bool,
) -> Result<DiscardOutcome, String> {
    if !confirmed {
        return Err("Discard requires explicit confirmation of the selected hunk".into());
    }
    if request.area != "unstaged" || request.hunks.len() != 1 || request.hunks[0].ranges.is_some() {
        return Err("Select one whole working-tree hunk to discard".into());
    }
    let _action = stage::ACTIONS.lock().await;
    super::idle_check()?;
    let snapshot = snapshot::read(
        &path,
        &request.file,
        request.orig_path.as_deref(),
        &request.area,
    )
    .await?;
    snapshot::require_hash(&snapshot, &request.content_hash)?;
    if snapshot.index.is_none() {
        return Err("Untracked files must be moved whole to the Recycle Bin or Trash".into());
    }
    let bytes = hunk_bytes(&path, &snapshot, &request).await?;
    let recovery = restore(
        app,
        DiscardWrite {
            root: path.into(),
            file: request.file.clone(),
            expected: snapshot.working,
            bytes,
            index_state: snapshot.index_state,
        },
    )
    .await?;
    Ok(DiscardOutcome {
        file: request.file,
        state: "discarded",
        recovery_id: Some(recovery.id),
        message: "Hunk discarded; undo in Recovery".into(),
        warning: recovery.warning,
    })
}

pub(crate) async fn hunk_bytes(
    _path: &str,
    snapshot: &snapshot::Snapshot,
    request: &stage::HunkRequest,
) -> Result<Vec<u8>, String> {
    let before = snapshot
        .before
        .as_deref()
        .ok_or("Missing index content to restore")?;
    let after = snapshot.after.as_deref().ok_or("Missing diff content")?;
    let working = snapshot
        .working
        .as_deref()
        .ok_or("Missing working file content")?;
    let diff = super::patch::Diff::new(before, after)?;
    diff.rebuild_working(working, &request.hunks)
}

#[cfg(test)]
mod review_tests;
#[cfg(test)]
mod tests;
