#[cfg(test)]
use super::diff_storage_hook;
use super::{Error, ErrorKind, Handle, Lock, PrivateDir};
use rustix::fs::FlockOperation;
use std::path::Path;

pub(crate) const DIFF_NAMESPACE_BYTES: u64 = 1024 * 1024;
pub(crate) const DIFF_ENTRY_GROWTH_BYTES: u64 = 64 * 1024;

#[derive(Clone, Copy)]
pub(crate) enum Namespace {
    Diff,
    Recovery,
}
impl Namespace {
    fn name(self) -> &'static str {
        match self {
            Self::Diff => "linux-diff-v1",
            Self::Recovery => "linux-recovery-v1",
        }
    }
}

pub(super) fn qualify(handle: &Handle) -> Result<(), Error> {
    let stat = rustix::fs::fstatfs(handle).map_err(Error::io)?;
    if stat.f_type != libc::EXT4_SUPER_MAGIC || !matches!(stat.f_bsize, 1024 | 2048 | 4096) {
        return Err(Error::unsupported(
            "Private diff storage requires local ext4 with supported block size",
        ));
    }
    Ok(())
}

impl PrivateDir {
    pub(crate) fn diff_filesystem(&self) -> Result<(), Error> {
        self.revalidate()?;
        qualify(&self.0.handle)
    }

    pub(crate) fn before_diff_entry(&self) -> Result<u64, Error> {
        self.diff_filesystem()?;
        let before = self.native_size()?;
        if before > DIFF_NAMESPACE_BYTES {
            return Err(Error::new(
                ErrorKind::Accounting,
                "Private diff namespace exceeds its allowance",
            ));
        }
        if before
            .checked_add(DIFF_ENTRY_GROWTH_BYTES)
            .is_none_or(|size| size > DIFF_NAMESPACE_BYTES)
        {
            return Err(Error::new(
                ErrorKind::Limit,
                "Private diff namespace capacity reached",
            ));
        }
        Ok(before)
    }

    pub(crate) fn after_diff_entry(&self, before: u64) -> Result<(), Error> {
        self.diff_filesystem()?;
        #[cfg(test)]
        diff_storage_hook("before-entry-postcheck")?;
        let after = self.native_size()?;
        if after > DIFF_NAMESPACE_BYTES || after.saturating_sub(before) > DIFF_ENTRY_GROWTH_BYTES {
            return Err(Error::new(
                ErrorKind::Accounting,
                "Private diff namespace growth exceeds its allowance",
            ));
        }
        Ok(())
    }
}

pub(crate) fn diff_path(path: &Path) -> Result<(), Error> {
    let text = path
        .to_str()
        .ok_or_else(|| Error::unsupported("Unsupported private diff path encoding"))?;
    if !path.is_absolute() || text.len() > 4096 || text == "/" {
        return Err(Error::unsupported("Unsupported private diff storage path"));
    }
    let parts: Vec<_> = text[1..].split('/').collect();
    if parts.len() > 64
        || parts
            .iter()
            .any(|part| part.is_empty() || *part == "." || *part == ".." || part.contains('\0'))
    {
        return Err(Error::unsupported("Unsafe private diff storage components"));
    }
    Ok(())
}
mod initialize;
#[cfg(test)]
pub(crate) use initialize::INITIALIZE_HOOK;

#[derive(Debug)]
pub(crate) enum LockAttempt {
    Acquired(Lock),
    Busy,
}
impl PrivateDir {
    pub(crate) fn try_diff_lock(&self) -> Result<LockAttempt, Error> {
        let file = match self.file("lock", false) {
            Ok(file) => file,
            Err(error) if error.kind == ErrorKind::Missing => {
                #[cfg(test)]
                diff_storage_hook("missing-lock")?;
                let before = self.before_diff_entry()?;
                let created = self.file("lock", true);
                self.after_diff_entry(before)?;
                match created {
                    Ok(file) => file,
                    Err(error) if error.code == Some(libc::EEXIST) => self.file("lock", false)?,
                    Err(error) => return Err(error),
                }
            }
            Err(error) => return Err(error),
        };
        self.revalidate()?;
        file.revalidate()?;
        match rustix::fs::flock(&file.handle, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => {
                file.revalidate()?;
                Ok(LockAttempt::Acquired(Lock(file)))
            }
            Err(error)
                if error == rustix::io::Errno::AGAIN || error == rustix::io::Errno::WOULDBLOCK =>
            {
                Ok(LockAttempt::Busy)
            }
            Err(error) => Err(Error::io(error)),
        }
    }
    pub(crate) fn native_size(&self) -> Result<u64, Error> {
        self.revalidate()?;
        let size = rustix::fs::fstat(&self.0.handle)
            .map_err(Error::io)?
            .st_size;
        if size < 0 {
            return Err(Error::unsupported("Invalid private directory size"));
        }
        Ok(size as u64)
    }
}
