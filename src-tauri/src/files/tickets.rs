use super::copy::CopyPlan;
use super::{contents, identity, io, pointer_bytes};
use std::fs::{File, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;
use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};

#[derive(Clone)]
pub(super) struct Ticket {
    pub(super) session: String,
    pub(super) generation: u64,
    pub(super) file_id: String,
    pub(super) side: String,
    pub(super) root: PathBuf,
    pub(super) path: String,
    pub(super) root_identity: (u32, u64),
    pub(super) expected: Option<Vec<u8>>,
    pub(super) safe: crate::paths::ReadRoot,
    pub(super) pointers: Vec<(PathBuf, Option<Vec<u8>>)>,
    pub(super) undo_records: Vec<String>,
}

#[derive(Default)]
pub struct Service {
    pub(super) tickets: tokio::sync::Mutex<std::collections::HashMap<String, Ticket>>,
    pub(super) copies: tokio::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<CopyPlan>>>,
}

impl Service {
    pub async fn release_tickets(&self) {
        self.tickets.lock().await.clear();
        for plan in self.copies.lock().await.drain().map(|(_, plan)| plan) { plan.cancel.store(true, std::sync::atomic::Ordering::SeqCst); }
    }
}

pub(super) fn lock_context(entry: &Ticket) -> Result<(Vec<File>, Vec<File>), String> {
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
