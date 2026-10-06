use super::{metadata, relative, Error, ErrorKind, BytePermit, Handle, Identity, Root, CONFINED, FILE_LIMIT};
use rustix::fs::{AtFlags, Mode, OFlags, RenameFlags};
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod authorized;
mod parents;
pub(crate) use authorized::AuthorizedPublication;
pub(crate) type ParentMutation = parents::ParentMutation;
#[cfg(test)]
pub(crate) use parents::set_parent_hook;
pub(crate) use parents::{OperationAuthority, ParentCreation, ParentPlan};
#[cfg(test)]
mod parent_tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Snapshot { Missing, Regular { identity: Identity, bytes: Vec<u8>, security: metadata::Security } }

impl Snapshot {
    pub(crate) fn bytes(&self) -> Option<&[u8]> { match self { Self::Missing => None, Self::Regular { bytes, .. } => Some(bytes) } }
}

#[derive(Debug)]
struct Directory { relative: PathBuf, handle: Handle, identity: Identity }
#[derive(Debug)]
struct Inner { root: Root, directories: Vec<Directory>, relative: String, leaf: String }
#[derive(Clone, Debug)]
pub(crate) struct Parent(Arc<Inner>);

#[derive(Debug)]
pub(crate) struct Staged { parent: Parent, name: String, directory: Handle, identity: Identity, file: Option<Handle>, file_identity: Option<Identity>, bytes: Vec<u8>, _bytes: BytePermit, published: bool }
#[derive(Debug)]
pub(crate) struct Published { pub identity: Identity }
#[derive(Debug)]
pub(crate) struct MutationError { pub error: Error, pub applied: bool }
impl From<Error> for MutationError { fn from(error: Error) -> Self { Self { error, applied: false } } }

impl Root {
    pub(crate) fn parent(&self, path: &str, create: bool) -> Result<Parent, Error> {
        let parts = relative(path)?;
        self.probe_write()?;
        if self.protected(path, None) { return Err(Error::unsupported("Repository metadata is protected")); }
        let mut directories: Vec<Directory> = Vec::new();
        let mut relative = PathBuf::new();
        for part in &parts[..parts.len() - 1] {
            relative.push(part);
            let previous = directories.last().map_or(&self.0.handle, |directory| &directory.handle);
            let handle = match Handle::open(previous, Path::new(part), OFlags::RDONLY | OFlags::DIRECTORY, Mode::empty(), CONFINED) {
                Ok(handle) => handle,
                Err(error) if create && error.kind == ErrorKind::Missing => {
                    self.probe_write()?;
                    validate_directories(self, &directories)?;
                    rustix::fs::mkdirat(previous, *part, Mode::from_raw_mode(0o700)).map_err(Error::io)?;
                    rustix::fs::fsync(previous).map_err(Error::io)?;
                    Handle::open(previous, Path::new(part), OFlags::RDONLY | OFlags::DIRECTORY, Mode::empty(), CONFINED)?
                },
                Err(error) => return Err(error),
            };
            metadata::directory(&handle)?;
            let identity = handle.identity()?;
            let text = relative.to_str().ok_or_else(|| Error::unsupported("Unsupported path encoding"))?;
            if self.protected(text, Some(&identity)) { return Err(Error::unsupported("Repository metadata is protected")); }
            match Handle::open(&handle, Path::new(".git"), OFlags::PATH, Mode::empty(), CONFINED) {
                Err(error) if error.kind == ErrorKind::Missing => (),
                _ => return Err(Error::unsupported("Nested repository directories are protected")),
            }
            directories.push(Directory { relative: relative.clone(), handle, identity });
        }
        let parent = Parent(Arc::new(Inner { root: self.clone(), directories, relative: path.to_string(), leaf: parts.last().unwrap().to_string() }));
        parent.revalidate()?;
        Ok(parent)
    }
}

fn validate_directories(root: &Root, directories: &[Directory]) -> Result<(), Error> {
        for saved in directories {
            let current = Handle::open(&root.0.handle, &saved.relative, OFlags::RDONLY | OFlags::DIRECTORY, Mode::empty(), CONFINED)?;
            if current.identity()? != saved.identity || saved.handle.identity()? != saved.identity {
                return Err(Error::conflict("Destination ancestor identity changed"));
            }
            metadata::directory(&current)?;
            match Handle::open(&current, Path::new(".git"), OFlags::PATH, Mode::empty(), CONFINED) {
                Err(error) if error.kind == ErrorKind::Missing => (),
                _ => return Err(Error::unsupported("Nested repository directories are protected")),
            }
        }
    Ok(())
}

impl Parent {
    fn directory(&self) -> &Handle { self.0.directories.last().map_or(&self.0.root.0.handle, |directory| &directory.handle) }

    pub(crate) fn revalidate(&self) -> Result<(), Error> {
        self.0.root.probe_write()?;
        validate_directories(&self.0.root, &self.0.directories)?;
        Ok(())
    }

    pub(crate) fn snapshot(&self) -> Result<Snapshot, Error> {
        self.revalidate()?;
        let handle = match Handle::open(self.directory(), Path::new(&self.0.leaf), OFlags::RDONLY, Mode::empty(), CONFINED) {
            Ok(handle) => handle,
            Err(error) if error.kind == ErrorKind::Missing => { self.revalidate()?; return Ok(Snapshot::Missing); },
            Err(error) => return Err(error),
        };
        let identity = handle.identity()?;
        if self.0.root.protected(&self.0.relative, Some(&identity)) { return Err(Error::unsupported("Repository metadata is protected")); }
        let security = metadata::snapshot(&handle)?;
        let bytes = handle.read(FILE_LIMIT)?;
        if metadata::snapshot(&handle)? != security { return Err(Error::conflict("File metadata changed during snapshot")); }
        let current = Handle::open(self.directory(), Path::new(&self.0.leaf), OFlags::RDONLY, Mode::empty(), CONFINED)?;
        if current.identity()? != identity { return Err(Error::conflict("Destination file identity changed")); }
        self.revalidate()?;
        Ok(Snapshot::Regular { identity, bytes, security })
    }

    pub(crate) fn validate(&self, expected: &Snapshot) -> Result<(), Error> {
        if &self.snapshot()? != expected { return Err(Error::conflict("Destination bytes, identity or metadata changed")); }
        Ok(())
    }

    pub(crate) fn stage(&self, bytes: &[u8], expected: &Snapshot) -> Result<Staged, Error> {
        self.stage_named(bytes, expected, &super::storage::unique_name(".paperwing-stage-")?)
    }
    pub(crate) fn stage_named(&self, bytes: &[u8], expected: &Snapshot, name: &str) -> Result<Staged, Error> {
        stage_name(name)?;
        if bytes.len() > FILE_LIMIT { return Err(Error::new(ErrorKind::Limit, "File content exceeds the limit")); }
        let byte_permit = BytePermit::acquire(bytes.len())?;
        self.validate(expected)?;
        let name = name.to_string();
        rustix::fs::mkdirat(self.directory(), name.as_str(), Mode::from_raw_mode(0o700)).map_err(Error::io)?;
        let directory = Handle::open(self.directory(), Path::new(&name), OFlags::RDONLY | OFlags::DIRECTORY, Mode::empty(), CONFINED)?;
        super::storage::private_directory(&directory)?;
        let identity = directory.identity()?;
        let mut stage = Staged { parent: self.clone(), name, directory, identity, file: None, file_identity: None,
            bytes: bytes.to_vec(), _bytes: byte_permit, published: false };
        let file = Handle::open(&stage.directory, Path::new("content"), OFlags::RDWR | OFlags::CREATE | OFlags::EXCL, Mode::from_raw_mode(0o600), CONFINED)?;
        let file_identity = file.identity()?;
        stage.file_identity = Some(file_identity);
        stage.file = Some(file);
        stage.file()?.write(bytes)?;
        rustix::fs::fsync(&stage.directory).map_err(Error::io)?;
        rustix::fs::fsync(self.directory()).map_err(Error::io)?;
        self.validate(expected)?;
        Ok(stage)
    }

    pub(crate) fn publish(&self, stage: Staged, expected: &Snapshot) -> Result<Published, MutationError> {
        self.publish_inner(stage, expected, None, || Ok(()), || Ok(()), sync_directory)
    }

    fn publish_inner(&self, mut stage: Staged, expected: &Snapshot, restore: Option<&metadata::Security>, after_check: impl FnOnce() -> Result<(), Error>, after_mutation: impl FnOnce() -> Result<(), Error>, sync: fn(&Handle) -> Result<(), Error>) -> Result<Published, MutationError> {
        if !Arc::ptr_eq(&self.0, &stage.parent.0) { return Err(Error::unsupported("Staged file belongs to another destination").into()); }
        self.validate(expected)?;
        let current = Handle::open(self.directory(), Path::new(&stage.name), OFlags::RDONLY | OFlags::DIRECTORY, Mode::empty(), CONFINED)?;
        if current.identity()? != stage.identity { return Err(Error::conflict("Staging directory changed").into()); }
        super::storage::private_directory(&current)?;
        let file = Handle::open(&current, Path::new("content"), OFlags::RDONLY, Mode::empty(), CONFINED)?;
        if Some(file.identity()?) != stage.file_identity { return Err(Error::conflict("Staged file identity changed").into()); }
        let security = restore.cloned().unwrap_or_else(|| match expected { Snapshot::Missing => metadata::Security::new_file(), Snapshot::Regular { security, .. } => security.clone() });
        security.apply(stage.file()?)?;
        if stage.file()?.read(FILE_LIMIT)? != stage.bytes { return Err(Error::conflict("Staged file bytes changed").into()); }
        self.validate(expected)?;
        let final_file = Handle::open(&current, Path::new("content"), OFlags::RDONLY, Mode::empty(), CONFINED)?;
        if Some(final_file.identity()?) != stage.file_identity || final_file.read(FILE_LIMIT)? != stage.bytes
            || metadata::snapshot(&final_file)? != security { return Err(Error::conflict("Staged file bytes or metadata changed").into()); }
        after_check()?;
        match expected {
            Snapshot::Missing => rustix::fs::renameat_with(&stage.directory, "content", self.directory(), self.0.leaf.as_str(), RenameFlags::NOREPLACE),
            Snapshot::Regular { .. } => rustix::fs::renameat(&stage.directory, "content", self.directory(), self.0.leaf.as_str()),
        }.map_err(|error| MutationError::from(if error == rustix::io::Errno::EXIST { Error::conflict("Destination was created by another writer") } else { Error::io(error) }))?;
        stage.published = true;
        after_mutation().map_err(|error| MutationError { error, applied: true })?;
        let identity = stage.file()?.identity().map_err(|error| MutationError { error, applied: true })?;
        sync(self.directory()).map_err(|error| MutationError { error, applied: true })?;
        rustix::fs::fsync(&stage.directory).map_err(|error| MutationError { error: Error::io(error), applied: true })?;
        Ok(Published { identity })
    }

    pub(crate) fn remove_created(&self, expected: &Snapshot) -> Result<(), MutationError> {
        self.remove_inner(expected, || Ok(()), || Ok(()), sync_directory)
    }

    pub(crate) fn remove_authorized(&self, expected: &Snapshot, authority: impl FnOnce() -> Result<(), Error>) -> Result<(), MutationError> {
        self.remove_inner(expected, authority, || Ok(()), sync_directory)
    }

    #[cfg(test)]
    pub(crate) fn remove_authorized_observed(&self, expected: &Snapshot, authority: impl FnOnce() -> Result<(), Error>, action: impl FnOnce() -> Result<(), Error>) -> Result<(), MutationError> {
        self.remove_inner(expected, authority, action, sync_directory)
    }

    fn remove_inner(&self, expected: &Snapshot, after_check: impl FnOnce() -> Result<(), Error>, after_mutation: impl FnOnce() -> Result<(), Error>, sync: fn(&Handle) -> Result<(), Error>) -> Result<(), MutationError> {
        if matches!(expected, Snapshot::Missing) { return Err(Error::unsupported("Created-file removal requires verified current bytes").into()); }
        self.validate(expected)?;
        after_check()?;
        rustix::fs::unlinkat(self.directory(), self.0.leaf.as_str(), AtFlags::empty()).map_err(|error| MutationError::from(Error::io(error)))?;
        after_mutation().map_err(|error| MutationError { error, applied: true })?;
        sync(self.directory()).map_err(|error| MutationError { error, applied: true })
    }

    #[cfg(test)]
    pub(super) fn publish_with_sync(&self, stage: Staged, expected: &Snapshot, sync: fn(&Handle) -> Result<(), Error>) -> Result<Published, MutationError> {
        self.publish_inner(stage, expected, None, || Ok(()), || Ok(()), sync)
    }
    #[cfg(test)]
    pub(super) fn remove_with_sync(&self, expected: &Snapshot, sync: fn(&Handle) -> Result<(), Error>) -> Result<(), MutationError> {
        self.remove_inner(expected, || Ok(()), || Ok(()), sync)
    }
    #[cfg(test)]
    pub(super) fn publish_after_check(&self, stage: Staged, expected: &Snapshot, action: impl FnOnce() -> Result<(), Error>) -> Result<Published, MutationError> {
        self.publish_inner(stage, expected, None, action, || Ok(()), sync_directory)
    }
    #[cfg(test)]
    pub(super) fn remove_after_check(&self, expected: &Snapshot, action: impl FnOnce() -> Result<(), Error>) -> Result<(), MutationError> {
        self.remove_inner(expected, action, || Ok(()), sync_directory)
    }
}

fn sync_directory(handle: &Handle) -> Result<(), Error> { rustix::fs::fsync(handle).map_err(Error::io) }

impl Staged {
    fn file(&self) -> Result<&Handle, Error> { self.file.as_ref().ok_or_else(|| Error::conflict("Staged file is incomplete")) }
}

impl Drop for Staged {
    fn drop(&mut self) {
        let cleanup = || -> Result<(), Error> {
            self.parent.revalidate()?;
            let current = Handle::open(self.parent.directory(), Path::new(&self.name), OFlags::RDONLY | OFlags::DIRECTORY, Mode::empty(), CONFINED)?;
            if current.identity()? != self.identity { return Err(Error::conflict("Staging directory changed")); }
            if !self.published && self.file_identity.is_some() {
                let file = Handle::open(&current, Path::new("content"), OFlags::RDONLY, Mode::empty(), CONFINED)?;
                if Some(file.identity()?) != self.file_identity || file.read(FILE_LIMIT)? != self.bytes { return Err(Error::conflict("Staged file changed")); }
                rustix::fs::unlinkat(&current, "content", AtFlags::empty()).map_err(Error::io)?;
            }
            rustix::fs::unlinkat(self.parent.directory(), self.name.as_str(), AtFlags::REMOVEDIR).map_err(Error::io)?;
            rustix::fs::fsync(self.parent.directory()).map_err(Error::io)
        };
        let _ = cleanup();
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AncestorValue { pub relative: PathBuf, pub identity: Identity }
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StageProof { pub name: String, pub directory: Identity, pub file: Identity }
#[derive(Debug)]
pub(crate) struct StageArtifact { pub name: String, pub directory: Identity, pub content: Option<(Identity, u64)>, pub size: u64 }
fn stage_name(name: &str) -> Result<(), Error> {
    if name.len() != 49 || !name.starts_with(".paperwing-stage-") || !name[17..].bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
        return Err(Error::unsupported("Invalid journal staging name"));
    }
    Ok(())
}
impl Parent {
    pub(crate) fn ancestors(&self) -> Result<Vec<AncestorValue>, Error> {
        self.revalidate()?;
        Ok(self.0.directories.iter().map(|entry| AncestorValue { relative: entry.relative.clone(), identity: entry.identity.clone() }).collect())
    }
    pub(crate) fn matches_ancestors(&self, values: &[AncestorValue]) -> Result<(), Error> {
        if self.ancestors()? != values { return Err(Error::conflict("Saved destination ancestors changed")); }
        Ok(())
    }
    pub(crate) fn prepare_security(&self, stage: &Staged, security: &metadata::Security) -> Result<(), Error> {
        if !Arc::ptr_eq(&self.0, &stage.parent.0) { return Err(Error::unsupported("Stage belongs to another destination")); }
        self.revalidate()?; security.apply(stage.file()?)
    }
    pub(crate) fn publish_restore(&self, stage: Staged, expected: &Snapshot, security: &metadata::Security) -> Result<Published, MutationError> {
        self.publish_inner(stage, expected, Some(security), || Ok(()), || Ok(()), sync_directory)
    }
    pub(crate) fn stage_artifacts(&self, limit: usize) -> Result<Vec<StageArtifact>, Error> {
        self.revalidate()?;
        let mut result = Vec::new();
        for name in super::entries(self.directory(), limit)? {
            if !name.starts_with(".paperwing-stage-") { continue; }
            stage_name(&name)?;
            let directory = Handle::open(self.directory(), Path::new(&name), OFlags::RDONLY | OFlags::DIRECTORY, Mode::empty(), CONFINED)?;
            super::storage::private_directory(&directory)?;
            let names = super::entries(&directory, 2)?;
            if names.iter().any(|name| name != "content") { return Err(Error::unsupported("Unknown persistent stage artifact")); }
            let content = if names.is_empty() { None } else {
                let file = Handle::open(&directory, Path::new("content"), OFlags::RDONLY, Mode::empty(), CONFINED)?;
                let security = metadata::snapshot(&file)?;
                let metadata_size = security.attributes.iter().try_fold(0u64, |total, (name, bytes)| total.checked_add((name.len() + bytes.len()) as u64)).ok_or_else(|| Error::new(ErrorKind::Limit, "Persistent metadata size overflow"))?;
                let size = rustix::fs::fstat(&file).map_err(Error::io)?.st_size;
                if size < 0 || size as u64 > FILE_LIMIT as u64 { return Err(Error::new(ErrorKind::Limit, "Persistent stage content exceeds the limit")); }
                Some((file.identity()?, (size as u64).checked_add(metadata_size).ok_or_else(|| Error::new(ErrorKind::Limit, "Persistent stage size overflow"))?))
            };
            let stat = rustix::fs::fstat(&directory).map_err(Error::io)?;
            let size = (stat.st_size as u64).checked_add(content.as_ref().map_or(0, |(_, size)| *size))
                .ok_or_else(|| Error::new(ErrorKind::Limit, "Persistent stage size overflow"))?;
            result.push(StageArtifact { name, directory: directory.identity()?, content, size });
        }
        self.revalidate()?;
        Ok(result)
    }
    #[cfg(test)]
    pub(crate) fn publish_observed(&self, stage: Staged, expected: &Snapshot, security: &metadata::Security, action: impl FnOnce() -> Result<(), Error>) -> Result<Published, MutationError> {
        self.publish_inner(stage, expected, Some(security), || Ok(()), action, sync_directory)
    }
    #[cfg(test)]
    pub(crate) fn remove_observed(&self, expected: &Snapshot, action: impl FnOnce() -> Result<(), Error>) -> Result<(), MutationError> {
        self.remove_inner(expected, || Ok(()), action, sync_directory)
    }
    pub(crate) fn verify_stage(&self, proof: &StageProof, bytes: &[u8], security: &metadata::Security) -> Result<(), Error> {
        stage_name(&proof.name)?; self.revalidate()?;
        let directory = Handle::open(self.directory(), Path::new(&proof.name), OFlags::RDONLY | OFlags::DIRECTORY, Mode::empty(), CONFINED)?;
        super::storage::private_directory(&directory)?;
        if directory.identity()? != proof.directory { return Err(Error::conflict("Persistent stage directory changed")); }
        let file = Handle::open(&directory, Path::new("content"), OFlags::RDONLY, Mode::empty(), CONFINED)?;
        if file.identity()? != proof.file || file.read(FILE_LIMIT)? != bytes || metadata::snapshot(&file)? != *security { return Err(Error::conflict("Persistent stage content changed")); }
        Ok(())
    }
    pub(crate) fn cleanup_stage(&self, proof: &StageProof, bytes: &[u8], security: &metadata::Security) -> Result<(), Error> {
        stage_name(&proof.name)?; self.revalidate()?;
        let directory = match Handle::open(self.directory(), Path::new(&proof.name), OFlags::RDONLY | OFlags::DIRECTORY, Mode::empty(), CONFINED) {
            Ok(directory) => directory,
            Err(error) if error.kind == ErrorKind::Missing => return Ok(()),
            Err(error) => return Err(error),
        };
        if directory.identity()? != proof.directory { return Err(Error::conflict("Persistent stage directory changed")); }
        super::storage::private_directory(&directory)?;
        let names = super::entries(&directory, 2)?;
        if names.iter().any(|name| name != "content") { return Err(Error::conflict("Persistent stage has unknown artifacts")); }
        if !names.is_empty() {
            let file = Handle::open(&directory, Path::new("content"), OFlags::RDONLY, Mode::empty(), CONFINED)?;
            if file.identity()? != proof.file || file.read(FILE_LIMIT)? != bytes || metadata::snapshot(&file)? != *security {
                return Err(Error::conflict("Persistent stage content changed"));
            }
            rustix::fs::unlinkat(&directory, "content", AtFlags::empty()).map_err(Error::io)?;
            rustix::fs::fsync(&directory).map_err(Error::io)?;
        }
        self.revalidate()?;
        rustix::fs::unlinkat(self.directory(), proof.name.as_str(), AtFlags::REMOVEDIR).map_err(Error::io)?;
        rustix::fs::fsync(self.directory()).map_err(Error::io)
    }
}
impl Staged {
    pub(crate) fn proof(&self) -> Result<StageProof, Error> {
        Ok(StageProof { name: self.name.clone(), directory: self.identity.clone(), file: self.file_identity.clone().ok_or_else(|| Error::conflict("Stage is incomplete"))? })
    }
}
