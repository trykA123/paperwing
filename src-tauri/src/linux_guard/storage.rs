use super::{Error, ErrorKind, Handle, Identity, CONFINED};
use rustix::fs::{FlockOperation, Mode, OFlags};
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod diff;
mod access;
mod removal;
pub(crate) use diff::{diff_path, LockAttempt, Namespace, DIFF_NAMESPACE_BYTES};
pub(crate) use removal::{ArtifactRemoval, DirectoryRemoval};
#[cfg(test)]
pub(crate) use diff::INITIALIZE_HOOK;

pub(crate) fn unique_name(prefix: &str) -> Result<String, Error> {
    let mut bytes = [0u8; 16];
    let mut offset = 0;
    while offset < bytes.len() {
        let count = unsafe { libc::getrandom(bytes[offset..].as_mut_ptr().cast(), bytes.len() - offset, 0) };
        if count < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted { continue; }
            return Err(Error::io(error));
        }
        if count == 0 { return Err(Error::new(ErrorKind::Io, "Random identity unavailable")); }
        offset += count as usize;
    }
    Ok(format!("{prefix}{}", bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>()))
}

pub(super) fn private_directory(handle: &Handle) -> Result<(), Error> {
    super::metadata::directory(handle)?;
    let stat = rustix::fs::fstat(handle).map_err(Error::io)?;
    let mut names = [0u8; 65536];
    if rustix::fs::flistxattr(handle, &mut names).map_err(Error::io)? != 0 { return Err(Error::unsupported("Private directory metadata changed")); }
    if stat.st_mode & 0o777 != 0o700 { return Err(Error::unsupported("Private Linux storage requires mode0700")); }
    Ok(())
}

#[derive(Debug)]
struct Inner { path: PathBuf, handle: Handle, identity: Identity }
#[derive(Clone, Debug)]
pub(crate) struct PrivateDir(Arc<Inner>);
#[derive(Debug)]
pub(crate) struct PrivateFile { directory: PrivateDir, name: String, handle: Handle, identity: Identity }
#[derive(Debug)]
pub(crate) struct Lock(PrivateFile);

fn leaf(name: &str) -> Result<(), Error> {
    if name.len() > 255 { return Err(Error::new(ErrorKind::Limit, "Private storage name exceeds the limit")); }
    crate::paths::filename_component(name, false).map_err(|_| Error::unsupported("Unsafe private storage name"))
}

impl PrivateDir {
    pub(crate) fn open(parent: &Path, name: &str) -> Result<Self, Error> {
        leaf(name)?;
        let parent_handle = Handle::absolute(parent, OFlags::RDONLY | OFlags::DIRECTORY)?;
        let path = parent.join(name);
        let handle = match Handle::open(&parent_handle, Path::new(name), OFlags::RDONLY | OFlags::DIRECTORY, Mode::empty(), CONFINED) {
            Ok(handle) => handle,
            Err(error) if error.kind == ErrorKind::Missing => {
                match rustix::fs::mkdirat(&parent_handle, name, Mode::from_raw_mode(0o700)) {
                    Ok(()) => {},
                    Err(error) if error == rustix::io::Errno::EXIST => {},
                    Err(error) => return Err(Error::io(error)),
                }
                rustix::fs::fsync(&parent_handle).map_err(Error::io)?;
                Handle::open(&parent_handle, Path::new(name), OFlags::RDONLY | OFlags::DIRECTORY, Mode::empty(), CONFINED)?
            },
            Err(error) => return Err(error),
        };
        private_directory(&handle)?;
        let identity = handle.identity()?;
        let directory = Self(Arc::new(Inner { path, handle, identity }));
        directory.revalidate()?;
        Ok(directory)
    }

    pub(crate) fn revalidate(&self) -> Result<(), Error> {
        let current = Handle::absolute(&self.0.path, OFlags::RDONLY | OFlags::DIRECTORY)?;
        private_directory(&current)?;
        if current.identity()? != self.0.identity || self.0.handle.identity()? != self.0.identity {
            return Err(Error::conflict("Private storage identity changed"));
        }
        Ok(())
    }

    pub(crate) fn child(&self, name: &str) -> Result<Self, Error> { self.revalidate()?; Self::open(&self.0.path, name) }

    pub(crate) fn file(&self, name: &str, create: bool) -> Result<PrivateFile, Error> {
        leaf(name)?; self.revalidate()?;
        let flags = if create { OFlags::RDWR | OFlags::CREATE | OFlags::EXCL } else { OFlags::RDWR };
        let handle = Handle::open(&self.0.handle, Path::new(name), flags, if create { Mode::from_raw_mode(0o600) } else { Mode::empty() }, CONFINED)?;
        let security = super::metadata::snapshot(&handle)?;
        if security.mode != 0o600 || !security.attributes.is_empty() { return Err(Error::unsupported("Private Linux files require mode0600")); }
        let identity = handle.identity()?;
        let file = PrivateFile { directory: self.clone(), name: name.to_string(), handle, identity };
        file.revalidate()?;
        if create { rustix::fs::fsync(&self.0.handle).map_err(Error::io)?; }
        Ok(file)
    }

    pub(crate) fn lock(&self) -> Result<Lock, Error> {
        let file = match self.file("lock", false) {
            Ok(file) => file,
            Err(error) if error.kind == ErrorKind::Missing => self.file("lock", true)?,
            Err(error) => return Err(error),
        };
        rustix::fs::flock(&file.handle, FlockOperation::NonBlockingLockExclusive)
            .map_err(|_| Error::conflict("Another Skein process owns the Linux recovery lock"))?;
        file.revalidate()?;
        Ok(Lock(file))
    }
}

impl PrivateFile {
    pub(crate) fn revalidate(&self) -> Result<(), Error> {
        self.directory.revalidate()?;
        let current = Handle::open(&self.directory.0.handle, Path::new(&self.name), OFlags::RDONLY, Mode::empty(), CONFINED)?;
        let security = super::metadata::snapshot(&current)?;
        if security.mode != 0o600 || !security.attributes.is_empty() || current.identity()? != self.identity { return Err(Error::conflict("Private file identity or permissions changed")); }
        Ok(())
    }
    pub(crate) fn read(&self, limit: usize) -> Result<Vec<u8>, Error> { self.revalidate()?; self.handle.read(limit) }
    pub(crate) fn write(&self, bytes: &[u8]) -> Result<(), Error> { self.revalidate()?; self.handle.write(bytes)?; self.revalidate() }
}

impl Lock { pub(crate) fn revalidate(&self) -> Result<(), Error> { self.0.revalidate() } }
impl Drop for Lock { fn drop(&mut self) { let _ = rustix::fs::flock(&self.0.handle, FlockOperation::Unlock); } }

#[derive(Debug)]
pub(crate) struct Temporary { directory: PrivateDir, files: Vec<(PrivateFile, Vec<u8>, super::BytePermit)> }

impl Temporary {
    pub(crate) fn new(parent: &Path) -> Result<Self, Error> {
        Ok(Self { directory: PrivateDir::open(parent, &unique_name("skein-diff-")?)?, files: Vec::new() })
    }
    pub(crate) fn path(&self) -> Result<PathBuf, Error> { self.directory.revalidate()?; Ok(self.directory.0.path.clone()) }
    pub(crate) fn write(&mut self, name: &str, bytes: &[u8]) -> Result<PathBuf, Error> {
        if bytes.len() > super::FILE_LIMIT { return Err(Error::new(ErrorKind::Limit, "File content exceeds the limit")); }
        let permit = super::BytePermit::acquire(bytes.len())?;
        let file = self.directory.file(name, true)?;
        self.files.push((file, bytes.to_vec(), permit));
        self.files.last().unwrap().0.write(bytes)?;
        Ok(self.path()?.join(name))
    }
}

impl Drop for Temporary {
    fn drop(&mut self) {
        for (file, expected, _) in &self.files {
            let cleanup = || -> Result<(), Error> {
                file.revalidate()?;
                if file.handle.read(super::FILE_LIMIT)? != *expected { return Err(Error::conflict("Private diff file bytes changed")); }
                rustix::fs::unlinkat(&file.directory.0.handle, file.name.as_str(), rustix::fs::AtFlags::empty()).map_err(Error::io)?;
                rustix::fs::fsync(&file.directory.0.handle).map_err(Error::io)
            };
            let _ = cleanup();
        }
        let cleanup = || -> Result<(), Error> {
            self.directory.revalidate()?;
            let path = &self.directory.0.path;
            let parent = path.parent().ok_or_else(|| Error::unsupported("Private diff parent is unavailable"))?;
            let name = path.file_name().ok_or_else(|| Error::unsupported("Private diff name is unavailable"))?;
            let parent = Handle::absolute(parent, OFlags::RDONLY | OFlags::DIRECTORY)?;
            let current = Handle::open(&parent, Path::new(name), OFlags::RDONLY | OFlags::DIRECTORY, Mode::empty(), CONFINED)?;
            if current.identity()? != self.directory.0.identity { return Err(Error::conflict("Private diff directory identity changed")); }
            rustix::fs::unlinkat(&parent, Path::new(name), rustix::fs::AtFlags::REMOVEDIR).map_err(Error::io)?;
            rustix::fs::fsync(&parent).map_err(Error::io)
        };
        let _ = cleanup();
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Entry { pub name: String, pub identity: Identity, pub size: u64, pub directory: bool }
impl PrivateDir {
    pub(crate) fn lookup(&self, name: &str) -> Result<Self, Error> {
        leaf(name)?; self.revalidate()?;
        let handle = Handle::open(&self.0.handle, Path::new(name), OFlags::RDONLY | OFlags::DIRECTORY, Mode::empty(), CONFINED)?;
        private_directory(&handle)?;
        let identity = handle.identity()?;
        let result = Self(Arc::new(Inner { path: self.0.path.join(name), handle, identity }));
        result.revalidate()?;
        Ok(result)
    }
    pub(crate) fn create_child(&self, name: &str) -> Result<Self, Error> {
        leaf(name)?; self.revalidate()?;
        rustix::fs::mkdirat(&self.0.handle, name, Mode::from_raw_mode(0o700)).map_err(Error::io)?;
        self.sync()?;
        self.lookup(name)
    }
    pub(crate) fn names(&self, limit: usize) -> Result<Vec<String>, Error> { self.revalidate()?; super::entries(&self.0.handle, limit) }
    pub(crate) fn entries(&self, limit: usize) -> Result<Vec<Entry>, Error> {
        self.revalidate()?;
        let names = super::entries(&self.0.handle, limit)?;
        let mut result = Vec::new();
        for name in names {
            let handle = Handle::open(&self.0.handle, Path::new(&name), OFlags::RDONLY, Mode::empty(), CONFINED)?;
            let stat = rustix::fs::fstat(&handle).map_err(Error::io)?;
            let directory = match rustix::fs::FileType::from_raw_mode(stat.st_mode) {
                rustix::fs::FileType::Directory => { private_directory(&handle)?; true },
                rustix::fs::FileType::RegularFile => {
                    let security = super::metadata::snapshot(&handle)?;
                    if security.mode != 0o600 || !security.attributes.is_empty() { return Err(Error::unsupported("Unsafe private artifact metadata")); }
                    false
                },
                _ => return Err(Error::unsupported("Unsafe private artifact type")),
            };
            if stat.st_size < 0 { return Err(Error::unsupported("Invalid private artifact size")); }
            result.push(Entry { name, identity: handle.identity()?, size: stat.st_size as u64, directory });
        }
        self.revalidate()?;
        Ok(result)
    }
    pub(crate) fn identity(&self) -> Result<Identity, Error> { self.revalidate()?; Ok(self.0.identity.clone()) }
    pub(crate) fn path(&self) -> &Path { &self.0.path }
    pub(crate) fn sync(&self) -> Result<(), Error> { self.revalidate()?; rustix::fs::fsync(&self.0.handle).map_err(Error::io) }
    pub(crate) fn local_ext4(&self) -> Result<(), Error> {
        self.revalidate()?;
        if rustix::fs::fstatfs(&self.0.handle).map_err(Error::io)?.f_type != libc::EXT4_SUPER_MAGIC {
            return Err(Error::unsupported("Linux recovery currently requires proven local ext4 storage"));
        }
        Ok(())
    }
    pub(crate) fn write_new(&self, name: &str, bytes: &[u8]) -> Result<(), Error> {
        let file = self.file(name, true)?;
        file.write(bytes)?;
        if file.read(bytes.len())? != bytes { return Err(Error::conflict("Private artifact verification failed")); }
        self.sync()
    }
    pub(crate) fn unlink_owned(&self, name: &str, identity: &Identity, bytes: &[u8]) -> Result<(), Error> {
        let file = self.file(name, false)?;
        if &file.identity != identity || file.read(bytes.len())? != bytes { return Err(Error::conflict("Private cleanup artifact changed")); }
        file.revalidate()?;
        rustix::fs::unlinkat(&self.0.handle, name, rustix::fs::AtFlags::empty()).map_err(Error::io)?;
        self.sync()
    }
    pub(crate) fn remove_empty(&self, name: &str, identity: &Identity) -> Result<(), Error> {
        let child = self.lookup(name)?;
        if child.identity()? != *identity || !child.entries(1)?.is_empty() { return Err(Error::conflict("Private cleanup directory changed or is not empty")); }
        self.revalidate()?;
        rustix::fs::unlinkat(&self.0.handle, name, rustix::fs::AtFlags::REMOVEDIR).map_err(Error::io)?;
        self.sync()
    }
}
impl PrivateFile { pub(crate) fn identity(&self) -> Result<Identity, Error> { self.revalidate()?; Ok(self.identity.clone()) } }

pub(crate) fn local_ext4_parent(path: &Path) -> Result<(), Error> {
    let handle = Handle::absolute(path, OFlags::RDONLY | OFlags::DIRECTORY)?;
    super::metadata::directory(&handle)?;
    if rustix::fs::fstatfs(&handle).map_err(Error::io)?.f_type != libc::EXT4_SUPER_MAGIC {
        return Err(Error::unsupported("Linux recovery requires proven local ext4 storage"));
    }
    if rustix::fs::fstatvfs(&handle).map_err(Error::io)?.f_flag.contains(rustix::fs::StatVfsMountFlags::RDONLY) {
        return Err(Error::unsupported("Linux recovery storage is read-only"));
    }
    rustix::fs::fsync(&handle).map_err(Error::io)
}

impl PrivateDir {
    pub(crate) fn outside_root(&self, root: &Identity) -> Result<(), Error> {
        self.revalidate()?;
        if self.0.identity.device == root.device && self.0.identity.mount != root.mount {
            return Err(Error::unsupported("Same-device recovery bind separation is unproven"));
        }
        let mut values = Vec::new();
        for (index, path) in self.0.path.ancestors().enumerate() {
            if index >= 64 { return Err(Error::new(ErrorKind::Limit, "Recovery store ancestry exceeds the bound")); }
            let handle = Handle::absolute(path, OFlags::RDONLY | OFlags::DIRECTORY)?;
            let identity = handle.identity()?;
            if identity.device == root.device && identity.inode == root.inode {
                return Err(Error::unsupported("Recovery store must stay outside the physical target root"));
            }
            if handle.identity()? != identity { return Err(Error::conflict("Recovery store ancestor identity changed")); }
            values.push((path.to_path_buf(), identity));
        }
        for (path, identity) in values {
            let handle = Handle::absolute(&path, OFlags::RDONLY | OFlags::DIRECTORY)?;
            if handle.identity()? != identity { return Err(Error::conflict("Recovery store ancestry changed")); }
        }
        self.revalidate()
    }
}


#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileProof { pub name: String, pub identity: Identity, pub length: u64, pub sha256: String, pub uid: u32, pub gid: u32, pub mode: u32 }
impl PrivateFile {
    pub(crate) fn proof(&self, limit: usize) -> Result<FileProof, Error> {
        use sha2::{Digest, Sha256};
        use std::os::unix::fs::FileExt;
        self.revalidate()?;
        let security = super::metadata::snapshot(&self.handle)?;
        let stat = rustix::fs::fstat(&self.handle).map_err(Error::io)?;
        if stat.st_size < 0 || stat.st_size as u64 > limit as u64 { return Err(Error::new(ErrorKind::Limit,"Private artifact exceeds the content bound")); }
        let mut digest=Sha256::new(); let mut block=[0u8;8192]; let mut length=0u64;
        loop {
            let count=self.handle.file.read_at(&mut block,length).map_err(Error::io)?;
            if count==0 { break; }
            length=length.checked_add(count as u64).ok_or_else(||Error::new(ErrorKind::Limit,"Private artifact length overflow"))?;
            if length>limit as u64 { return Err(Error::new(ErrorKind::Limit,"Private artifact exceeds the content bound")); }
            digest.update(&block[..count]);
        }
        self.revalidate()?;
        if length != stat.st_size as u64 || super::metadata::snapshot(&self.handle)? != security { return Err(Error::conflict("Private artifact changed during verification")); }
        Ok(FileProof{name:self.name.clone(),identity:self.identity.clone(),length,sha256:format!("{:x}",digest.finalize()),uid:security.uid,gid:security.gid,mode:security.mode})
    }
}
impl PrivateDir {
    pub(crate) fn unlink_proven(&self, proof: &FileProof, lock: &Lock) -> Result<(), Error> {
        let file=self.file(&proof.name,false)?;
        if file.proof(super::FILE_LIMIT)? != *proof { return Err(Error::conflict("Private cleanup artifact changed")); }
        lock.revalidate()?; file.revalidate()?;
        #[cfg(test)]
        diff_storage_hook("unlink-file")?;
        rustix::fs::unlinkat(&self.0.handle,proof.name.as_str(),rustix::fs::AtFlags::empty()).map_err(Error::io)?;
        #[cfg(test)]
        diff_storage_hook("sync-file-removal")?;
        self.sync()
    }
}
#[cfg(test)]
pub(crate) type DiffStorageHook = Box<dyn FnMut(&str) -> Result<(), Error>>;
#[cfg(test)]
thread_local! {
    pub(crate) static DIFF_STORAGE_HOOK: std::cell::RefCell<Option<DiffStorageHook>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
pub(crate) fn diff_storage_hook(phase: &str) -> Result<(), Error> {
    DIFF_STORAGE_HOOK.with(|hook| { if let Some(hook) = hook.borrow_mut().as_mut() { hook(phase) } else { Ok(()) } })
}

impl PrivateDir {
    pub(crate) fn remove_empty_locked(&self, name: &str, identity: &Identity, lock: &Lock) -> Result<(), Error> {
        let child=self.lookup(name)?;
        if child.identity()?!=*identity || !child.entries(1)?.is_empty() {return Err(Error::conflict("Private cleanup directory changed or is not empty"));}
        self.revalidate()?; child.revalidate()?; lock.revalidate()?;
        #[cfg(test)]
        diff_storage_hook("remove-directory")?;
        rustix::fs::unlinkat(&self.0.handle,name,rustix::fs::AtFlags::REMOVEDIR).map_err(Error::io)?;
        self.sync()
    }
}
