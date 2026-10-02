#![cfg(windows)]

use std::fs::{File, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::path::{Component, Path, PathBuf};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES,
    FILE_SHARE_READ, FILE_SHARE_WRITE,
};

pub struct PinnedPath {
    pub path: PathBuf,
    handles: Vec<File>,
}

pub struct Transaction { handle: OwnedHandle }

impl Transaction {
    pub fn begin() -> Result<Self, String> {
        let handle = unsafe { windows_sys::Win32::Storage::FileSystem::CreateTransaction(std::ptr::null_mut(), std::ptr::null_mut(), 0, 0, 0, 30_000, std::ptr::null()) };
        if handle == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE { return Err(format!("Transactional writes unavailable: {}", std::io::Error::last_os_error())); }
        Ok(Self { handle: unsafe { OwnedHandle::from_raw_handle(handle.cast()) } })
    }
    pub fn open(&self, path: &Path, create: bool) -> Result<Option<File>, String> {
        use windows_sys::Win32::Storage::FileSystem::{CreateFileTransactedW, CREATE_NEW, OPEN_EXISTING, FILE_SHARE_DELETE};
        let handle = unsafe { CreateFileTransactedW(wide(path).as_ptr(), if create { 0xc00d_0000 } else { 0xc001_0000 }, FILE_SHARE_READ | FILE_SHARE_DELETE,
            std::ptr::null(), if create { CREATE_NEW } else { OPEN_EXISTING }, FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(), self.handle.as_raw_handle().cast(), std::ptr::null(), std::ptr::null()) };
        if handle == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            let error = std::io::Error::last_os_error();
            if !create && error.kind() == std::io::ErrorKind::NotFound { return Ok(None); }
            return Err(format!("Transactional file access refused: {error}"));
        }
        Ok(Some(unsafe { File::from_raw_handle(handle.cast()) }))
    }
    pub fn replace(&self, source: &Path, target: &Path, exists: bool) -> Result<(), String> {
        use windows_sys::Win32::Storage::FileSystem::{MoveFileTransactedW, MOVEFILE_REPLACE_EXISTING};
        let success = unsafe { MoveFileTransactedW(wide(source).as_ptr(), wide(target).as_ptr(), None, std::ptr::null(),
            if exists { MOVEFILE_REPLACE_EXISTING } else { 0 }, self.handle.as_raw_handle().cast()) };
        if success == 0 { return Err(format!("Transactional replacement refused: {}", std::io::Error::last_os_error())); }
        Ok(())
    }
    pub fn commit(&self) -> Result<(), String> {
        if unsafe { windows_sys::Win32::Storage::FileSystem::CommitTransaction(self.handle.as_raw_handle().cast()) } == 0 {
            return Err(format!("Transaction commit refused: {}", std::io::Error::last_os_error()));
        }
        Ok(())
    }
}

impl Drop for Transaction {
    fn drop(&mut self) { unsafe { windows_sys::Win32::Storage::FileSystem::RollbackTransaction(self.handle.as_raw_handle().cast()); } }
}

fn wide(path: &Path) -> Vec<u16> { path.as_os_str().encode_wide().chain(Some(0)).collect() }

#[cfg(test)]
pub fn atomic_replace(source: &Path, destination: &Path, backup: Option<&Path>) -> Result<(), String> {
    use windows_sys::Win32::Storage::FileSystem::{ReplaceFileW, MoveFileExW, MOVEFILE_WRITE_THROUGH};
    let target_name = destination.display().to_string();
    let source = wide(source);
    let destination = wide(destination);
    let success = if let Some(backup) = backup {
        let backup = wide(backup);
        unsafe { ReplaceFileW(destination.as_ptr(), source.as_ptr(), backup.as_ptr(), 0, std::ptr::null(), std::ptr::null()) }
    } else {
        unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), MOVEFILE_WRITE_THROUGH) }
    };
    if success == 0 { return Err(format!("Atomic replacement of {target_name} refused: {}", std::io::Error::last_os_error())); }
    Ok(())
}

fn pin_directory(path: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|error| format!("Cannot protect directory: {error}"))?;
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    let success = unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut information) };
    if success == 0 {
        return Err(format!("Cannot inspect protected directory: {}", std::io::Error::last_os_error()));
    }
    if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || information.dwFileAttributes & 0x10 == 0
    {
        return Err("Linked or non-directory ancestor is unsupported".into());
    }
    Ok(file)
}

pub fn pin_metadata(path: &Path) -> Result<File, String> { pin_directory(path) }

impl PinnedPath {
    pub fn permit_entry_update(&mut self) -> Result<(), String> {
        let replacement = OpenOptions::new().access_mode(FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE).custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&self.path).map_err(|error| error.to_string())?;
        let last = self.handles.last_mut().ok_or("Missing parent")?;
        *last = replacement;
        Ok(())
    }
    #[cfg(test)]
    pub fn rename_file(&self, file: &File, leaf: &str, replace: bool) -> Result<(), String> {
        crate::paths::relative(leaf)?;
        if leaf.contains('/') { return Err("Rename requires one leaf name".into()); }
        use windows_sys::Wdk::Storage::FileSystem::{NtSetInformationFile, FileRenameInformationEx, FILE_RENAME_INFORMATION, FILE_RENAME_POSIX_SEMANTICS, FILE_RENAME_REPLACE_IF_EXISTS};
        use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;
        let name: Vec<u16> = leaf.encode_utf16().collect();
        let offset = std::mem::offset_of!(FILE_RENAME_INFORMATION, FileName);
        let length = offset + name.len() * 2;
        let mut buffer = vec![FILE_RENAME_INFORMATION::default(); length.div_ceil(std::mem::size_of::<FILE_RENAME_INFORMATION>())];
        let information = buffer.as_mut_ptr();
        let mut status = IO_STATUS_BLOCK::default();
        let result = unsafe {
            (*information).Anonymous.Flags = if replace { FILE_RENAME_REPLACE_IF_EXISTS | FILE_RENAME_POSIX_SEMANTICS } else { 0 };
            (*information).RootDirectory = self.handles.last().ok_or("Missing pinned parent")?.as_raw_handle().cast();
            (*information).FileNameLength = (name.len() * 2) as u32;
            std::ptr::copy_nonoverlapping(name.as_ptr(), information.cast::<u8>().add(offset).cast(), name.len());
            NtSetInformationFile(file.as_raw_handle().cast(), &mut status, information.cast(), length as u32, FileRenameInformationEx)
        };
        if result < 0 { return Err(format!("Handle-relative rename refused: NTSTATUS {result:#x}")); }
        file.sync_all().map_err(|error| error.to_string())
    }

    pub fn existing_directory(path: &Path) -> Result<Self, String> {
        Self::directory(path, false)
    }

    pub fn ensure_directory(path: &Path) -> Result<Self, String> {
        Self::directory(path, true)
    }

    fn directory(path: &Path, create: bool) -> Result<Self, String> {
        let text = path.to_str().ok_or("Unsupported directory encoding")?;
        let path = Path::new(text.strip_prefix("\\\\?\\").unwrap_or(text));
        crate::git::valid_path(path.to_str().ok_or("Unsupported directory encoding")?, !create)?;
        let mut current = PathBuf::new();
        let mut handles = Vec::new();
        for component in path.components() {
            match component {
                Component::Prefix(_) | Component::RootDir => current.push(component),
                Component::Normal(part) => {
                    current.push(part);
                    if create && !current.exists() {
                        match std::fs::create_dir(&current) {
                            Ok(()) => {},
                            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {},
                            Err(error) => return Err(format!("Cannot create protected directory: {error}")),
                        }
                    }
                    handles.push(pin_directory(&current)?);
                }
                _ => return Err("Unsafe directory components".into()),
            }
        }
        if handles.is_empty() {
            return Err("A volume root cannot be a repository".into());
        }
        Ok(Self { path: current, handles })
    }

    pub fn relative_parent(root: &Path, relative: &str) -> Result<Self, String> {
        Self::parent(root, relative, false)
    }

    pub fn create_parent(root: &Path, relative: &str) -> Result<Self, String> {
        Self::parent(root, relative, true)
    }

    fn parent(root: &Path, relative: &str, create: bool) -> Result<Self, String> {
        crate::paths::relative(relative)?;
        let mut guard = Self::existing_directory(root)?;
        let parts: Vec<_> = relative.split('/').collect();
        for part in &parts[..parts.len() - 1] {
            guard.path.push(part);
            if create && !guard.path.exists() {
                match std::fs::create_dir(&guard.path) {
                    Ok(()) => {},
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {},
                    Err(error) => return Err(error.to_string()),
                }
            }
            guard.handles.push(pin_directory(&guard.path)?);
            if guard.path.join(".git").exists() {
                return Err("Nested repositories are protected".into());
            }
            let names = std::fs::read_dir(guard.path.parent().ok_or("Missing parent")?).map_err(|error| error.to_string())?;
            if !names.filter_map(Result::ok).any(|entry| entry.file_name() == std::ffi::OsStr::new(part)) {
                return Err("Case or short-name aliases are unsupported".into());
            }
        }
        Ok(guard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn junction_probe_positive_control_converts_unprotected_empty_directory() {
        let base = std::env::temp_dir().join(format!("paperwing-positive-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let directory = base.join("directory"); let outside = base.join("outside");
        std::fs::create_dir_all(&directory).unwrap(); std::fs::create_dir(&outside).unwrap(); std::fs::write(outside.join("sentinel"), b"outside").unwrap();
        let changed = attempt_junction(&directory, &outside); let reaches_outside = directory.join("sentinel").exists();
        std::fs::remove_dir(&directory).unwrap(); std::fs::remove_dir_all(base).unwrap();
        assert!(changed && reaches_outside, "Junction probe must demonstrate a real successful control attack");
    }

    #[test]
    fn native_atomic_replace_refuses_share_locked_destination() {
        use std::io::Write;
        let base = std::env::temp_dir().join(format!("paperwing-native-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&base).unwrap(); std::fs::write(base.join("target"), b"old").unwrap();
        let mut parent = PinnedPath::existing_directory(&base).unwrap();
        let target = OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).open(base.join("target")).unwrap();
        let mut source = OpenOptions::new().read(true).write(true).access_mode(0xc001_0000).share_mode(FILE_SHARE_READ).create_new(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT).open(base.join("source")).unwrap();
        source.write_all(b"new").unwrap(); source.sync_all().unwrap();
        parent.permit_entry_update().unwrap();
        assert!(std::fs::rename(base.join("target"), base.join("foreign")).is_err());
        assert!(parent.rename_file(&source, "target", true).is_err());
        assert_eq!(std::fs::read(base.join("target")).unwrap(), b"old");
        let mut original = target; let mut bytes = String::new();
        std::io::Read::read_to_string(&mut original, &mut bytes).unwrap(); assert_eq!(bytes, "old");
        drop(original); drop(source); drop(parent); std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn transaction_replaces_atomically_without_unlocked_destination_gap() {
        use std::io::Write;
        let base = std::env::temp_dir().join(format!("paperwing-transaction-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&base).unwrap(); std::fs::write(base.join("target"), b"old").unwrap();
        let mut parent = PinnedPath::existing_directory(&base).unwrap();
        let keeper = OpenOptions::new().read(true).write(true).create_new(true).access_mode(0xc001_0000).share_mode(FILE_SHARE_READ).open(base.join("keeper")).unwrap();
        parent.permit_entry_update().unwrap();
        let transaction = Transaction::begin().unwrap();
        let old = transaction.open(&base.join("target"), false).unwrap().unwrap();
        assert!(std::fs::write(base.join("target"), b"foreign").is_err());
        assert!(std::fs::rename(base.join("target"), base.join("foreign")).is_err());
        drop(old);
        assert!(std::fs::write(base.join("target"), b"foreign").is_err());
        assert!(std::fs::rename(base.join("target"), base.join("foreign")).is_err());
        let mut staged = transaction.open(&base.join("staged"), true).unwrap().unwrap();
        staged.write_all(b"new").unwrap(); staged.sync_all().unwrap(); drop(staged);
        transaction.replace(&base.join("staged"), &base.join("target"), true).unwrap();
        assert_eq!(std::fs::read(base.join("target")).unwrap(), b"old");
        transaction.commit().unwrap(); drop(transaction);
        assert_eq!(std::fs::read(base.join("target")).unwrap(), b"new");
        drop(keeper); drop(parent); std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn pinned_ancestors_block_substitution_and_reject_aliases() {
        let base = std::env::temp_dir().join(format!("paperwing-pins-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let root = base.join("repo");
        std::fs::create_dir_all(root.join("nested")).unwrap();
        let guard = PinnedPath::relative_parent(&root, "nested/file.txt").unwrap();
        assert!(std::fs::rename(&root, base.join("moved")).is_err());
        assert!(std::fs::rename(root.join("nested"), root.join("replaced")).is_err());
        for relative in ["../outside", ".git/config", "file:stream", "NUL.txt"] {
            assert!(PinnedPath::relative_parent(&root, relative).is_err());
        }
        drop(guard);
        std::fs::rename(&root, base.join("moved")).unwrap();
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn atomic_move_never_overwrites_an_existing_destination() {
        let base = std::env::temp_dir().join(format!("paperwing-move-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&base).unwrap();
        std::fs::write(base.join("source"), b"incoming").unwrap();
        std::fs::write(base.join("target"), b"keep").unwrap();
        assert!(atomic_replace(&base.join("source"), &base.join("target"), None).is_err());
        assert_eq!(std::fs::read(base.join("target")).unwrap(), b"keep");
        assert_eq!(std::fs::read(base.join("source")).unwrap(), b"incoming");
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn directory_pin_blocks_in_place_junction_substitution() {
        let base = std::env::temp_dir().join(format!("paperwing-reparse-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let root = base.join("repo"); let outside = base.join("outside");
        std::fs::create_dir_all(root.join("nested")).unwrap(); std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("sentinel"), b"outside").unwrap();
        let guard = PinnedPath::relative_parent(&root, "nested/sentinel").unwrap();
        let changed = attempt_junction(&root.join("nested"), &outside);
        drop(guard);
        let escaped = root.join("nested/sentinel").exists();
        std::fs::remove_dir(root.join("nested")).unwrap(); std::fs::remove_dir_all(base).unwrap();
        assert!(!changed && !escaped, "Directory handle allowed in-place reparse substitution");
    }

    fn attempt_junction(directory: &Path, target: &Path) -> bool {
        use windows_sys::Win32::System::IO::DeviceIoControl;
        let Ok(handle) = OpenOptions::new().access_mode(0x4000_0000).share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS).open(directory) else { return false; };
        let substitute: Vec<u16> = format!("\\??\\{}", target.display()).encode_utf16().collect();
        let print: Vec<u16> = target.as_os_str().encode_wide().collect();
        let mut buffer = Vec::new(); buffer.extend_from_slice(&0xa0000003u32.to_le_bytes());
        buffer.extend_from_slice(&(8u16 + ((substitute.len() + print.len() + 2) * 2) as u16).to_le_bytes());
        buffer.extend_from_slice(&0u16.to_le_bytes());
        for value in [0, (substitute.len() * 2) as u16, ((substitute.len() + 1) * 2) as u16, (print.len() * 2) as u16] { buffer.extend_from_slice(&value.to_le_bytes()); }
        for character in substitute.into_iter().chain(Some(0)).chain(print).chain(Some(0)) { buffer.extend_from_slice(&character.to_le_bytes()); }
        let mut returned = 0;
        unsafe { DeviceIoControl(handle.as_raw_handle().cast(), 0x000900a4, buffer.as_ptr().cast(), buffer.len() as u32, std::ptr::null_mut(), 0, &mut returned, std::ptr::null_mut()) != 0 }
    }

    #[test]
    fn held_staged_file_blocks_in_place_junction_with_parent_write_sharing() {
        let base = std::env::temp_dir().join(format!("paperwing-nonempty-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let directory = base.join("parent"); let outside = base.join("outside");
        std::fs::create_dir_all(&directory).unwrap(); std::fs::create_dir(&outside).unwrap();
        let mut parent = PinnedPath::existing_directory(&directory).unwrap();
        let keeper = OpenOptions::new().write(true).create_new(true).share_mode(FILE_SHARE_READ).open(directory.join("keeper")).unwrap();
        parent.permit_entry_update().unwrap();
        let changed = attempt_junction(&directory, &outside);
        assert!(std::fs::rename(&directory, base.join("moved")).is_err());
        assert!(std::fs::remove_file(directory.join("keeper")).is_err());
        drop(keeper); drop(parent);
        if changed { std::fs::remove_dir(&directory).unwrap(); }
        std::fs::remove_dir_all(base).unwrap();
        assert!(!changed, "A held file did not prevent in-place junction conversion");
    }
}
