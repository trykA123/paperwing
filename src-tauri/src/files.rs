#![cfg(windows)]

mod copy;
mod discard;
mod journal;
mod tickets;

pub use copy::{CopyOutcome, CopyPreview, CopyPreviewFile};
pub use journal::{contents, locked_file, Journal, Record};
pub use tickets::Service;
pub(crate) use discard::discard_restore;
pub(crate) use journal::identity;

use crate::file_guard::PinnedPath;
use journal::{io, WritePolicy};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tickets::Ticket;
use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};

pub(super) const ENABLE_WRITES: bool = true;
static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub(super) fn unique() -> String {
    format!("{}-{}-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos(), std::process::id(), SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

#[tauri::command]
pub async fn copy_preview(app: tauri::AppHandle, comparisons: tauri::State<'_, crate::compare::Service>, service: tauri::State<'_, Service>,
    id: String, generation: u64, file_id: String, side: String) -> Result<CopyPreview, String> {
    copy::preview(app, comparisons, service, id, generation, file_id, side).await
}

#[tauri::command]
pub async fn copy_cancel(service: tauri::State<'_, Service>, id: String) -> Result<bool, String> {
    copy::cancel(service, id).await
}

#[tauri::command]
pub async fn copy_apply(app: tauri::AppHandle, comparisons: tauri::State<'_, crate::compare::Service>, service: tauri::State<'_, Service>,
    id: String, confirmed: bool) -> Result<Vec<CopyOutcome>, String> {
    copy::apply(app, comparisons, service, id, confirmed).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditFile { pub ticket: String, pub bytes: Vec<u8>, pub exists: bool }
pub(super) fn recovery_directory(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let directory = app.path().app_data_dir().map_err(|error| error.to_string())?.join("recovery-v2");
    fs::create_dir_all(&directory).map_err(io)?;
    PinnedPath::existing_directory(&directory)?;
    Ok(directory)
}

pub(super) fn pointer_bytes(path: &Path) -> Result<Option<Vec<u8>>, String> {
    if path.is_dir() { return Ok(None); }
    let Some(mut file) = locked_file(path)? else { return Ok(None); };
    if file.metadata().map_err(io)?.len() > 4096 { return Err("Oversized Git metadata pointer".into()); }
    Ok(Some(contents(&mut file)?))
}
#[tauri::command]
pub async fn file_edit_open(
    app: tauri::AppHandle,
    comparisons: tauri::State<'_, crate::compare::Service>,
    service: tauri::State<'_, Service>,
    id: String, generation: u64, file_id: String, side: String,
) -> Result<EditFile, String> {
    let settings = crate::settings::load_settings(app.clone())?;
    let context = comparisons.write_context(&settings, &id, generation, &file_id, &side, true).await?;
    let _filesystem = crate::git::filesystem_gate().read().await;
    comparisons.write_context(&crate::settings::load_settings(app)?, &id, generation, &file_id, &side, false).await?;
    let root_guard = PinnedPath::existing_directory(&context.root)?;
    let target = context.safe.resolve_cached(&context.path, false, &mut crate::paths::ReadCache::default())?;
    let expected = if target.exists() {
        let _parent = PinnedPath::relative_parent(&context.root, &context.path)?;
        locked_file(&target)?.as_mut().map(contents).transpose()?
    } else { None };
    if expected.as_ref().is_some_and(|bytes| bytes.len() > crate::paths::CONTENT_LIMIT) { return Err("File exceeds editor limit".into()); }
    let root_identity = identity(&context.root)?;
    let mut pointers = vec![(context.root.join(".git"), pointer_bytes(&context.root.join(".git"))?)];
    for directory in context.safe.metadata_locations() {
        if directory.is_dir() { let path = directory.join("commondir"); pointers.push((path.clone(), pointer_bytes(&path)?)); }
    }
    drop(root_guard);
    let ticket = unique();
    let result = EditFile { ticket: ticket.clone(), bytes: expected.clone().unwrap_or_default(), exists: expected.is_some() };
    let mut tickets = service.tickets.lock().await;
    if tickets.len() >= 32 { return Err("Too many open editable files; close an editor first".into()); }
    tickets.insert(ticket, Ticket { session: id, generation, file_id, side, root: context.root, path: context.path,
        root_identity, expected, safe: context.safe, pointers, undo_records: Vec::new() });
    Ok(result)
}

#[tauri::command]
pub async fn file_edit_close(service: tauri::State<'_, Service>, ticket: String) -> Result<bool, String> {
    Ok(service.tickets.lock().await.remove(&ticket).is_some())
}

#[tauri::command]
pub async fn file_save(
    app: tauri::AppHandle, comparisons: tauri::State<'_, crate::compare::Service>,
    service: tauri::State<'_, Service>, ticket: String, bytes: Vec<u8>,
) -> Result<Record, String> {
    if !ENABLE_WRITES { return Err("Write safety review is pending".into()); }
    if bytes.len() > crate::paths::CONTENT_LIMIT { return Err("File exceeds editor limit".into()); }
    let _filesystem = crate::git::filesystem_gate().try_write().map_err(|_| "Git or another write is running; retry after it finishes")?;
    if crate::clone::busy() { return Err("Git operation is still running; retry the write when it finishes".into()); }
    let mut tickets = service.tickets.lock().await;
    let entry = tickets.get(&ticket).cloned().ok_or("Unknown or closed edit ticket")?;
    if entry.expected.as_ref().is_some_and(|bytes| bytes.contains(&0) || std::str::from_utf8(bytes).is_err())
        || bytes.contains(&0) || std::str::from_utf8(&bytes).is_err() { return Err("Editor saves support UTF-8 text only".into()); }
    let settings = crate::settings::load_settings(app.clone())?;
    let context = comparisons.write_context(&settings, &entry.session, entry.generation, &entry.file_id, &entry.side, false).await?;
    let validation_guard = PinnedPath::existing_directory(&entry.root)?;
    if context.root != entry.root || context.path != entry.path || identity(&entry.root)? != entry.root_identity {
        return Err("Repository context changed; reopen the comparison".into());
    }
    let mut pointer_guards = Vec::new();
    let mut metadata_guards = Vec::new();
    for directory in entry.safe.metadata_locations() {
        if directory.is_dir() { metadata_guards.push(crate::file_guard::pin_metadata(directory)?); }
    }
    for (path, expected) in &entry.pointers {
        if pointer_bytes(path)? != *expected { return Err("Git metadata context changed; reopen comparison".into()); }
        if expected.is_some() {
            let mut guard = OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).custom_flags(FILE_FLAG_OPEN_REPARSE_POINT).open(path).map_err(io)?;
            if Some(contents(&mut guard)?) != *expected { return Err("Git metadata pointer changed before it was locked".into()); }
            pointer_guards.push(guard);
        }
    }
    entry.safe.resolve_cached(&entry.path, false, &mut crate::paths::ReadCache::default())?;
    let directory = recovery_directory(&app)?;
    if fs::canonicalize(&directory).map_err(io)?.starts_with(fs::canonicalize(&entry.root).map_err(io)?) {
        return Err("Recovery storage must be outside the repository".into());
    }
    let mut journal = Journal::open(&directory)?;
    drop(validation_guard);
    let record = journal.replace_authorized(&entry.root, &entry.path, entry.expected.as_deref(), &bytes,
        WritePolicy { identity: Some(entry.root_identity), recovery: false })?;
    if let Some(entry) = tickets.get_mut(&ticket) {
        entry.expected = Some(bytes); entry.undo_records.push(record.id.clone());
        if entry.undo_records.len() > 32 { entry.undo_records.remove(0); }
    }
    Ok(record)
}

#[tauri::command]
pub async fn recovery_list(app: tauri::AppHandle) -> Result<Vec<Record>, String> {
    let _filesystem = crate::git::filesystem_gate().read().await;
    Journal::open(&recovery_directory(&app)?)?.list()
}

#[tauri::command]
pub async fn recovery_undo(app: tauri::AppHandle, id: String) -> Result<Record, String> {
    if !ENABLE_WRITES { return Err("Write safety review is pending".into()); }
    let record = {
        let _filesystem = crate::git::filesystem_gate().read().await;
        Journal::open(&recovery_directory(&app)?)?.get(&id)?
    };
    let settings = crate::settings::load_settings(app.clone())?;
    let safe = crate::compare::registered_write_root(&settings, &record.root, &record.path).await?;
    let _filesystem = crate::git::filesystem_gate().try_write().map_err(|_| "Git or another write is running; retry recovery after it finishes")?;
    if crate::clone::busy() { return Err("Git operation is still running; retry recovery when it finishes".into()); }
    if serde_json::to_value(&crate::settings::load_settings(app.clone())?).map_err(|error| error.to_string())?
        != serde_json::to_value(&settings).map_err(|error| error.to_string())? { return Err("Settings changed during recovery; retry".into()); }
    safe.resolve_cached(&record.path, false, &mut crate::paths::ReadCache::default())?;
    let mut journal = Journal::open(&recovery_directory(&app)?)?;
    journal.undo(&id)
}

#[tauri::command]
pub async fn recovery_cleanup(app: tauri::AppHandle, service: tauri::State<'_, Service>, ids: Vec<String>, confirmed: bool) -> Result<usize, String> {
    if !ENABLE_WRITES || !confirmed { return Err("Cleanup requires approved writes and explicit confirmation".into()); }
    if ids.len() > 1024 { return Err("Cleanup request exceeds limit".into()); }
    let _filesystem = crate::git::filesystem_gate().try_write().map_err(|_| "Git or another write is running; retry cleanup after it finishes")?;
    let tickets = service.tickets.lock().await;
    if ids.iter().any(|id| tickets.values().any(|ticket| ticket.undo_records.contains(id))) {
        return Err("Close editors referencing these undo records before cleanup".into());
    }
    Journal::open(&recovery_directory(&app)?)?.cleanup(&ids)
}

#[tauri::command]
pub async fn recovery_resolve(app: tauri::AppHandle, id: String, confirmed: bool) -> Result<Record, String> {
    if !ENABLE_WRITES || !confirmed { return Err("Conflict resolution requires explicit confirmation".into()); }
    let _filesystem = crate::git::filesystem_gate().try_write().map_err(|_| "Another operation is running; retry later")?;
    Journal::open(&recovery_directory(&app)?)?.resolve_conflict(&id)
}