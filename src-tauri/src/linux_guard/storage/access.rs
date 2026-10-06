use super::{Lock, PrivateDir, PrivateFile};
use crate::linux_guard::Error;
use rustix::fs::FlockOperation;

impl PrivateDir {
    pub(crate) fn lock_existing(&self) -> Result<Lock, Error> {
        let file = self.file("lock", false)?;
        rustix::fs::flock(&file.handle, FlockOperation::NonBlockingLockExclusive).map_err(
            |_| Error::conflict("Another Skein process owns the Linux recovery lock"),
        )?;
        file.revalidate()?;
        Ok(Lock(file))
    }
    pub(crate) fn security(&self) -> Result<(u32, u32, u32), Error> {
        self.revalidate()?;
        let stat = rustix::fs::fstat(&self.0.handle).map_err(Error::io)?;
        Ok((stat.st_uid, stat.st_gid, stat.st_mode & 0o777))
    }
}
impl PrivateFile {
    pub(crate) fn sync(&self) -> Result<(), Error> {
        self.revalidate()?;
        rustix::fs::fsync(&self.handle).map_err(Error::io)
    }
}
