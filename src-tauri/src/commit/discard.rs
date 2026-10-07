use super::{snapshot, stage};
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

async fn discard_file(
    app: tauri::AppHandle,
    path: &str,
    request: &DiscardFile,
) -> Result<DiscardOutcome, String> {
    let snapshot = read_file(path, request).await?;
    if let Some(bytes) = snapshot.index {
        if snapshot.working.as_deref() == Some(&bytes) {
            return Err("File has no working changes to discard".into());
        }
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
        return Ok(DiscardOutcome {
            file: request.file.clone(),
            state: "discarded",
            recovery_id: Some(recovery.id),
            message: "Changes discarded; undo in Recovery".into(),
            warning: recovery.warning,
        });
    }
    let changes = super::repo_changes(path.into()).await?;
    if !changes
        .files
        .iter()
        .any(|file| file.path == request.file && file.kind == "untracked")
    {
        return Err("File has no index version to restore; refresh the diff".into());
    }
    if snapshot.working.is_none() {
        return Err("Untracked file is missing".into());
    }
    let message = crate::trash::trash_untracked(
        app,
        DiscardWrite {
            root: path.into(),
            file: request.file.clone(),
            expected: snapshot.working,
            bytes: Vec::new(),
            index_state: snapshot.index_state,
        },
    )
    .await?;
    Ok(DiscardOutcome {
        file: request.file.clone(),
        state: "trashed",
        recovery_id: None,
        message,
        warning: None,
    })
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
    let bytes = stage::build(&snapshot, &request, true)?
        .content
        .ok_or("Hunk discard must retain the file")?;
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

#[cfg(test)]
mod tests;
