use super::git::list_entries;
use super::{
    ops, switch, ApplyOutcome, PushOutcome, Restore, StashDiff, StashEntry, SwitchOutcome,
};
use crate::git::valid_root;

fn idle_check() -> Result<(), String> {
    if crate::clone::busy() {
        return Err("A clone, fetch or pull is running; try again when it finishes".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn stash_list(path: String) -> Result<Vec<StashEntry>, String> {
    valid_root(&path)?;
    list_entries(&path).await
}

#[tauri::command]
pub async fn stash_push(
    path: String,
    message: Option<String>,
    include_untracked: bool,
) -> Result<PushOutcome, String> {
    valid_root(&path)?;
    idle_check()?;
    ops::push(&path, message.as_deref(), include_untracked).await
}

#[tauri::command]
pub async fn stash_apply(path: String, oid: String) -> Result<ApplyOutcome, String> {
    valid_root(&path)?;
    idle_check()?;
    ops::restore(&path, &oid, Restore::Apply).await
}

#[tauri::command]
pub async fn stash_pop(path: String, oid: String) -> Result<ApplyOutcome, String> {
    valid_root(&path)?;
    idle_check()?;
    ops::restore(&path, &oid, Restore::Pop).await
}

#[tauri::command]
pub async fn stash_drop(path: String, oid: String) -> Result<(), String> {
    valid_root(&path)?;
    idle_check()?;
    ops::drop_stash(&path, &oid).await
}

#[tauri::command]
pub async fn stash_show(path: String, oid: String) -> Result<StashDiff, String> {
    valid_root(&path)?;
    ops::show(&path, &oid).await
}

#[tauri::command]
pub async fn switch_with_stash(path: String, branch: String) -> Result<SwitchOutcome, String> {
    valid_root(&path)?;
    idle_check()?;
    switch::switch_with_stash(&path, &branch).await
}
