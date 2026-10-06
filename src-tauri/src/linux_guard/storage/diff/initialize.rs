use super::{diff_path, qualify, Namespace};
use crate::linux_guard::{
    root::RootValue,
    storage::{private_directory, Inner, PrivateDir},
    Error, ErrorKind, Handle, Identity, Root, CONFINED,
};
use rustix::fs::{Mode, OFlags};
use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};

struct Binding {
    path: PathBuf,
    handle: Handle,
    identity: Identity,
}
struct Initializer<'a> {
    target: PathBuf,
    namespace: Namespace,
    sources: &'a [RootValue],
    held: Vec<Binding>,
}
impl<'a> Initializer<'a> {
    fn new(parent: &Path, sources: &'a [RootValue], namespace: Namespace) -> Result<Self, Error> {
        diff_path(parent)?;
        let target = parent.join(namespace.name());
        diff_path(&target)?;
        if sources.is_empty() || sources.len() > 2 {
            return Err(Error::unsupported(
                "Private diff source authority is missing",
            ));
        }
        let path = PathBuf::from("/");
        let handle = Handle::absolute(&path, OFlags::RDONLY | OFlags::DIRECTORY)?;
        let identity = handle.identity()?;
        Ok(Self {
            target,
            namespace,
            sources,
            held: vec![Binding {
                path,
                handle,
                identity,
            }],
        })
    }
    fn current(&self) -> Result<&Binding, Error> {
        self.held
            .last()
            .ok_or_else(|| Error::unsupported("Private diff parent is missing"))
    }
    fn verify_sources(&self, target: &Path) -> Result<(), Error> {
        let prefix = &self.current()?.path;
        for value in self.sources {
            Root::reopen(value)?.separate(target, prefix)?;
        }
        Ok(())
    }
    fn verify_bindings(&self) -> Result<(), Error> {
        for binding in &self.held {
            if binding.handle.identity()? != binding.identity
                || Handle::absolute(&binding.path, OFlags::RDONLY | OFlags::DIRECTORY)?
                    .identity()?
                    != binding.identity
            {
                return Err(Error::conflict(
                    "Private diff initialization parent changed",
                ));
            }
        }
        Ok(())
    }
    fn push(&mut self, path: PathBuf, handle: Handle) -> Result<(), Error> {
        let identity = handle.identity()?;
        self.held.push(Binding {
            path,
            handle,
            identity,
        });
        Ok(())
    }
    fn existing(&mut self, parts: &[Component<'_>]) -> Result<usize, Error> {
        for (index, part) in parts.iter().enumerate() {
            let current = self.current()?;
            let handle = match Handle::open(
                &current.handle,
                Path::new(part.as_os_str()),
                OFlags::RDONLY | OFlags::DIRECTORY,
                Mode::empty(),
                CONFINED - rustix::fs::ResolveFlags::NO_XDEV,
            ) {
                Ok(handle) => handle,
                Err(error) if error.kind == ErrorKind::Missing => return Ok(index),
                Err(error) => return Err(error),
            };
            self.push(current.path.join(part.as_os_str()), handle)?;
        }
        Ok(parts.len())
    }
    fn extend(&mut self, part: Component<'_>) -> Result<(), Error> {
        #[cfg(test)]
        initialize_hook(&self.current()?.path);
        self.verify_sources(&self.target)?;
        let current = self.current()?;
        let candidate = current.path.join(part.as_os_str());
        self.verify_sources(&candidate)?;
        self.verify_bindings()?;
        qualify(&current.handle)?;
        crate::linux_guard::metadata::directory(&current.handle)?;
        create(&current.handle, Path::new(part.as_os_str()))?;
        let handle = Handle::open(
            &current.handle,
            Path::new(part.as_os_str()),
            OFlags::RDONLY | OFlags::DIRECTORY,
            Mode::empty(),
            CONFINED - rustix::fs::ResolveFlags::NO_XDEV,
        )?;
        private_directory(&handle)?;
        rustix::fs::fsync(&current.handle).map_err(Error::io)?;
        self.push(candidate, handle)
    }
    fn namespace(&self) -> Result<Handle, Error> {
        let current = self.current()?;
        let open = || {
            Handle::open(
                &current.handle,
                Path::new(self.namespace.name()),
                OFlags::RDONLY | OFlags::DIRECTORY,
                Mode::empty(),
                CONFINED,
            )
        };
        match open() {
            Ok(handle) => Ok(handle),
            Err(error) if error.kind == ErrorKind::Missing => {
                #[cfg(test)]
                initialize_hook(&current.path);
                self.verify_sources(&self.target)?;
                self.verify_bindings()?;
                create(&current.handle, Path::new(self.namespace.name()))?;
                rustix::fs::fsync(&current.handle).map_err(Error::io)?;
                open()
            }
            Err(error) => Err(error),
        }
    }
    fn finish(self) -> Result<PrivateDir, Error> {
        self.verify_sources(&self.target)?;
        self.verify_bindings()?;
        let current = self.current()?;
        qualify(&current.handle)?;
        crate::linux_guard::metadata::directory(&current.handle)?;
        let handle = self.namespace()?;
        private_directory(&handle)?;
        qualify(&handle)?;
        let identity = handle.identity()?;
        let directory = PrivateDir(Arc::new(Inner {
            path: self.target.clone(),
            handle,
            identity,
        }));
        directory.revalidate()?;
        for value in self.sources {
            Root::reopen(value)?.separate(&self.target, directory.path())?;
        }
        Ok(directory)
    }
}
fn create(parent: &Handle, name: &Path) -> Result<(), Error> {
    match rustix::fs::mkdirat(parent, name, Mode::from_raw_mode(0o700)) {
        Ok(()) => Ok(()),
        Err(error) if error == rustix::io::Errno::EXIST => Ok(()),
        Err(error) => Err(Error::io(error)),
    }
}
impl PrivateDir {
    pub(crate) fn initialize_diff(parent: &Path, sources: &[RootValue]) -> Result<Self, Error> {
        Self::initialize_guarded(parent, sources, Namespace::Diff)
    }
    pub(crate) fn initialize_guarded(
        parent: &Path,
        sources: &[RootValue],
        namespace: Namespace,
    ) -> Result<Self, Error> {
        let mut initializer = Initializer::new(parent, sources, namespace)?;
        let parts = parent
            .strip_prefix("/")
            .map_err(|_| Error::unsupported("Invalid private diff path"))?
            .components()
            .collect::<Vec<_>>();
        let missing = initializer.existing(&parts)?;
        initializer.verify_sources(&initializer.target)?;
        qualify(&initializer.current()?.handle)?;
        for part in &parts[missing..] {
            initializer.extend(*part)?;
        }
        initializer.finish()
    }
    pub(crate) fn open_existing(
        parent: &Path,
        namespace: Namespace,
    ) -> Result<Option<Self>, Error> {
        diff_path(parent)?;
        let parent_handle = match Handle::absolute(parent, OFlags::RDONLY | OFlags::DIRECTORY) {
            Ok(handle) => handle,
            Err(error) if error.kind == ErrorKind::Missing => return Ok(None),
            Err(error) => return Err(error),
        };
        let handle = match Handle::open(
            &parent_handle,
            Path::new(namespace.name()),
            OFlags::RDONLY | OFlags::DIRECTORY,
            Mode::empty(),
            CONFINED,
        ) {
            Ok(handle) => handle,
            Err(error) if error.kind == ErrorKind::Missing => return Ok(None),
            Err(error) => return Err(error),
        };
        private_directory(&handle)?;
        qualify(&handle)?;
        let identity = handle.identity()?;
        let directory = Self(Arc::new(Inner {
            path: parent.join(namespace.name()),
            handle,
            identity,
        }));
        directory.revalidate()?;
        Ok(Some(directory))
    }
}
#[cfg(test)]
type InitializeHook = Box<dyn FnMut(&Path)>;
#[cfg(test)]
thread_local! {
    pub(crate) static INITIALIZE_HOOK: std::cell::RefCell<Option<InitializeHook>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
fn initialize_hook(path: &Path) {
    INITIALIZE_HOOK.with(|hook| {
        if let Some(hook) = hook.borrow_mut().as_mut() {
            hook(path);
        }
    });
}
