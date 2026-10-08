use crate::file_guard::PinnedPath;
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};
use super::journal::{Journal, Record, WritePolicy};
use super::tickets::{lock_context, Service, Ticket};
use super::{contents, identity, io, locked_file, pointer_bytes, recovery_directory, unique, ENABLE_WRITES};

pub(super) struct FrozenCopy { ticket: Ticket, source: Option<Ticket>, bytes: Vec<u8> }
pub(super) struct CopyPlan {
    entries: Vec<FrozenCopy>, started: std::sync::atomic::AtomicBool, pub(super) cancel: std::sync::atomic::AtomicBool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyPreview { id: String, files: Vec<super::CopyPreviewFile>, retained: usize }
#[derive(Serialize)]
pub struct CopyPreviewFile { path: String, action: String, bytes: usize }
#[derive(Serialize)]
pub struct CopyOutcome { path: String, state: String, record: Option<Record>, error: Option<String> }
pub(super) async fn preview(app: tauri::AppHandle, comparisons: tauri::State<'_, crate::compare::Service>, service: tauri::State<'_, Service>,
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
pub(super) async fn cancel(service: tauri::State<'_, Service>, id: String) -> Result<bool, String> {
    let mut copies = service.copies.lock().await;
    let Some(plan) = copies.get(&id) else { return Ok(false); };
    plan.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
    if !plan.started.load(std::sync::atomic::Ordering::SeqCst) { copies.remove(&id); }
    Ok(true)
}
pub(super) async fn apply(app: tauri::AppHandle, comparisons: tauri::State<'_, crate::compare::Service>, service: tauri::State<'_, Service>,
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
