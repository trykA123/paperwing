use super::{leaf, Directory};
use crate::linux_guard::{
    mutation::{Parent, Snapshot},
    Error, Root,
};
use std::path::PathBuf;

pub(crate) struct FileMove<'a> {
    pub leaf: &'a str,
    pub target: &'a Directory,
    pub name: &'a str,
    pub parent: &'a Parent,
    pub expected: &'a Snapshot,
}

impl Directory {
    pub(crate) fn move_file_to_tracked(
        &self,
        operation: FileMove<'_>,
        renamed: &mut bool,
    ) -> Result<PathBuf, Error> {
        *renamed = false;
        leaf(operation.leaf)?;
        leaf(operation.name)?;
        self.revalidate()?;
        operation.target.revalidate()?;
        if !self.same_mount(operation.target) {
            return Err(Error::unsupported("Cross-device trash moves are refused"));
        }
        operation.parent.validate(operation.expected)?;
        rustix::fs::renameat_with(
            &self.handle,
            operation.leaf,
            &operation.target.handle,
            operation.name,
            rustix::fs::RenameFlags::NOREPLACE,
        )
        .map_err(Error::io)?;
        *renamed = true;
        let actual = Root::open(&operation.target.path, &[])?
            .parent(operation.name, false)?
            .snapshot()?;
        if actual != *operation.expected {
            return Err(Error::conflict(
                "Moved file changed; inspect retained Trash data",
            ));
        }
        rustix::fs::fsync(&self.handle).map_err(Error::io)?;
        rustix::fs::fsync(&operation.target.handle).map_err(Error::io)?;
        Ok(operation.target.path.join(operation.name))
    }
}
