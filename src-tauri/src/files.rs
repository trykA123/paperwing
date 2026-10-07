#![cfg(windows)]

mod journal;

pub use journal::{contents, locked_file, Journal, Record};
pub(crate) use journal::identity;

use crate::file_guard::PinnedPath;
use journal::{io, WritePolicy};
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};

const ENABLE_WRITES: bool = true;
static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
fn unique() -> String {
    format!("{}-{}-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos(), std::process::id(), SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}
#[derive(Clone)]
struct Ticket {
    session: String,
    generation: u64,
    file_id: String,
    side: String,
    root: PathBuf,
    path: String,
    root_identity: (u32, u64),
    expected: Option<Vec<u8>>,
    safe: crate::paths::ReadRoot,
    pointers: Vec<(PathBuf, Option<Vec<u8>>)>,
    undo_records: Vec<String>,
}

#[derive(Default)]
pub struct Service {
    tickets: tokio::sync::Mutex<std::collections::HashMap<String, Ticket>>,
    copies: tokio::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<CopyPlan>>>,
}

impl Service {
    pub async fn release_tickets(&self) {
        self.tickets.lock().await.clear();
        for plan in self.copies.lock().await.drain().map(|(_, plan)| plan) { plan.cancel.store(true, std::sync::atomic::Ordering::SeqCst); }
    }
}

struct FrozenCopy { ticket: Ticket, source: Option<Ticket>, bytes: Vec<u8> }
struct CopyPlan {
    entries: Vec<FrozenCopy>, started: std::sync::atomic::AtomicBool, cancel: std::sync::atomic::AtomicBool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyPreview { id: String, files: Vec<CopyPreviewFile>, retained: usize }
#[derive(Serialize)]
pub struct CopyPreviewFile { path: String, action: String, bytes: usize }
#[derive(Serialize)]
pub struct CopyOutcome { path: String, state: String, record: Option<Record>, error: Option<String> }

fn lock_context(entry: &Ticket) -> Result<(Vec<File>, Vec<File>), String> {
    if identity(&entry.root)? != entry.root_identity { return Err("Repository identity changed; preview again".into()); }
    let mut pointers = Vec::new(); let mut metadata = Vec::new();
    for directory in entry.safe.metadata_locations() {
        if directory.is_dir() { metadata.push(crate::file_guard::pin_metadata(directory)?); }
    }
    for (path, expected) in &entry.pointers {
        if pointer_bytes(path)? != *expected { return Err("Git metadata context changed; preview again".into()); }
        if expected.is_some() {
            let mut guard = OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).custom_flags(FILE_FLAG_OPEN_REPARSE_POINT).open(path).map_err(io)?;
            if Some(contents(&mut guard)?) != *expected { return Err("Git metadata pointer changed before it was locked".into()); }
            pointers.push(guard);
        }
    }
    entry.safe.resolve_cached(&entry.path, false, &mut crate::paths::ReadCache::default())?;
    Ok((pointers, metadata))
}

#[tauri::command]
pub async fn copy_preview(app: tauri::AppHandle, comparisons: tauri::State<'_, crate::compare::Service>, service: tauri::State<'_, Service>,
    id: String, generation: u64, file_id: String, side: String) -> Result<CopyPreview, String> {
    let source = match side.as_str() { "left" => "right", "right" => "left", _ => return Err("Unknown copy direction".into()) };
    let settings = crate::settings::load_settings(app.clone())?;
    let (ids, retained) = comparisons.copy_ids(&settings, &id, generation, &file_id, source).await?;
    let mut entries = Vec::new(); let mut files = Vec::new(); let mut total = 0usize;
    for file_id in ids {
        let bytes = crate::compare::comparison_content(app.clone(), comparisons.clone(), id.clone(), generation, file_id.clone(), source.into())
            .await.map_err(|problem| problem.message)?.bytes;
        let context = comparisons.write_context(&settings, &id, generation, &file_id, &side, true).await?;
        let source_context = comparisons.copy_source_context(&settings, &id, generation, &file_id, source).await?;
        let source_ticket = if let Some(context) = source_context {
            let _root = PinnedPath::existing_directory(&context.root)?;
            let mut pointers = vec![(context.root.join(".git"), pointer_bytes(&context.root.join(".git"))?)];
            for directory in context.safe.metadata_locations() {
                if directory.is_dir() { let path = directory.join("commondir"); pointers.push((path.clone(), pointer_bytes(&path)?)); }
            }
            Some(Ticket { session: id.clone(), generation, file_id: file_id.clone(), side: source.into(), root_identity: identity(&context.root)?,
                root: context.root, path: context.path, safe: context.safe, expected: None, pointers, undo_records: Vec::new() })
        } else { None };
        let _filesystem = crate::git::filesystem_gate().read().await;
        comparisons.write_context(&crate::settings::load_settings(app.clone())?, &id, generation, &file_id, &side, false).await?;
        let _root = PinnedPath::existing_directory(&context.root)?;
        let target = context.safe.resolve_cached(&context.path, false, &mut crate::paths::ReadCache::default())?;
        let expected = if target.exists() {
            let _parent = PinnedPath::relative_parent(&context.root, &context.path)?;
            locked_file(&target)?.as_mut().map(contents).transpose()?
        } else { None };
        total = total.saturating_add(bytes.len()).saturating_add(expected.as_ref().map_or(0, Vec::len));
        if total > 32 * 1024 * 1024 { return Err("Copy preview exceeds 32 MiB; select a smaller scope".into()); }
        let root_identity = identity(&context.root)?;
        let mut pointers = vec![(context.root.join(".git"), pointer_bytes(&context.root.join(".git"))?)];
        for directory in context.safe.metadata_locations() {
            if directory.is_dir() { let path = directory.join("commondir"); pointers.push((path.clone(), pointer_bytes(&path)?)); }
        }
        files.push(CopyPreviewFile { path: context.path.clone(), action: if expected.is_some() { "overwrite" } else { "create" }.into(), bytes: bytes.len() });
        entries.push(FrozenCopy { bytes, source: source_ticket, ticket: Ticket { session: id.clone(), generation, file_id, side: side.clone(), root: context.root,
            path: context.path, root_identity, expected, safe: context.safe, pointers, undo_records: Vec::new() } });
    }
    let mut copies = service.copies.lock().await;
    if copies.len() >= 4 { return Err("Too many pending copy previews; cancel one first".into()); }
    let id = unique(); copies.insert(id.clone(), std::sync::Arc::new(CopyPlan { entries, started: false.into(), cancel: false.into() }));
    Ok(CopyPreview { id, files, retained })
}

#[tauri::command]
pub async fn copy_cancel(service: tauri::State<'_, Service>, id: String) -> Result<bool, String> {
    let mut copies = service.copies.lock().await;
    let Some(plan) = copies.get(&id) else { return Ok(false); };
    plan.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
    if !plan.started.load(std::sync::atomic::Ordering::SeqCst) { copies.remove(&id); }
    Ok(true)
}

#[tauri::command]
pub async fn copy_apply(app: tauri::AppHandle, comparisons: tauri::State<'_, crate::compare::Service>, service: tauri::State<'_, Service>,
    id: String, confirmed: bool) -> Result<Vec<CopyOutcome>, String> {
    if !ENABLE_WRITES || !confirmed { return Err("Byte copies require explicit confirmation".into()); }
    let plan = service.copies.lock().await.get(&id).cloned().ok_or("Unknown or cancelled copy preview")?;
    if plan.started.swap(true, std::sync::atomic::Ordering::SeqCst) { return Err("Copy preview was already submitted".into()); }
    let mut outcomes = Vec::new(); let mut stopped = false;
    for entry in &plan.entries {
        if stopped || plan.cancel.load(std::sync::atomic::Ordering::SeqCst) {
            outcomes.push(CopyOutcome { path: entry.ticket.path.clone(), state: "notAttempted".into(), record: None, error: Some("Cancelled or stopped after a failure; destination retained".into()) }); continue;
        }
        let result = async {
            let _filesystem = crate::git::filesystem_gate().try_write().map_err(|_| "Git or another write is running; retry after it finishes")?;
            if crate::clone::busy() { return Err("Git operation is still running".into()); }
            let settings = crate::settings::load_settings(app.clone())?;
            let context = comparisons.write_context(&settings, &entry.ticket.session, entry.ticket.generation, &entry.ticket.file_id, &entry.ticket.side, false).await?;
            if context.root != entry.ticket.root || context.path != entry.ticket.path { return Err("Repository context changed; preview again".into()); }
            let _validation = PinnedPath::existing_directory(&entry.ticket.root)?;
            let _guards = lock_context(&entry.ticket)?;
            let mut source_guards = None;
            let mut source_file = None;
            if let Some(source) = &entry.source {
                if source.root == entry.ticket.root && source.path == entry.ticket.path { return Err("Source and destination are the same file".into()); }
                let root = PinnedPath::existing_directory(&source.root)?;
                let parent = PinnedPath::relative_parent(&source.root, &source.path)?;
                source_guards = Some(lock_context(source)?);
                let target = source.safe.resolve_cached(&source.path, false, &mut crate::paths::ReadCache::default())?;
                let mut file = OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).custom_flags(FILE_FLAG_OPEN_REPARSE_POINT).open(target).map_err(io)?;
                let metadata = file.metadata().map_err(io)?;
                if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 || contents(&mut file)? != entry.bytes {
                    return Err("Source changed after preview; destination retained".into());
                }
                source_file = Some(file); drop(parent); drop(root);
            }
            let directory = recovery_directory(&app)?;
            if fs::canonicalize(&directory).map_err(io)?.starts_with(fs::canonicalize(&entry.ticket.root).map_err(io)?) { return Err("Recovery storage must be outside the repository".into()); }
            let mut journal = Journal::open(&directory)?;
            drop(_validation);
            let result = journal.replace_authorized(&entry.ticket.root, &entry.ticket.path, entry.ticket.expected.as_deref(), &entry.bytes,
                WritePolicy { identity: Some(entry.ticket.root_identity), recovery: false });
            drop(source_file); drop(source_guards); result
        }.await;
        match result {
            Ok(record) => outcomes.push(CopyOutcome { path: entry.ticket.path.clone(), state: "applied".into(), record: Some(record), error: None }),
            Err(error) => { stopped = true; outcomes.push(CopyOutcome { path: entry.ticket.path.clone(), state: "failed".into(), record: None, error: Some(error) }); }
        }
        tokio::task::yield_now().await;
    }
    service.copies.lock().await.remove(&id);
    Ok(outcomes)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditFile { pub ticket: String, pub bytes: Vec<u8>, pub exists: bool }

fn recovery_directory(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let directory = app.path().app_data_dir().map_err(|error| error.to_string())?.join("recovery-v2");
    fs::create_dir_all(&directory).map_err(io)?;
    PinnedPath::existing_directory(&directory)?;
    Ok(directory)
}

fn pointer_bytes(path: &Path) -> Result<Option<Vec<u8>>, String> {
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