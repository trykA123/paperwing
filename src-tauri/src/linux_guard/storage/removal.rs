use super::{FileProof, Lock, PrivateDir};
use crate::linux_guard::{mutation::MutationError, Error, Identity};
use rustix::fs::AtFlags;

pub(crate) struct ArtifactRemoval<'a, F, G> {
    pub proof: &'a FileProof,
    pub lock: &'a Lock,
    pub limit: usize,
    pub binding: F,
    pub after_unlink: G,
}
pub(crate) struct DirectoryRemoval<'a, F, G> {
    pub identity: &'a Identity,
    pub lock: &'a Lock,
    pub binding: F,
    pub after_unlink: G,
}
impl PrivateDir {
    pub(crate) fn unlink_authorized<F, G>(
        &self,
        removal: ArtifactRemoval<'_, F, G>,
    ) -> Result<(), MutationError>
    where
        F: FnOnce() -> Result<(), Error>,
        G: FnOnce() -> Result<(), Error>,
    {
        let ArtifactRemoval {
            proof,
            lock,
            limit,
            binding,
            after_unlink,
        } = removal;
        let file = self.file(&proof.name, false)?;
        if file.proof(limit)? != *proof {
            return Err(Error::conflict("Private cleanup artifact changed").into());
        }
        #[cfg(test)]
        super::diff_storage_hook("unlink-file")?;
        binding()?;
        file.revalidate()?;
        lock.revalidate()?;
        rustix::fs::unlinkat(&self.0.handle, proof.name.as_str(), AtFlags::empty())
            .map_err(|error| MutationError::from(Error::io(error)))?;
        let finish = || -> Result<(), Error> {
            after_unlink()?;
            #[cfg(test)]
            super::diff_storage_hook("sync-file-removal")?;
            self.sync()
        };
        finish().map_err(|error| MutationError {
            error,
            applied: true,
        })
    }
    pub(crate) fn remove_empty_authorized<F, G>(
        &self,
        name: &str,
        removal: DirectoryRemoval<'_, F, G>,
    ) -> Result<(), MutationError>
    where
        F: FnOnce() -> Result<(), Error>,
        G: FnOnce() -> Result<(), Error>,
    {
        let DirectoryRemoval {
            identity,
            lock,
            binding,
            after_unlink,
        } = removal;
        let child = self.lookup(name)?;
        if child.identity()? != *identity || !child.entries(1)?.is_empty() {
            return Err(
                Error::conflict("Private cleanup directory changed or is not empty").into(),
            );
        }
        #[cfg(test)]
        super::diff_storage_hook("remove-directory")?;
        binding()?;
        self.revalidate()?;
        child.revalidate()?;
        lock.revalidate()?;
        rustix::fs::unlinkat(&self.0.handle, name, AtFlags::REMOVEDIR)
            .map_err(|error| MutationError::from(Error::io(error)))?;
        let finish = || -> Result<(), Error> {
            after_unlink()?;
            #[cfg(test)]
            super::diff_storage_hook("sync-directory-removal")?;
            self.sync()
        };
        finish().map_err(|error| MutationError {
            error,
            applied: true,
        })
    }
}
