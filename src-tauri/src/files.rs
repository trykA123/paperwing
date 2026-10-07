#![cfg(windows)]

use crate::file_guard::{PinnedPath, Transaction};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ};

const FILE_LIMIT: usize = 64 * 1024 * 1024;
const STORAGE_LIMIT: u64 = 512 * 1024 * 1024;
const ENABLE_WRITES: bool = true;
static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Record {
    pub id: String,
    pub root: PathBuf,
    pub path: String,
    pub existed: bool,
    pub stage: String,
    pub created_at: u128,
    pub root_volume: u32,
    pub root_index: u64,
    #[serde(default)]
    pub warning: Option<String>,
}

fn unique() -> String {
    format!("{}-{}-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos(), std::process::id(), SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}
fn io(error: std::io::Error) -> String { error.to_string() }
fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |value, byte| (value ^ *byte as u64).wrapping_mul(0x100000001b3))
}
fn durable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path).map_err(io)?;
    file.write_all(bytes).map_err(io)?; file.sync_all().map_err(io)?;
    if fs::read(path).map_err(io)? != bytes { return Err("Backup verification failed".into()); }
    Ok(())
}
pub fn locked_file(path: &Path) -> Result<Option<File>, String> {
    let file = match OpenOptions::new().read(true).share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("File is locked or unreadable: {error}")),
    };
    let metadata = file.metadata().map_err(io)?;
    if metadata.file_attributes() & 0x400 != 0 || !metadata.is_file() { return Err("Linked, reparse or special files cannot be written".into()); }
    if metadata.len() > FILE_LIMIT as u64 { return Err("File exceeds safe operation limit".into()); }
    Ok(Some(file))
}
pub fn contents(file: &mut File) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    file.take(FILE_LIMIT as u64 + 1).read_to_end(&mut bytes).map_err(io)?;
    if bytes.len() > FILE_LIMIT { return Err("File exceeds safe operation limit".into()); }
    Ok(bytes)
}
pub(crate) fn identity(path: &Path) -> Result<(u32, u64), String> {
    use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS};
    let file = OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT).open(path).map_err(io)?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut info) } == 0 { return Err(io(std::io::Error::last_os_error())); }
    Ok((info.dwVolumeSerialNumber, (info.nFileIndexHigh as u64) << 32 | info.nFileIndexLow as u64))
}

pub struct Journal {
    directory: PathBuf, _guard: PinnedPath, _lock: File,
    #[cfg(test)]
    fault: Option<&'static str>,
}

fn preserve_security(source: &File, target: &File) -> Result<(), String> {
    use windows_sys::Win32::Security::{GetKernelObjectSecurity, SetKernelObjectSecurity};
    let mut size = 0;
    unsafe { GetKernelObjectSecurity(source.as_raw_handle().cast(), 7, std::ptr::null_mut(), 0, &mut size); }
    if size == 0 || size > 1024 * 1024 { return Err("Unsupported file security descriptor".into()); }
    let mut descriptor = vec![0u8; size as usize];
    if unsafe { GetKernelObjectSecurity(source.as_raw_handle().cast(), 7, descriptor.as_mut_ptr().cast(), size, &mut size) } == 0 {
        return Err(format!("Cannot read file security: {}", std::io::Error::last_os_error()));
    }
    if unsafe { SetKernelObjectSecurity(target.as_raw_handle().cast(), 7, descriptor.as_mut_ptr().cast()) } == 0 {
        return Err(format!("Cannot preserve file security: {}", std::io::Error::last_os_error()));
    }
    Ok(())
}

fn regular_stream(file: &File) -> Result<(), String> {
    use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandleEx, FileStreamInfo, FILE_STREAM_INFO};
    let mut buffer = vec![FILE_STREAM_INFO::default(); 2048];
    if unsafe { GetFileInformationByHandleEx(file.as_raw_handle().cast(), FileStreamInfo,
        buffer.as_mut_ptr().cast(), std::mem::size_of_val(buffer.as_slice()) as u32) } == 0 {
        return Err(format!("Cannot verify file streams: {}", std::io::Error::last_os_error()));
    }
    let stream = &buffer[0];
    let name = unsafe { std::slice::from_raw_parts(stream.StreamName.as_ptr(), stream.StreamNameLength as usize / 2) };
    if stream.NextEntryOffset != 0 || String::from_utf16_lossy(name) != "::$DATA" { return Err("Files with alternate streams are unsupported".into()); }
    Ok(())
}

fn preserve_attributes(source: &File, target: &File) -> Result<(), String> {
    use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandleEx, SetFileInformationByHandle, FileBasicInfo, FILE_BASIC_INFO};
    let mut information = FILE_BASIC_INFO::default();
    if unsafe { GetFileInformationByHandleEx(source.as_raw_handle().cast(), FileBasicInfo, (&mut information as *mut FILE_BASIC_INFO).cast(), std::mem::size_of_val(&information) as u32) } == 0 {
        return Err(format!("Cannot read file attributes: {}", std::io::Error::last_os_error()));
    }
    information.LastWriteTime = 0; information.ChangeTime = 0; information.LastAccessTime = 0;
    if unsafe { SetFileInformationByHandle(target.as_raw_handle().cast(), FileBasicInfo, (&information as *const FILE_BASIC_INFO).cast(), std::mem::size_of_val(&information) as u32) } == 0 {
        return Err(format!("Cannot preserve file attributes: {}", std::io::Error::last_os_error()));
    }
    Ok(())
}

#[derive(Default)]
struct WritePolicy { identity: Option<(u32, u64)>, recovery: bool }

impl Journal {
    pub fn open(directory: &Path) -> Result<Self, String> {
        let guard = PinnedPath::existing_directory(directory)?;
        let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false).share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT).open(directory.join("journal.lock")).map_err(io)?;
        if lock.metadata().map_err(io)?.file_attributes() & 0x400 != 0 { return Err("Linked recovery lock is unsupported".into()); }
        Ok(Self { directory: directory.to_path_buf(), _guard: guard, _lock: lock, #[cfg(test)] fault: None })
    }
    fn checkpoint(&self, phase: &str, id: &str) -> Result<(), String> {
        #[cfg(test)]
        if self.fault == Some(phase) { return Err(format!("Injected {phase} failure; recovery {id} retained")); }
        let _ = (phase, id);
        Ok(())
    }
    fn record_dir(&self, id: &str) -> Result<PathBuf, String> {
        if id.is_empty() || id.len() > 100 || !id.chars().all(|character| character.is_ascii_digit() || character == '-') { return Err("Unknown recovery record".into()); }
        Ok(self.directory.join(id))
    }
    fn store(&self, directory: &Path, record: &Record) -> Result<(), String> {
        let sequence = Self::states(directory)?.last().map_or(1, |(sequence, _)| sequence + 1);
        let payload = serde_json::to_vec(record).map_err(|error| error.to_string())?;
        let envelope = serde_json::to_vec(&(record, checksum(&payload))).map_err(|error| error.to_string())?;
        durable(&directory.join(format!("journal-{sequence}.json")), &envelope)
    }
    fn states(directory: &Path) -> Result<Vec<(u64, PathBuf)>, String> {
        let mut states = Vec::new();
        for entry in fs::read_dir(directory).map_err(io)? {
            let entry = entry.map_err(io)?; let name = entry.file_name(); let name = name.to_string_lossy();
            if let Some(sequence) = name.strip_prefix("journal-").and_then(|name| name.strip_suffix(".json")) {
                let sequence = sequence.parse::<u64>().map_err(|_| "Invalid journal sequence")?;
                if sequence >= 4096 { return Err("Journal state limit reached".into()); }
                states.push((sequence, entry.path()));
            }
        }
        states.sort_by_key(|(sequence, _)| *sequence); Ok(states)
    }
    fn latest(directory: &Path) -> Result<Record, String> {
        for (_, path) in Self::states(directory)?.into_iter().rev() {
            let mut file = locked_file(&path)?.ok_or("Recovery state missing")?;
            if file.metadata().map_err(io)?.len() > 64 * 1024 { return Err("Oversized recovery state".into()); }
            let Ok((record, hash)) = serde_json::from_slice::<(Record, u64)>(&contents(&mut file)?) else { continue; };
            let payload = serde_json::to_vec(&record).map_err(|error| error.to_string())?;
            if checksum(&payload) == hash { return Ok(record); }
        }
        Err("Incomplete backup requires inspection".into())
    }
    pub fn list(&self) -> Result<Vec<Record>, String> {
        let mut records = Vec::new();
        for entry in fs::read_dir(&self.directory).map_err(io)? {
            let entry = entry.map_err(io)?;
            if entry.file_name() == "journal.lock" { continue; }
            if !entry.file_type().map_err(io)?.is_dir() { return Err("Unexpected recovery entry; writes refused".into()); }
            let _guard = PinnedPath::existing_directory(&entry.path())?;
            if records.len() >= 1024 { return Err("Recovery journal limit reached".into()); }
            let record = match Self::latest(&entry.path()) {
                Ok(record) => record,
                Err(_) => {
                    records.push(Record { id: entry.file_name().to_string_lossy().into(), root: PathBuf::new(), path: String::new(),
                        existed: false, stage: "incomplete".into(), created_at: 0, root_volume: 0, root_index: 0,
                        warning: Some("Incomplete backup retained; other valid recovery records remain accessible".into()) });
                    continue;
                }
            };
            if self.record_dir(&record.id)? != entry.path() { return Err("Recovery identity mismatch".into()); }
            records.push(record);
        }
        records.sort_by_key(|record| record.created_at);
        Ok(records)
    }
    pub fn get(&self, id: &str) -> Result<Record, String> {
        let directory = self.record_dir(id)?;
        let _guard = PinnedPath::existing_directory(&directory)?;
        let record = Self::latest(&directory)?;
        if record.id != id { return Err("Recovery identity mismatch".into()); }
        Ok(record)
    }
    fn used(&self) -> Result<u64, String> {
        let mut total = 0;
        for entry in fs::read_dir(&self.directory).map_err(io)? {
            let entry = entry.map_err(io)?;
            if entry.file_name() == "journal.lock" { continue; }
            let _guard = PinnedPath::existing_directory(&entry.path())?;
            for file in fs::read_dir(entry.path()).map_err(io)? { total += file.map_err(io)?.metadata().map_err(io)?.len(); }
        }
        Ok(total)
    }
    #[cfg(test)]
    pub fn replace(&mut self, root: &Path, relative: &str, expected: Option<&[u8]>, bytes: &[u8]) -> Result<Record, String> {
        self.replace_authorized(root, relative, expected, bytes, WritePolicy::default())
    }
    fn replace_authorized(&mut self, root: &Path, relative: &str, expected: Option<&[u8]>, bytes: &[u8], policy: WritePolicy) -> Result<Record, String> {
        if bytes.len() > FILE_LIMIT { return Err("File exceeds safe operation limit".into()); }
        let mut parent = PinnedPath::create_parent(root, relative)?;
        let root_identity = identity(root)?;
        if policy.identity.is_some_and(|expected| expected != root_identity) { return Err("Repository identity changed after authorization".into()); }
        let target = parent.path.join(relative.split('/').next_back().ok_or("Missing file name")?);
        let transaction = Transaction::begin()?;
        let mut old_file = transaction.open(&target, false)?;
        if let Some(file) = &old_file {
            regular_stream(file)?;
            let metadata = file.metadata().map_err(io)?;
            if !metadata.is_file() || metadata.file_attributes() & (0x400 | 0x1 | 0x4000 | 0x200 | 0x800) != 0 {
                return Err("Linked, read-only, encrypted, sparse or compressed destinations are unsupported".into());
            }
        }
        let old = old_file.as_mut().map(contents).transpose()?;
        if old.is_some() && !fs::read_dir(&parent.path).map_err(io)?.filter_map(Result::ok)
            .any(|entry| entry.file_name() == target.file_name().unwrap_or_default()) {
            return Err("Case or short-name alias is unsupported".into());
        }
        if old.as_deref() != expected { return Err("File changed externally; refresh before writing".into()); }
        let records = self.list()?;
        if !policy.recovery && records.iter().any(|record| !["applied", "undone", "notApplied", "failed", "conflict"].contains(&record.stage.as_str())) { return Err("Pending recovery must be resolved before writing".into()); }
        if records.len() >= 1024 || self.used()? + (bytes.len() + old.as_ref().map_or(0, Vec::len)) as u64 * 3 > STORAGE_LIMIT { return Err("Recovery storage is full; no backups evicted".into()); }
        let id = unique(); let directory = self.record_dir(&id)?;
        fs::create_dir(&directory).map_err(io)?;
        let backup_guard = PinnedPath::existing_directory(&directory)?;
        let mut record = Record { id, root: root.to_path_buf(), path: relative.into(), existed: old.is_some(), stage: "prepared".into(),
            created_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(), root_volume: root_identity.0, root_index: root_identity.1, warning: None };
        let prepared = (|| -> Result<(), String> {
            self.checkpoint("beforeBackup", &record.id)?;
            if let Some(old) = &old { durable(&directory.join("before.bytes"), old)?; }
            self.checkpoint("afterFirstBackup", &record.id)?;
            durable(&directory.join("after.bytes"), bytes)?;
            durable(&directory.join("checksums.json"), &serde_json::to_vec(&(old.as_ref().map(|bytes| checksum(bytes)), checksum(bytes))).map_err(|error| error.to_string())?)?;
            self.store(&directory, &record)
        })();
        if let Err(error) = prepared {
            drop(backup_guard);
            return match self.remove_directory(&directory) {
                Ok(()) => Err(error),
                Err(cleanup) => Err(format!("{error}; incomplete recovery {} retained: {cleanup}", record.id)),
            };
        }
        let attempt = (|| -> Result<(), String> {
        self.checkpoint("backedUp", &record.id)?;
        let keeper_path = parent.path.join(format!(".skein-{}.lock", record.id));
        let keeper = OpenOptions::new().read(true).write(true).create_new(true).access_mode(0xc001_0000).share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | 0x04000000).open(&keeper_path).map_err(io)?;
        parent.permit_entry_update()?;
        let staged = parent.path.join(format!(".skein-{}.tmp", record.id));
        let mut staged_file = transaction.open(&staged, true)?.ok_or("Missing staged file")?;
        if let Some(old_file) = &old_file { preserve_security(old_file, &staged_file)?; preserve_attributes(old_file, &staged_file)?; }
        staged_file.write_all(bytes).map_err(io)?; staged_file.sync_all().map_err(io)?;
        drop(staged_file); drop(old_file.take());
        self.checkpoint("staged", &record.id)?;
        record.stage = "replacing".into(); self.store(&directory, &record)?;
        if let Err(error) = transaction.replace(&staged, &target, old.is_some())
            .and_then(|()| self.checkpoint("renamed", &record.id)).and_then(|()| transaction.commit()) {
            drop(transaction);
            return Err(format!("{error}; recovery {} retained", record.id));
        }
        drop(transaction); drop(keeper);
        self.checkpoint("committed", &record.id)?;
        record.stage = "applied".into(); self.store(&directory, &record)?;
        Ok(())
        })();
        if let Err(error) = attempt {
            drop(old_file.take());
            let _guard = PinnedPath::relative_parent(root, relative)?;
            let actual = locked_file(&target)?.as_mut().map(contents).transpose()?;
            record.warning = Some(error.clone());
            if actual.as_deref() == Some(bytes) {
                record.stage = "applied".into(); self.store(&directory, &record)?;
                return Ok(record);
            }
            if actual == old { record.stage = "notApplied".into(); self.store(&directory, &record)?; }
            else { record.stage = "replacing".into(); self.store(&directory, &record)?; }
            return Err(error);
        }
        Ok(record)
    }
    pub fn undo(&mut self, id: &str) -> Result<Record, String> {
        let directory = self.record_dir(id)?;
        let _guard = PinnedPath::existing_directory(&directory)?;
        let mut record = Self::latest(&directory)?;
        if record.id != id || !["applied", "replacing", "prepared"].contains(&record.stage.as_str()) { return Err("Recovery record is not eligible for automatic undo".into()); }
        let parent = PinnedPath::relative_parent(&record.root, &record.path)?;
        if identity(&record.root)? != (record.root_volume, record.root_index) { return Err("Repository identity changed; restore refused".into()); }
        let target = parent.path.join(record.path.split('/').next_back().ok_or("Missing file name")?);
        let before = if record.existed { Some(fs::read(directory.join("before.bytes")).map_err(io)?) } else { None };
        let after = fs::read(directory.join("after.bytes")).map_err(io)?;
        let hashes: (Option<u64>, u64) = serde_json::from_slice(&fs::read(directory.join("checksums.json")).map_err(io)?).map_err(|error| error.to_string())?;
        if (before.as_ref().map(|bytes| checksum(bytes)), checksum(&after)) != hashes { return Err("Recovery bytes failed verification".into()); }
        let mut current = locked_file(&target)?;
        let bytes = current.as_mut().map(contents).transpose()?;
        if bytes.as_ref() != Some(&after) {
            if bytes == before && record.stage != "applied" { record.stage = "notApplied".into(); self.store(&directory, &record)?; return Ok(record); }
            return Err("File changed after this operation; undo would overwrite later work".into());
        }
        if record.stage != "applied" {
            record.stage = "applied".into(); self.store(&directory, &record)?;
        }
        if let Some(before) = before {
            drop(current.take()); drop(parent);
            let restored = self.replace_authorized(&record.root, &record.path, Some(&after), &before,
                WritePolicy { identity: Some((record.root_volume, record.root_index)), recovery: true })?;
            if restored.stage != "applied" { return Err("Restore did not complete".into()); }
        } else {
            drop(current.take());
            let transaction = Transaction::begin()?;
            let mut file = transaction.open(&target, false)?.ok_or("Created file is missing")?;
            regular_stream(&file)?;
            if file.metadata().map_err(io)?.file_attributes() & 0x400 != 0 || contents(&mut file)? != after { return Err("Created file changed; removal refused".into()); }
            use windows_sys::Win32::Storage::FileSystem::{SetFileInformationByHandle, FileDispositionInfo, FILE_DISPOSITION_INFO};
            let information = FILE_DISPOSITION_INFO { DeleteFile: true };
            if unsafe { SetFileInformationByHandle(file.as_raw_handle().cast(), FileDispositionInfo, (&information as *const FILE_DISPOSITION_INFO).cast(), std::mem::size_of_val(&information) as u32) } == 0 { return Err(io(std::io::Error::last_os_error())); }
            drop(file); transaction.commit()?;
        }
        drop(current); record.stage = "undone".into(); self.store(&directory, &record)?;
        Ok(record)
    }
    pub fn cleanup(&mut self, ids: &[String]) -> Result<usize, String> {
        let mut removed = 0;
        for id in ids {
            let directory = self.record_dir(id)?;
            let guard = PinnedPath::existing_directory(&directory)?;
            match Self::latest(&directory) {
                Ok(record) if record.id == *id && ["applied", "undone", "notApplied", "failed", "conflict"].contains(&record.stage.as_str()) => {},
                Ok(_) => return Err("Pending recovery records cannot be deleted".into()),
                Err(error) if error == "Incomplete backup requires inspection" => {},
                Err(error) => return Err(error),
            }
            drop(guard); self.remove_directory(&directory)?; removed += 1;
        }
        Ok(removed)
    }
    fn remove_directory(&self, directory: &Path) -> Result<(), String> {
        let guard = PinnedPath::existing_directory(directory)?;
        let mut entries = fs::read_dir(directory).map_err(io)?.collect::<Result<Vec<_>, _>>().map_err(io)?;
        for entry in &entries {
            if !entry.file_type().map_err(io)?.is_file() || entry.metadata().map_err(io)?.file_attributes() & 0x400 != 0 {
                return Err("Unsafe recovery entry; cleanup refused".into());
            }
        }
        entries.sort_by_key(|entry| entry.file_name().to_string_lossy().starts_with("journal-"));
        for entry in entries { fs::remove_file(entry.path()).map_err(io)?; }
        drop(guard); fs::remove_dir(directory).map_err(io)
    }
    pub fn resolve_conflict(&mut self, id: &str) -> Result<Record, String> {
        let mut record = self.get(id)?;
        if !["prepared", "replacing"].contains(&record.stage.as_str()) { return Err("Record does not need conflict resolution".into()); }
        record.stage = "conflict".into(); record.warning = Some("Conflict acknowledged; all backups retained, no working-tree bytes changed".into());
        self.store(&self.record_dir(id)?, &record)?;
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_preparation_and_confirmed_incomplete_cleanup_allow_retry() {
        for phase in ["beforeBackup", "afterFirstBackup"] {
            let (base, root, backup) = fixture(); fs::write(root.join("file"), b"before").unwrap();
            let mut journal = Journal::open(&backup).unwrap(); journal.fault = Some(phase);
            assert!(journal.replace(&root, "file", Some(b"before"), b"after").is_err());
            assert!(journal.list().unwrap().is_empty());
            assert_eq!(fs::read(root.join("file")).unwrap(), b"before");
            journal.fault = None;
            fs::create_dir(backup.join("123-456")).unwrap(); fs::write(backup.join("123-456/before.bytes"), b"retained").unwrap();
            assert!(journal.replace(&root, "file", Some(b"before"), b"after").is_err());
            assert_eq!(journal.cleanup(&["123-456".into()]).unwrap(), 1);
            journal.replace(&root, "file", Some(b"before"), b"after").unwrap();
            drop(journal); fs::remove_dir_all(base).unwrap();
        }
    }

    #[test]
    fn acknowledged_conflict_retains_backups_and_allows_new_saves() {
        let (base, root, backup) = fixture(); fs::write(root.join("file"), b"before").unwrap();
        let mut journal = Journal::open(&backup).unwrap();
        let mut record = journal.replace(&root, "file", Some(b"before"), b"after").unwrap();
        record.stage = "replacing".into(); let directory = journal.record_dir(&record.id).unwrap(); journal.store(&directory, &record).unwrap();
        fs::write(root.join("file"), b"external").unwrap();
        assert!(journal.replace(&root, "file", Some(b"external"), b"new").is_err());
        assert_eq!(journal.resolve_conflict(&record.id).unwrap().stage, "conflict");
        assert_eq!(fs::read(directory.join("before.bytes")).unwrap(), b"before");
        assert_eq!(fs::read(directory.join("after.bytes")).unwrap(), b"after");
        assert_eq!(fs::read(root.join("file")).unwrap(), b"external");
        assert!(journal.undo(&record.id).is_err());
        journal.replace(&root, "file", Some(b"external"), b"new").unwrap();
        assert_eq!(journal.cleanup(&[record.id]).unwrap(), 1);
        drop(journal); fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn incomplete_backups_do_not_block_valid_undo_or_cleanup() {
        let (base, root, backup) = fixture(); fs::write(root.join("file"), b"before").unwrap();
        let mut journal = Journal::open(&backup).unwrap();
        let record = journal.replace(&root, "file", Some(b"before"), b"after").unwrap();
        fs::create_dir(backup.join("unfinished")).unwrap(); fs::write(backup.join("unfinished/before.bytes"), b"retained").unwrap();
        assert!(journal.list().unwrap().iter().any(|record| record.stage == "incomplete"));
        journal.undo(&record.id).unwrap(); assert_eq!(fs::read(root.join("file")).unwrap(), b"before");
        journal.cleanup(&[record.id]).unwrap(); assert_eq!(fs::read(backup.join("unfinished/before.bytes")).unwrap(), b"retained");
        drop(journal); fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn created_file_undo_retains_new_streams_and_changed_root_is_refused() {
        let (base, root, backup) = fixture(); let mut journal = Journal::open(&backup).unwrap();
        let record = journal.replace(&root, "created", None, b"new").unwrap();
        fs::write(root.join("created:later"), b"later work").unwrap();
        assert!(journal.undo(&record.id).unwrap_err().contains("alternate streams"));
        assert_eq!(fs::read(root.join("created:later")).unwrap(), b"later work");
        assert!(journal.replace_authorized(&root, "other", None, b"bad", WritePolicy { identity: Some((0, 0)), recovery: false }).is_err());
        assert!(!root.join("other").exists());
        drop(journal); fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn named_streams_are_retained_by_refusing_replacement() {
        let (base, root, backup) = fixture(); fs::write(root.join("file"), b"ordinary").unwrap();
        fs::write(root.join("file:secret"), b"stream").unwrap();
        let mut journal = Journal::open(&backup).unwrap();
        assert!(journal.replace(&root, "file", Some(b"ordinary"), b"new").unwrap_err().contains("alternate streams"));
        assert_eq!(fs::read(root.join("file")).unwrap(), b"ordinary");
        assert_eq!(fs::read(root.join("file:secret")).unwrap(), b"stream");
        drop(journal); fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn rollback_and_recovery_at_each_commit_boundary() {
        for phase in ["backedUp", "staged", "renamed", "committed"] {
            let (base, root, backup) = fixture(); fs::write(root.join("file"), b"before").unwrap();
            let mut journal = Journal::open(&backup).unwrap(); journal.fault = Some(phase);
            let outcome = journal.replace(&root, "file", Some(b"before"), b"after");
            if phase == "committed" { assert_eq!(outcome.unwrap().stage, "applied"); } else { assert!(outcome.is_err()); }
            assert_eq!(fs::read(root.join("file")).unwrap(), if phase == "committed" { &b"after"[..] } else { &b"before"[..] });
            let record = journal.list().unwrap().pop().unwrap();
            assert_eq!(fs::read(journal.record_dir(&record.id).unwrap().join("before.bytes")).unwrap(), b"before");
            assert!(!fs::read_dir(&root).unwrap().filter_map(Result::ok).any(|entry| entry.file_name().to_string_lossy().starts_with(".skein-")));
            journal.fault = None;
            if phase == "committed" { journal.undo(&record.id).unwrap(); assert_eq!(fs::read(root.join("file")).unwrap(), b"before"); }
            else { journal.replace(&root, "file", Some(b"before"), b"retry").unwrap(); }
            drop(journal); fs::remove_dir_all(base).unwrap();
        }
    }

    #[test]
    fn torn_latest_state_falls_back_and_full_store_never_evicts() {
        let (base, root, backup) = fixture(); fs::write(root.join("file"), b"before").unwrap();
        let mut journal = Journal::open(&backup).unwrap();
        let record = journal.replace(&root, "file", Some(b"before"), b"after").unwrap();
        let directory = journal.record_dir(&record.id).unwrap();
        fs::write(directory.join("journal-4.json"), b"{torn").unwrap();
        assert_eq!(journal.list().unwrap()[0].stage, "applied");
        let padding = File::create(directory.join("capacity-test")).unwrap(); padding.set_len(STORAGE_LIMIT).unwrap(); drop(padding);
        assert!(journal.replace(&root, "file", Some(b"after"), b"later").is_err());
        assert_eq!(fs::read(root.join("file")).unwrap(), b"after");
        assert_eq!(fs::read(directory.join("before.bytes")).unwrap(), b"before");
        drop(journal); fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn atomic_replacement_restart_and_conflict_safe_undo() {
        let base = std::env::temp_dir().join(format!("skein-recovery-{}", unique()));
        let root = base.join("repo"); let backup = base.join("recovery");
        fs::create_dir_all(root.join(".git")).unwrap(); fs::create_dir_all(&backup).unwrap();
        fs::write(root.join("file.txt"), b"original\r\n").unwrap();
        let mut journal = Journal::open(&backup).unwrap();
        assert!(journal.replace(&root, "file.txt", Some(b"wrong"), b"edited").is_err());
        let record = journal.replace(&root, "file.txt", Some(b"original\r\n"), b"edited\r\n").unwrap();
        assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"edited\r\n");
        drop(journal); let mut journal = Journal::open(&backup).unwrap();
        assert_eq!(journal.list().unwrap()[0].id, record.id);
        fs::write(root.join("file.txt"), b"external edit").unwrap();
        assert!(journal.undo(&record.id).is_err());
        assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"external edit");
        fs::write(root.join("file.txt"), b"edited\r\n").unwrap();
        assert_eq!(journal.undo(&record.id).unwrap().stage, "undone");
        assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"original\r\n");
        let created = journal.replace(&root, "new.txt", None, b"new").unwrap();
        journal.undo(&created.id).unwrap(); assert!(!root.join("new.txt").exists());
        assert!(journal.replace(&root, ".git/config", None, b"bad").is_err());
        drop(journal); fs::remove_dir_all(base).unwrap();
    }

    fn fixture() -> (PathBuf, PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("skein-recovery-{}", unique()));
        let root = base.join("repo"); let backup = base.join("recovery");
        fs::create_dir_all(root.join(".git")).unwrap(); fs::create_dir_all(&backup).unwrap();
        (base, root, backup)
    }

    #[test]
    fn locks_aliases_and_incomplete_backups_never_change_target() {
        let (base, root, backup) = fixture();
        fs::write(root.join("file.txt"), b"original").unwrap();
        let mut journal = Journal::open(&backup).unwrap();
        let locked = OpenOptions::new().read(true).write(true).share_mode(0).open(root.join("file.txt")).unwrap();
        assert!(journal.replace(&root, "file.txt", Some(b"original"), b"edited").is_err());
        drop(locked);
        assert!(journal.replace(&root, "FILE.txt", Some(b"original"), b"edited").is_err());
        fs::create_dir(root.join("nested")).unwrap(); fs::create_dir(root.join("nested/.git")).unwrap();
        assert!(journal.replace(&root, "nested/file", None, b"edited").is_err());
        fs::create_dir(backup.join("unfinished")).unwrap();
        assert!(journal.replace(&root, "file.txt", Some(b"original"), b"edited").is_err());
        assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"original");
        drop(journal); fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn restart_reconciles_prepared_and_replacing_states() {
        let (base, root, backup) = fixture();
        fs::write(root.join("file.txt"), b"original").unwrap();
        let mut journal = Journal::open(&backup).unwrap();
        let mut record = journal.replace(&root, "file.txt", Some(b"original"), b"edited").unwrap();
        let directory = journal.record_dir(&record.id).unwrap();
        fs::write(root.join(format!(".skein-{}.previous", record.id)), b"original").unwrap();
        record.stage = "replacing".into(); journal.store(&directory, &record).unwrap();
        drop(journal); let mut journal = Journal::open(&backup).unwrap();
        assert_eq!(journal.undo(&record.id).unwrap().stage, "undone");
        assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"original");
        let mut prepared = journal.replace(&root, "file.txt", Some(b"original"), b"not committed").unwrap();
        fs::write(root.join("file.txt"), b"original").unwrap();
        prepared.stage = "prepared".into(); journal.store(&journal.record_dir(&prepared.id).unwrap(), &prepared).unwrap();
        drop(journal); let mut journal = Journal::open(&backup).unwrap();
        assert_eq!(journal.undo(&prepared.id).unwrap().stage, "notApplied");
        assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"original");
        drop(journal); fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn corrupt_backups_and_modified_creates_refuse_undo() {
        let (base, root, backup) = fixture(); let mut journal = Journal::open(&backup).unwrap();
        let created = journal.replace(&root, "created.txt", None, b"first").unwrap();
        fs::write(root.join("created.txt"), b"later work").unwrap();
        assert!(journal.undo(&created.id).is_err());
        assert_eq!(fs::read(root.join("created.txt")).unwrap(), b"later work");
        fs::write(root.join("existing.txt"), b"before").unwrap();
        let changed = journal.replace(&root, "existing.txt", Some(b"before"), b"after").unwrap();
        fs::write(journal.record_dir(&changed.id).unwrap().join("before.bytes"), b"corrupt").unwrap();
        assert!(journal.undo(&changed.id).is_err());
        assert_eq!(fs::read(root.join("existing.txt")).unwrap(), b"after");
        drop(journal); fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn junction_escape_and_hardlink_mutation_are_blocked() {
        let (base, root, backup) = fixture(); let outside = base.join("outside"); fs::create_dir(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"outside").unwrap();
        let output = std::process::Command::new("cmd").args(["/C", "mklink", "/J"])
            .arg(root.join("junction")).arg(&outside).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let mut journal = Journal::open(&backup).unwrap();
        assert!(journal.replace(&root, "junction/sentinel", Some(b"outside"), b"bad").is_err());
        assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"outside");
        fs::hard_link(outside.join("sentinel"), root.join("hardlink")).unwrap();
        journal.replace(&root, "hardlink", Some(b"outside"), b"inside").unwrap();
        assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"outside");
        assert_eq!(fs::read(root.join("hardlink")).unwrap(), b"inside");
        drop(journal); fs::remove_dir(root.join("junction")).unwrap(); fs::remove_dir_all(base).unwrap();
    }
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