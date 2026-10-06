use super::{validate_directories, AncestorValue, Directory, Inner, Parent};
use crate::linux_guard::{
    metadata, relative, root::RootValue, Error, ErrorKind, Handle, Identity, Root, CONFINED,
};
use rustix::fs::{Mode, OFlags};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub(crate) trait OperationAuthority {
    fn refresh(&mut self) -> Result<(), Error>;
    fn check(&self) -> Result<(), Error>;
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ParentPlan {
    pub root: RootValue,
    pub destination: String,
    pub existing: Vec<AncestorValue>,
    pub missing: Vec<String>,
}
impl ParentPlan {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        self.root.validate()?;
        let parts = relative(&self.destination)?;
        if self.destination.len() > 4096
            || self.existing.len() + self.missing.len() != parts.len() - 1
        {
            return Err(Error::unsupported("Invalid parent plan component bound"));
        }
        let mut path = PathBuf::new();
        for (index, part) in parts[..parts.len() - 1].iter().enumerate() {
            path.push(part);
            let text = path
                .to_str()
                .ok_or_else(|| Error::unsupported("Unsupported parent path encoding"))?;
            if self.root.protects(text)
                || (index < self.existing.len() && self.existing[index].relative != path)
                || (index >= self.existing.len()
                    && self.missing[index - self.existing.len()] != text)
            {
                return Err(Error::unsupported("Invalid parent plan prefix"));
            }
        }
        if self.root.protects(&self.destination) {
            return Err(Error::unsupported("Repository metadata is protected"));
        }
        Ok(())
    }
    pub(crate) fn validate_created(&self, created: &[AncestorValue]) -> Result<(), Error> {
        self.validate()?;
        if created.len() > self.missing.len()
            || created
                .iter()
                .zip(&self.missing)
                .any(|(entry, path)| entry.relative != Path::new(path))
        {
            return Err(Error::unsupported("Invalid created parent prefix"));
        }
        Ok(())
    }
}
pub(crate) struct ParentCreation<'a> {
    pub plan: &'a ParentPlan,
    pub created: &'a [AncestorValue],
}
#[derive(Debug)]
pub(crate) struct ParentMutation {
    pub applied: bool,
    pub identity: Option<Identity>,
    pub error: Option<Error>,
}
struct Component<'a> {
    root: &'a Root,
    opened: Vec<Directory>,
    next: String,
    part: String,
}
impl Component<'_> {
    fn previous(&self) -> &Handle {
        self.opened
            .last()
            .map_or(&self.root.0.handle, |entry| &entry.handle)
    }
    fn validate(&self) -> Result<(), Error> {
        self.root.probe_write()?;
        validate_directories(self.root, &self.opened)?;
        self.root.require_missing(&self.next)?;
        metadata::directory(self.previous())
    }
    fn finish(&self, outcome: &mut ParentMutation) -> Result<(), Error> {
        #[cfg(test)]
        parent_hook("mkdir")?;
        let entry = directory(
            self.root,
            self.previous(),
            &self.part,
            Path::new(&self.next),
        )?;
        outcome.identity = Some(entry.identity.clone());
        if rustix::fs::fstat(&entry.handle).map_err(Error::io)?.st_mode & 0o777 != 0o700 {
            return Err(Error::unsupported("Created parent requires mode0700"));
        }
        #[cfg(test)]
        parent_hook("reopened")?;
        self.root.probe_write()?;
        validate_directories(self.root, &self.opened)?;
        if Handle::open(
            self.previous(),
            Path::new(&self.part),
            OFlags::RDONLY | OFlags::DIRECTORY,
            Mode::empty(),
            CONFINED,
        )?
        .identity()?
            != entry.identity
        {
            return Err(Error::conflict("Created parent identity changed"));
        }
        #[cfg(test)]
        parent_hook("parentSync")?;
        rustix::fs::fsync(self.previous()).map_err(Error::io)?;
        #[cfg(test)]
        parent_hook("parentSynced")?;
        Ok(())
    }
}
fn directory(root: &Root, previous: &Handle, part: &str, path: &Path) -> Result<Directory, Error> {
    let handle = Handle::open(
        previous,
        Path::new(part),
        OFlags::RDONLY | OFlags::DIRECTORY,
        Mode::empty(),
        CONFINED,
    )?;
    metadata::directory(&handle)?;
    let identity = handle.identity()?;
    let text = path
        .to_str()
        .ok_or_else(|| Error::unsupported("Unsupported parent path encoding"))?;
    if root.protected(text, Some(&identity)) {
        return Err(Error::unsupported("Repository metadata is protected"));
    }
    match Handle::open(
        &handle,
        Path::new(".git"),
        OFlags::PATH,
        Mode::empty(),
        CONFINED,
    ) {
        Err(error) if error.kind == ErrorKind::Missing => (),
        _ => {
            return Err(Error::unsupported(
                "Nested repository directories are protected",
            ))
        }
    }
    Ok(Directory {
        relative: path.to_path_buf(),
        handle,
        identity,
    })
}
impl Root {
    pub(crate) fn preview_parents(&self, destination: &str) -> Result<ParentPlan, Error> {
        let parts = relative(destination)?;
        if destination.len() > 4096 {
            return Err(Error::unsupported("Parent destination exceeds the bound"));
        }
        self.probe_write()?;
        if self.protected(destination, None) {
            return Err(Error::unsupported("Repository metadata is protected"));
        }
        let mut opened = Vec::<Directory>::new();
        let mut path = PathBuf::new();
        let mut missing = Vec::new();
        for part in &parts[..parts.len() - 1] {
            path.push(part);
            if missing.is_empty() {
                let previous = opened.last().map_or(&self.0.handle, |entry| &entry.handle);
                match directory(self, previous, part, &path) {
                    Ok(entry) => {
                        opened.push(entry);
                        continue;
                    }
                    Err(error) if error.kind == ErrorKind::Missing => (),
                    Err(error) => return Err(error),
                }
            }
            let text = path
                .to_str()
                .ok_or_else(|| Error::unsupported("Unsupported parent path encoding"))?;
            if self.protected(text, None) {
                return Err(Error::unsupported("Repository metadata is protected"));
            }
            missing.push(text.to_string());
        }
        self.probe_write()?;
        validate_directories(self, &opened)?;
        if let Some(first) = missing.first() {
            self.require_missing(first)?;
        }
        let plan = ParentPlan {
            root: self.value()?,
            destination: destination.into(),
            existing: ancestor_values(&opened),
            missing,
        };
        plan.validate()?;
        Ok(plan)
    }
    fn require_missing(&self, path: &str) -> Result<(), Error> {
        match Handle::open(
            &self.0.handle,
            Path::new(path),
            OFlags::PATH,
            Mode::empty(),
            CONFINED,
        ) {
            Err(error) if error.kind == ErrorKind::Missing => Ok(()),
            Ok(_) => Err(Error::conflict(
                "Expected missing parent was created by another writer",
            )),
            Err(error) => Err(error),
        }
    }
    fn planned_prefix(&self, input: &ParentCreation<'_>) -> Result<Vec<Directory>, Error> {
        input.plan.validate_created(input.created)?;
        self.probe_write()?;
        if self.value()? != input.plan.root {
            return Err(Error::conflict("Parent plan root authority changed"));
        }
        let mut opened = Vec::<Directory>::new();
        for (index, saved) in input.plan.existing.iter().chain(input.created).enumerate() {
            let previous = opened.last().map_or(&self.0.handle, |entry| &entry.handle);
            let part = saved
                .relative
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| Error::unsupported("Unsupported parent path encoding"))?;
            let entry = directory(self, previous, part, &saved.relative)?;
            if entry.identity != saved.identity {
                return Err(Error::conflict("Saved destination ancestor changed"));
            }
            if index >= input.plan.existing.len()
                && rustix::fs::fstat(&entry.handle).map_err(Error::io)?.st_mode & 0o777 != 0o700
            {
                return Err(Error::conflict("Created parent permissions changed"));
            }
            opened.push(entry);
        }
        validate_directories(self, &opened)?;
        Ok(opened)
    }
    pub(crate) fn verify_parent_prefix(&self, input: ParentCreation<'_>) -> Result<(), Error> {
        self.planned_prefix(&input)?;
        Ok(())
    }
    pub(crate) fn verify_parent_next(&self, input: ParentCreation<'_>) -> Result<(), Error> {
        self.planned_prefix(&input)?;
        let next = input
            .plan
            .missing
            .get(input.created.len())
            .ok_or_else(|| Error::conflict("No missing parent remains"))?;
        self.require_missing(next)
    }
    pub(crate) fn planned_parent(&self, input: ParentCreation<'_>) -> Result<Parent, Error> {
        if input.created.len() != input.plan.missing.len() {
            return Err(Error::conflict("Parent creation is incomplete"));
        }
        let directories = self.planned_prefix(&input)?;
        let leaf = relative(&input.plan.destination)?
            .last()
            .ok_or_else(|| Error::unsupported("Missing destination leaf"))?
            .to_string();
        let parent = Parent(Arc::new(Inner {
            root: self.clone(),
            directories,
            relative: input.plan.destination.clone(),
            leaf,
        }));
        parent.revalidate()?;
        Ok(parent)
    }
    pub(crate) fn create_parent_component(
        &self,
        input: ParentCreation<'_>,
        authority: &mut impl OperationAuthority,
        binding: impl FnOnce() -> Result<(), Error>,
    ) -> ParentMutation {
        let mut outcome = ParentMutation {
            applied: false,
            identity: None,
            error: None,
        };
        let result = (|| -> Result<(), Error> {
            input.plan.validate_created(input.created)?;
            let next = input
                .plan
                .missing
                .get(input.created.len())
                .ok_or_else(|| Error::unsupported("No missing parent remains"))?;
            authority.refresh()?;
            let opened = self.planned_prefix(&input)?;
            let part = Path::new(next)
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| Error::unsupported("Unsupported parent path encoding"))?
                .to_string();
            let component = Component {
                root: self,
                opened,
                next: next.clone(),
                part,
            };
            component.validate()?;
            binding()?;
            authority.check()?;
            component.create()?;
            outcome.applied = true;
            component.finish(&mut outcome)
        })();
        outcome.error = result.err();
        outcome
    }
}
impl Identity {
    pub(crate) fn maximum_width() -> Self {
        Self {
            device: (u32::MAX, u32::MAX),
            inode: u64::MAX,
            mount: u64::MAX,
        }
    }
}
#[cfg(test)]
type ParentHook = Box<dyn FnMut(&str) -> Result<(), Error>>;
#[cfg(test)]
pub(crate) fn set_parent_hook(hook: Option<ParentHook>) {
    PARENT_HOOK.with(|slot| *slot.borrow_mut() = hook);
}
#[cfg(test)]
thread_local! { pub(super) static PARENT_HOOK: std::cell::RefCell<Option<ParentHook>> = const {std::cell::RefCell::new(None)}; }
#[cfg(test)]
fn parent_hook(phase: &str) -> Result<(), Error> {
    PARENT_HOOK.with(|hook| {
        if let Some(hook) = hook.borrow_mut().as_mut() {
            hook(phase)
        } else {
            Ok(())
        }
    })
}

impl Component<'_> {
    fn create(&self) -> Result<(), Error> {
        rustix::fs::mkdirat(
            self.previous(),
            self.part.as_str(),
            Mode::from_raw_mode(0o700),
        )
        .map_err(|error| {
            if error == rustix::io::Errno::EXIST {
                Error::conflict("Expected missing parent was created by another writer")
            } else {
                Error::io(error)
            }
        })
    }
}
fn ancestor_values(opened: &[Directory]) -> Vec<AncestorValue> {
    opened
        .iter()
        .map(|entry| AncestorValue {
            relative: entry.relative.clone(),
            identity: entry.identity.clone(),
        })
        .collect()
}
