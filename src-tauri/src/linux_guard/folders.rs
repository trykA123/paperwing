mod trash_file;
pub(crate) use trash_file::FileMove;

use super::{metadata, Error, ErrorKind, Handle, Identity, Root, CONFINED};
use rustix::fs::{Mode, OFlags, RenameFlags};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

pub(crate) struct Directory {
    pub path: PathBuf,
    handle: Handle,
    identity: Identity,
    ancestors: Vec<(PathBuf, Handle, Identity)>,
}

impl Directory {
    pub(crate) fn open(path: &Path) -> Result<Self, Error> {
        crate::git::valid_path(
            path.to_str()
                .ok_or_else(|| Error::unsupported("Unsupported folder encoding"))?,
            true,
        )
        .map_err(|_| Error::unsupported("Unsafe folder path"))?;
        let mut ancestors = Vec::new();
        for path in path.ancestors().skip(1) {
            if ancestors.len() >= 64 {
                return Err(Error::unsupported("Folder ancestry exceeds the bound"));
            }
            let handle = Handle::absolute(path, OFlags::RDONLY | OFlags::DIRECTORY)?;
            let identity = handle.identity()?;
            ancestors.push((path.to_path_buf(), handle, identity));
        }
        let handle = Handle::absolute(path, OFlags::RDONLY | OFlags::DIRECTORY)?;
        let identity = handle.identity()?;
        let directory = Self {
            path: path.to_path_buf(),
            handle,
            identity,
            ancestors,
        };
        directory.revalidate()?;
        Ok(directory)
    }

    pub(crate) fn revalidate(&self) -> Result<(), Error> {
        for (path, held, identity) in &self.ancestors {
            if held.identity()? != *identity
                || Handle::absolute(path, OFlags::RDONLY | OFlags::DIRECTORY)?.identity()?
                    != *identity
            {
                return Err(Error::conflict("Folder ancestor identity changed"));
            }
        }
        if self.handle.identity()? != self.identity
            || Handle::absolute(&self.path, OFlags::RDONLY | OFlags::DIRECTORY)?.identity()?
                != self.identity
        {
            return Err(Error::conflict("Folder identity changed"));
        }
        Ok(())
    }

    pub(crate) fn private(&self) -> Result<(), Error> {
        self.revalidate()?;
        super::storage::private_directory(&self.handle)
    }

    pub(crate) fn child(&self, name: &str, create: bool) -> Result<Self, Error> {
        leaf(name)?;
        self.revalidate()?;
        if create {
            match rustix::fs::mkdirat(&self.handle, name, Mode::from_raw_mode(0o700)) {
                Ok(()) => rustix::fs::fsync(&self.handle).map_err(Error::io)?,
                Err(rustix::io::Errno::EXIST) => (),
                Err(error) => return Err(Error::io(error)),
            }
        }
        let handle = Handle::open(
            &self.handle,
            Path::new(name),
            OFlags::RDONLY | OFlags::DIRECTORY,
            Mode::empty(),
            CONFINED,
        )?;
        let identity = handle.identity()?;
        self.revalidate()?;
        let directory = Self::open(&self.path.join(name))?;
        if directory.identity != identity {
            return Err(Error::conflict("Created folder identity changed"));
        }
        Ok(directory)
    }

    pub(crate) fn same_mount(&self, other: &Self) -> bool {
        self.identity.device == other.identity.device && self.identity.mount == other.identity.mount
    }

    pub(crate) fn probe_write(&self) -> Result<(), Error> {
        self.revalidate()?;
        Root::open(&self.path, &[])?.probe_write()
    }

    pub(crate) fn fd_path(&self) -> String {
        format!(
            "/proc/{}/fd/{}",
            std::process::id(),
            self.handle.file.as_raw_fd()
        )
    }

    pub(crate) fn missing(&self, name: &str) -> Result<bool, Error> {
        leaf(name)?;
        self.revalidate()?;
        match Handle::open(
            &self.handle,
            Path::new(name),
            OFlags::PATH,
            Mode::empty(),
            CONFINED,
        ) {
            Ok(_) => Ok(false),
            Err(error) if error.kind == ErrorKind::Missing => Ok(true),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn create_new(&self, name: &str) -> Result<Self, Error> {
        leaf(name)?;
        self.probe_write()?;
        rustix::fs::mkdirat(&self.handle, name, Mode::from_raw_mode(0o700)).map_err(Error::io)?;
        rustix::fs::fsync(&self.handle).map_err(Error::io)?;
        let child = self.child(name, false)?;
        child.private()?;
        Ok(child)
    }

    pub(crate) fn move_to(&self, target: &Self, name: &str) -> Result<PathBuf, Error> {
        self.move_to_tracked(target, name, &mut false)
    }

    pub(crate) fn move_to_tracked(
        &self,
        target: &Self,
        name: &str,
        renamed: &mut bool,
    ) -> Result<PathBuf, Error> {
        *renamed = false;
        leaf(name)?;
        self.probe_write()?;
        target.revalidate()?;
        if !self.same_mount(target) {
            return Err(Error::unsupported("Cross-device folder moves are refused"));
        }
        let parent = self.parent()?;
        self.revalidate()?;
        parent.revalidate()?;
        target.revalidate()?;
        let source = self
            .path
            .file_name()
            .ok_or_else(|| Error::unsupported("Missing folder name"))?;
        rustix::fs::renameat_with(
            &parent.handle,
            source,
            &target.handle,
            name,
            RenameFlags::NOREPLACE,
        )
        .map_err(Error::io)?;
        *renamed = true;
        let path = target.path.join(name);
        let moved = Self::open(&path)?;
        if moved.identity != self.identity {
            return Err(Error::conflict(
                "Moved folder identity changed; inspect retained data",
            ));
        }
        rustix::fs::fsync(&parent.handle).map_err(Error::io)?;
        rustix::fs::fsync(&target.handle).map_err(Error::io)?;
        Ok(path)
    }

    pub(crate) fn write_info(&self, name: &str, bytes: &[u8]) -> Result<(), Error> {
        leaf(name)?;
        self.private()?;
        let handle = Handle::open(
            &self.handle,
            Path::new(name),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL,
            Mode::from_raw_mode(0o600),
            CONFINED,
        )?;
        let result = (|| {
            let security = metadata::snapshot(&handle)?;
            if security.mode != 0o600 || !security.attributes.is_empty() {
                return Err(Error::unsupported(
                    "Trash info requires private ordinary metadata",
                ));
            }
            handle.write(bytes)?;
            self.revalidate()?;
            rustix::fs::fsync(&self.handle).map_err(Error::io)
        })();
        if result.is_err() {
            self.remove_info(name)?;
        }
        result
    }

    pub(crate) fn remove_info(&self, name: &str) -> Result<(), Error> {
        leaf(name)?;
        rustix::fs::unlinkat(&self.handle, name, rustix::fs::AtFlags::empty())
            .map_err(Error::io)?;
        rustix::fs::fsync(&self.handle).map_err(Error::io)
    }

    pub(crate) fn remove_created(&self) -> Result<(), Error> {
        self.private()?;
        let parent = self.parent()?;
        std::fs::remove_dir_all(&self.path).map_err(Error::io)?;
        rustix::fs::fsync(&parent.handle).map_err(Error::io)
    }

    fn parent(&self) -> Result<Self, Error> {
        let parent = Self::open(
            self.path
                .parent()
                .ok_or_else(|| Error::unsupported("Missing folder parent"))?,
        )?;
        metadata::directory(&parent.handle)?;
        if self.ancestors.first().map(|ancestor| &ancestor.2) != Some(&parent.identity) {
            return Err(Error::conflict("Folder parent identity changed"));
        }
        Ok(parent)
    }

    pub(crate) fn sticky(&self) -> Result<(), Error> {
        self.revalidate()?;
        if rustix::fs::fstat(&self.handle).map_err(Error::io)?.st_mode & 0o1000 == 0 {
            return Err(Error::unsupported("Shared trash requires the sticky bit"));
        }
        Ok(())
    }
}

fn leaf(name: &str) -> Result<(), Error> {
    crate::paths::filename_component(name, false)
        .map_err(|_| Error::unsupported("Unsafe folder name"))?;
    if name.len() > 255 {
        return Err(Error::unsupported("Folder name exceeds the limit"));
    }
    Ok(())
}

pub(crate) fn create_private_path(path: &Path) -> Result<Directory, Error> {
    crate::git::valid_path(
        path.to_str()
            .ok_or_else(|| Error::unsupported("Unsupported trash path"))?,
        false,
    )
    .map_err(|_| Error::unsupported("Unsafe trash path"))?;
    let mut prefix = path;
    let mut missing = Vec::new();
    while !prefix.exists() {
        missing.push(
            prefix
                .file_name()
                .ok_or_else(|| Error::unsupported("Missing trash path component"))?,
        );
        if missing.len() > 64 {
            return Err(Error::unsupported("Trash path exceeds the bound"));
        }
        prefix = prefix
            .parent()
            .ok_or_else(|| Error::unsupported("Missing trash parent"))?;
    }
    let mut directory = Directory::open(prefix)?;
    for part in missing.into_iter().rev() {
        metadata::directory(&directory.handle)?;
        directory = directory.child(
            part.to_str()
                .ok_or_else(|| Error::unsupported("Unsupported trash path encoding"))?,
            true,
        )?;
        directory.private()?;
    }
    Ok(directory)
}

#[cfg(test)]
pub(crate) struct Fixture(pub PathBuf);

#[cfg(test)]
impl Fixture {
    pub(crate) fn new(label: &str) -> Self {
        let root = crate::env_names::var_os("SKEIN_TEST_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp"));
        assert!(root.is_absolute());
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(super::storage::unique_name(label).unwrap());
        std::fs::create_dir(&path).unwrap();
        std::fs::write(
            path.join(".skein-folder-fixture"),
            b"packet15 disposable fixture",
        )
        .unwrap();
        assert_eq!(
            rustix::fs::fstatfs(std::fs::File::open(&path).unwrap())
                .unwrap()
                .f_type,
            libc::EXT4_SUPER_MAGIC
        );
        Self(path)
    }

    pub(crate) fn repository(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir(&path).unwrap();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["remote", "add", "origin", "https://example.test/repo"],
        ] {
            assert!(std::process::Command::new("git")
                .arg("-C")
                .arg(&path)
                .args(args)
                .status()
                .unwrap()
                .success());
        }
        std::fs::write(path.join("file"), b"restorable data").unwrap();
        let backup = std::fs::read(path.join("file")).unwrap();
        std::fs::write(path.join("file"), b"restore drill").unwrap();
        std::fs::write(path.join("file"), &backup).unwrap();
        assert_eq!(std::fs::read(path.join("file")).unwrap(), backup);
        path
    }
}

#[cfg(test)]
impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(
            std::fs::read(self.0.join(".skein-folder-fixture")).unwrap(),
            b"packet15 disposable fixture"
        );
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[cfg(test)]
mod tests;
