use super::{relative, Error, ErrorKind, Handle, Identity, CONFINED, METADATA_LIMIT};
use rustix::fs::{FileType, OFlags};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug)]
struct Protected {
    path: PathBuf,
    handle: Option<Handle>,
    identity: Option<Identity>,
    pointer: Option<Vec<u8>>,
}

#[derive(Debug)]
pub(super) struct Inner {
    pub path: PathBuf,
    pub handle: Handle,
    identity: Identity,
    inputs: Vec<PathBuf>,
    metadata: Vec<Protected>,
}

#[derive(Clone, Debug)]
pub(crate) struct Root(pub(super) Arc<Inner>);

#[derive(Debug)]
pub(crate) struct ReadFile {
    pub bytes: Vec<u8>,
    pub modified_ms: Option<u128>,
}

impl Root {
    pub(crate) fn open(path: &Path, metadata: &[PathBuf]) -> Result<Self, Error> {
        let handle = Handle::absolute(path, OFlags::RDONLY | OFlags::DIRECTORY)?;
        let identity = handle.identity()?;
        let mut inputs = metadata.to_vec();
        inputs.sort();
        inputs.dedup();
        let mut paths = inputs.clone();
        paths.push(path.join(".git"));
        paths.sort();
        paths.dedup();
        if paths.len() > METADATA_LIMIT {
            return Err(Error::new(
                ErrorKind::Limit,
                "Too many protected metadata locations",
            ));
        }
        let mut protected = Vec::new();
        for path in paths {
            let value = Protected::open(&path)?;
            if value.handle.as_ref().is_some_and(|handle| {
                rustix::fs::fstat(handle)
                    .is_ok_and(|stat| FileType::from_raw_mode(stat.st_mode) == FileType::Directory)
            }) {
                let common = Protected::open(&path.join("commondir"))?;
                protected.push(common);
            }
            protected.push(value);
        }
        if protected.len() > METADATA_LIMIT {
            return Err(Error::new(
                ErrorKind::Limit,
                "Too many protected metadata locations",
            ));
        }
        let root = Self(Arc::new(Inner {
            path: path.to_path_buf(),
            handle,
            identity,
            inputs,
            metadata: protected,
        }));
        root.revalidate()?;
        Ok(root)
    }

    pub(crate) fn revalidate(&self) -> Result<(), Error> {
        let current = Handle::absolute(&self.0.path, OFlags::RDONLY | OFlags::DIRECTORY)?;
        if current.identity()? != self.0.identity || self.0.handle.identity()? != self.0.identity {
            return Err(Error::conflict("Repository root identity changed"));
        }
        for saved in &self.0.metadata {
            let current = Protected::open(&saved.path)?;
            if current.identity != saved.identity || current.pointer != saved.pointer {
                return Err(Error::conflict(
                    "Repository metadata identity or pointer changed",
                ));
            }
            if let Some(held) = &saved.handle {
                if Some(held.identity()?) != saved.identity {
                    return Err(Error::conflict("Repository metadata identity changed"));
                }
            }
        }
        Ok(())
    }

    pub(super) fn protected(&self, relative: &str, identity: Option<&Identity>) -> bool {
        let path = self.0.path.join(relative);
        self.0.metadata.iter().any(|metadata| {
            path.starts_with(&metadata.path)
                || identity.is_some_and(|identity| metadata.identity.as_ref() == Some(identity))
        })
    }

    pub(crate) fn read(&self, path: &str, limit: usize) -> Result<Option<ReadFile>, Error> {
        relative(path)?;
        self.revalidate()?;
        if self.protected(path, None) {
            return Err(Error::unsupported("Repository metadata is protected"));
        }
        let handle = match Handle::open(
            &self.0.handle,
            Path::new(path),
            OFlags::RDONLY,
            rustix::fs::Mode::empty(),
            CONFINED,
        ) {
            Ok(handle) => handle,
            Err(error) if error.kind == ErrorKind::Missing => {
                self.revalidate()?;
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let identity = handle.identity()?;
        if self.protected(path, Some(&identity)) {
            return Err(Error::unsupported("Repository metadata is protected"));
        }
        let stat = rustix::fs::fstat(&handle).map_err(Error::io)?;
        let bytes = handle.read(limit)?;
        let current = Handle::open(
            &self.0.handle,
            Path::new(path),
            OFlags::RDONLY,
            rustix::fs::Mode::empty(),
            CONFINED,
        )?;
        if current.identity()? != identity {
            return Err(Error::conflict("File identity changed during read"));
        }
        self.revalidate()?;
        let modified_ms = (stat.st_mtime >= 0)
            .then_some(stat.st_mtime as u128 * 1000 + stat.st_mtime_nsec as u128 / 1_000_000);
        Ok(Some(ReadFile { bytes, modified_ms }))
    }

    pub(crate) fn probe_write(&self) -> Result<(), Error> {
        self.revalidate()?;
        super::metadata::directory(&self.0.handle)?;
        for protected in &self.0.metadata {
            if let Some(handle) = &protected.handle {
                if handle.identity()?.mount != self.0.identity.mount {
                    return Err(Error::unsupported(
                        "Cross-mount repository metadata is unsupported for Linux writes",
                    ));
                }
                let stat = rustix::fs::fstat(handle).map_err(Error::io)?;
                if FileType::from_raw_mode(stat.st_mode) == FileType::Directory {
                    super::metadata::directory(handle)?;
                } else {
                    super::metadata::snapshot(handle)?;
                }
            }
        }
        let stat = rustix::fs::fstatfs(&self.0.handle).map_err(Error::io)?;
        if stat.f_type != libc::EXT4_SUPER_MAGIC {
            return Err(Error::unsupported(
                "Linux writes currently require a proven local ext4 root",
            ));
        }
        let volume = rustix::fs::fstatvfs(&self.0.handle).map_err(Error::io)?;
        if volume
            .f_flag
            .contains(rustix::fs::StatVfsMountFlags::RDONLY)
        {
            return Err(Error::unsupported("Linux write root is read-only"));
        }
        rustix::fs::fsync(&self.0.handle).map_err(Error::io)
    }
}

impl Protected {
    fn open(path: &Path) -> Result<Self, Error> {
        let handle = match Handle::absolute(path, OFlags::RDONLY) {
            Ok(handle) => Some(handle),
            Err(error) if error.kind == ErrorKind::Missing => None,
            Err(error) => return Err(error),
        };
        let identity = handle.as_ref().map(Handle::identity).transpose()?;
        let pointer = handle
            .as_ref()
            .map(|handle| {
                let stat = rustix::fs::fstat(handle).map_err(Error::io)?;
                match FileType::from_raw_mode(stat.st_mode) {
                    FileType::Directory => Ok(None),
                    FileType::RegularFile => handle.read(4096).map(Some),
                    _ => Err(Error::unsupported("Unsupported repository metadata type")),
                }
            })
            .transpose()?
            .flatten();
        Ok(Self {
            path: path.to_path_buf(),
            handle,
            identity,
            pointer,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RootValue {
    pub path: PathBuf,
    pub identity: Identity,
    pub inputs: Vec<PathBuf>,
    pub protected: Vec<ProtectedValue>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProtectedValue {
    path: PathBuf,
    identity: Option<Identity>,
    pointer: Option<Vec<u8>>,
}
impl Root {
    pub(crate) fn value(&self) -> Result<RootValue, Error> {
        self.revalidate()?;
        Ok(RootValue {
            path: self.0.path.clone(),
            identity: self.0.identity.clone(),
            inputs: self.0.inputs.clone(),
            protected: self
                .0
                .metadata
                .iter()
                .map(|entry| ProtectedValue {
                    path: entry.path.clone(),
                    identity: entry.identity.clone(),
                    pointer: entry.pointer.clone(),
                })
                .collect(),
        })
    }
    pub(crate) fn reopen(value: &RootValue) -> Result<Self, Error> {
        value.validate()?;
        let root = Self::open(&value.path, &value.inputs)?;
        if root.value()? != *value {
            return Err(Error::conflict("Saved repository root or metadata changed"));
        }
        Ok(root)
    }
}
impl RootValue {
    pub(crate) fn protects(&self, relative: &str) -> bool {
        let path = self.path.join(relative);
        self.protected
            .iter()
            .any(|entry| path.starts_with(&entry.path))
    }
    pub(crate) fn validate(&self) -> Result<(), Error> {
        if !self.path.is_absolute()
            || self.path.as_os_str().len() > 4096
            || self.inputs.len() > METADATA_LIMIT
            || self.protected.len() > METADATA_LIMIT
        {
            return Err(Error::unsupported("Unsupported persisted root snapshot"));
        }
        for path in &self.inputs {
            if !path.is_absolute() || path.as_os_str().len() > 4096 {
                return Err(Error::unsupported("Unsupported persisted metadata path"));
            }
        }
        for entry in &self.protected {
            if !entry.path.is_absolute()
                || entry.path.as_os_str().len() > 4096
                || entry
                    .pointer
                    .as_ref()
                    .is_some_and(|bytes| bytes.len() > 4096)
            {
                return Err(Error::unsupported(
                    "Unsupported persisted protected metadata",
                ));
            }
            if entry.identity.is_none() && entry.pointer.is_some() {
                return Err(Error::unsupported("Invalid absent metadata snapshot"));
            }
        }
        Ok(())
    }
}

impl Root {
    pub(crate) fn separate(&self, target: &Path, prefix: &Path) -> Result<(), Error> {
        self.revalidate()?;
        let paths =
            std::iter::once(&self.0.path).chain(self.0.metadata.iter().map(|value| &value.path));
        if paths
            .into_iter()
            .any(|path| target.starts_with(path) || path.starts_with(target))
        {
            return Err(Error::unsupported(
                "Private diff storage overlaps compared source or metadata",
            ));
        }
        let mut identities = vec![self.0.identity.clone()];
        for value in &self.0.metadata {
            if let Some(handle) = &value.handle {
                if FileType::from_raw_mode(rustix::fs::fstat(handle).map_err(Error::io)?.st_mode)
                    == FileType::Directory
                {
                    identities.push(handle.identity()?);
                }
            }
        }
        let first = Handle::absolute(prefix, OFlags::RDONLY | OFlags::DIRECTORY)?.identity()?;
        if identities
            .iter()
            .any(|id| id.device == first.device && id.mount != first.mount)
        {
            return Err(Error::unsupported(
                "Private diff bind separation is unproven",
            ));
        }
        let mut values = Vec::new();
        for (index, path) in prefix.ancestors().enumerate() {
            if index > 64 {
                return Err(Error::new(
                    ErrorKind::Limit,
                    "Private diff ancestry exceeds the bound",
                ));
            }
            let handle = Handle::absolute(path, OFlags::RDONLY | OFlags::DIRECTORY)?;
            let id = handle.identity()?;
            if identities
                .iter()
                .any(|saved| saved.device == id.device && saved.inode == id.inode)
            {
                return Err(Error::unsupported(
                    "Private diff storage is inside compared source or metadata",
                ));
            }
            if handle.identity()? != id {
                return Err(Error::conflict("Private diff ancestor changed"));
            }
            values.push((path.to_path_buf(), id));
        }
        for (path, identity) in values {
            if Handle::absolute(&path, OFlags::RDONLY | OFlags::DIRECTORY)?.identity()? != identity
            {
                return Err(Error::conflict("Private diff ancestry changed"));
            }
        }
        self.revalidate()
    }
}
