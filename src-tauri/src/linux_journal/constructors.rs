use super::{Error, Journal};
#[cfg(test)]
use super::{RECORD_LIMIT, STORAGE_LIMIT};
use crate::linux_guard::{
    root::RootValue,
    storage::{Lock, Namespace, PrivateDir},
    Root,
};
use std::path::Path;

impl Journal {
    fn constructed(directory: PrivateDir, lock: Lock, guarded: Option<Vec<RootValue>>) -> Self {
        Self {
            directory,
            _lock: lock,
            guarded,
            #[cfg(test)]
            fault: None,
            #[cfg(test)]
            quota: STORAGE_LIMIT,
            #[cfg(test)]
            records: RECORD_LIMIT,
            #[cfg(test)]
            writer: true,
        }
    }
    pub(crate) fn open_guarded(app_data: &Path, roots: &[RootValue]) -> Result<Self, Error> {
        let directory = PrivateDir::initialize_guarded(app_data, roots, Namespace::Recovery)?;
        let lock = directory.lock()?;
        lock.revalidate()?;
        for value in roots {
            Root::reopen(value)?.separate(directory.path(), directory.path())?;
        }
        directory.sync()?;
        lock.revalidate()?;
        Ok(Self::constructed(directory, lock, Some(roots.to_vec())))
    }
    pub(crate) fn open_existing(app_data: &Path) -> Result<Option<Self>, Error> {
        let Some(directory) = PrivateDir::open_existing(app_data, Namespace::Recovery)? else {
            return Ok(None);
        };
        let lock = directory.lock_existing()?;
        lock.revalidate()?;
        Ok(Some(Self::constructed(directory, lock, None)))
    }
    pub(crate) fn parent_authority(&self, value: &RootValue) -> Result<Root, Error> {
        let roots = self
            .guarded
            .as_ref()
            .ok_or_else(|| Error::invalid("Guarded parent constructor authority is missing"))?;
        if !roots.contains(value) {
            return Err(Error::invalid("Parent plan source authority differs"));
        }
        self._lock.revalidate()?;
        for source in roots {
            Root::reopen(source)?.separate(self.directory.path(), self.directory.path())?;
        }
        let root = Root::reopen(value)?;
        root.probe_write()?;
        Ok(root)
    }
}
