pub(crate) mod folders;
pub(crate) mod metadata;
pub(crate) mod mutation;
pub(crate) mod root;
pub(crate) mod storage;

#[cfg(test)]
use mutation::Snapshot;
pub(crate) use root::Root;
#[cfg(test)]
use storage::PrivateDir;

use rustix::fd::{AsFd, BorrowedFd, OwnedFd};
use rustix::fs::{AtFlags, OFlags, ResolveFlags, StatxFlags};
use rustix::io::Errno;
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) const FILE_LIMIT: usize = 64 * 1024 * 1024;
const HANDLE_LIMIT: usize = 512;
const DEPTH_LIMIT: usize = 64;
const METADATA_LIMIT: usize = 8;
const CONFINED: ResolveFlags = ResolveFlags::BENEATH
    .union(ResolveFlags::NO_SYMLINKS)
    .union(ResolveFlags::NO_XDEV);
static HANDLES: AtomicUsize = AtomicUsize::new(0);
static STAGED_BYTES: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ErrorKind {
    Unsupported,
    Conflict,
    Limit,
    Accounting,
    Io,
    Missing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Error {
    pub kind: ErrorKind,
    pub message: &'static str,
    pub code: Option<i32>,
}

impl Error {
    pub(crate) fn new(kind: ErrorKind, message: &'static str) -> Self {
        Self {
            kind,
            message,
            code: None,
        }
    }
    pub(crate) fn io(error: impl Into<std::io::Error>) -> Self {
        let error = error.into();
        let message = match error.raw_os_error() {
            Some(libc::ENOSPC) => "Linux storage is full",
            Some(libc::EROFS) => "Linux storage is read-only",
            Some(libc::EXDEV) => "Linux operation crossed a filesystem boundary",
            Some(libc::EACCES | libc::EPERM) => "Linux filesystem access was denied",
            _ => "Linux filesystem operation failed",
        };
        Self {
            kind: ErrorKind::Io,
            message,
            code: error.raw_os_error(),
        }
    }
    pub(crate) fn conflict(message: &'static str) -> Self {
        Self::new(ErrorKind::Conflict, message)
    }
    pub(crate) fn unsupported(message: &'static str) -> Self {
        Self::new(ErrorKind::Unsupported, message)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message)
    }
}
impl std::error::Error for Error {}

#[derive(Debug)]
struct Permit;
impl Permit {
    fn acquire() -> Result<Self, Error> {
        HANDLES
            .try_update(Ordering::AcqRel, Ordering::Acquire, |held| {
                (held < HANDLE_LIMIT).then_some(held + 1)
            })
            .map(|_| Self)
            .map_err(|_| Error::new(ErrorKind::Limit, "Linux filesystem handle limit reached"))
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        HANDLES.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Debug)]
pub(crate) struct BytePermit(usize);
impl BytePermit {
    pub(crate) fn acquire(bytes: usize) -> Result<Self, Error> {
        STAGED_BYTES
            .try_update(Ordering::AcqRel, Ordering::Acquire, |held| {
                held.checked_add(bytes)
                    .filter(|total| *total <= 512 * 1024 * 1024)
            })
            .map(|_| Self(bytes))
            .map_err(|_| {
                Error::new(
                    ErrorKind::Limit,
                    "Linux staged content exceeds the memory limit",
                )
            })
    }
}
impl Drop for BytePermit {
    fn drop(&mut self) {
        STAGED_BYTES.fetch_sub(self.0, Ordering::AcqRel);
    }
}

#[derive(Debug)]
struct Handle {
    file: File,
    _permit: Permit,
}
impl AsFd for Handle {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.file.as_fd()
    }
}
impl Handle {
    fn open(
        directory: impl AsFd,
        path: &std::path::Path,
        flags: OFlags,
        mode: rustix::fs::Mode,
        resolve: ResolveFlags,
    ) -> Result<Self, Error> {
        let permit = Permit::acquire()?;
        let fd = rustix::fs::openat2(
            directory,
            path,
            flags
                | OFlags::CLOEXEC
                | OFlags::NOFOLLOW
                | if flags.contains(OFlags::PATH) {
                    OFlags::empty()
                } else {
                    OFlags::NONBLOCK
                },
            mode,
            resolve,
        )
        .map_err(|error| match error {
            Errno::NOSYS | Errno::INVAL => {
                Error::unsupported("Linux confined resolution is unavailable")
            }
            Errno::NOENT => Error::new(ErrorKind::Missing, "Linux path is missing"),
            Errno::LOOP | Errno::XDEV => {
                Error::unsupported("Linked paths or mount crossings are unsupported")
            }
            _ => Error::io(error),
        })?;
        Ok(Self::owned(fd, permit))
    }
    fn owned(fd: OwnedFd, permit: Permit) -> Self {
        Self {
            file: File::from(fd),
            _permit: permit,
        }
    }
    fn absolute(path: &std::path::Path, flags: OFlags) -> Result<Self, Error> {
        if !path.is_absolute() {
            return Err(Error::unsupported("An absolute Linux path is required"));
        }
        Self::open(
            rustix::fs::CWD,
            path,
            flags,
            rustix::fs::Mode::empty(),
            ResolveFlags::NO_SYMLINKS,
        )
    }
    fn identity(&self) -> Result<Identity, Error> {
        let stat = rustix::fs::statx(
            self,
            "",
            AtFlags::EMPTY_PATH,
            StatxFlags::BASIC_STATS | StatxFlags::MNT_ID,
        )
        .map_err(Error::io)?;
        let required = StatxFlags::INO | StatxFlags::TYPE | StatxFlags::MNT_ID;
        if stat.stx_mask & required.bits() != required.bits() {
            return Err(Error::unsupported("Linux mount identity is unavailable"));
        }
        Ok(Identity {
            device: (stat.stx_dev_major, stat.stx_dev_minor),
            inode: stat.stx_ino,
            mount: stat.stx_mnt_id,
        })
    }
    fn read(&self, limit: usize) -> Result<Vec<u8>, Error> {
        if limit > FILE_LIMIT {
            return Err(Error::new(
                ErrorKind::Limit,
                "File content limit is unsupported",
            ));
        }
        let stat = rustix::fs::fstat(self).map_err(Error::io)?;
        if rustix::fs::FileType::from_raw_mode(stat.st_mode) != rustix::fs::FileType::RegularFile {
            return Err(Error::unsupported(
                "Only regular files support content reads",
            ));
        }
        if stat.st_size < 0 || stat.st_size as u64 > limit as u64 {
            return Err(Error::new(
                ErrorKind::Limit,
                "File content exceeds the limit",
            ));
        }
        let mut bytes = Vec::new();
        let mut block = [0; 8192];
        loop {
            let room = (limit + 1).saturating_sub(bytes.len());
            if room == 0 {
                return Err(Error::new(
                    ErrorKind::Limit,
                    "File content exceeds the limit",
                ));
            }
            let count = self
                .file
                .read_at(&mut block[..room.min(8192)], bytes.len() as u64)
                .map_err(Error::io)?;
            if count == 0 {
                return Ok(bytes);
            }
            bytes.extend_from_slice(&block[..count]);
            if bytes.len() > limit {
                return Err(Error::new(
                    ErrorKind::Limit,
                    "File content exceeds the limit",
                ));
            }
        }
    }
    fn write(&self, bytes: &[u8]) -> Result<(), Error> {
        if bytes.len() > FILE_LIMIT {
            return Err(Error::new(
                ErrorKind::Limit,
                "File content exceeds the limit",
            ));
        }
        self.file.write_all_at(bytes, 0).map_err(Error::io)?;
        self.file.set_len(bytes.len() as u64).map_err(Error::io)?;
        rustix::fs::fsync(self).map_err(Error::io)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Identity {
    device: (u32, u32),
    inode: u64,
    mount: u64,
}

fn relative(path: &str) -> Result<Vec<&str>, Error> {
    crate::paths::relative(path)
        .map_err(|_| Error::unsupported("Unsafe or protected relative path"))?;
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() > DEPTH_LIMIT {
        return Err(Error::new(
            ErrorKind::Limit,
            "Linux path depth exceeds the limit",
        ));
    }
    Ok(parts)
}

#[cfg(test)]
mod tests;

fn entries(handle: &Handle, limit: usize) -> Result<Vec<String>, Error> {
    let _permit = Permit::acquire()?;
    let mut directory = rustix::fs::Dir::read_from(handle).map_err(Error::io)?;
    let mut names = Vec::new();
    for entry in &mut directory {
        let entry = entry.map_err(Error::io)?;
        let name = entry
            .file_name()
            .to_str()
            .map_err(|_| Error::unsupported("Unsupported directory entry encoding"))?;
        if name == "." || name == ".." {
            continue;
        }
        if names.len() >= limit {
            return Err(Error::new(
                ErrorKind::Limit,
                "Directory entry limit reached",
            ));
        }
        names.push(name.to_string());
    }
    names.sort();
    Ok(names)
}
