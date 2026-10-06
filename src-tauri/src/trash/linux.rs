use crate::linux_guard::{
    folders::{create_private_path, Directory},
    Error, Root,
};
use std::path::{Path, PathBuf};

pub(super) struct Trash {
    directory: Directory,
    files: Directory,
    info: Directory,
    top: Option<PathBuf>,
}

pub(super) fn data_home() -> Result<PathBuf, String> {
    if let Some(value) = std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        let path = PathBuf::from(value);
        if path.is_absolute() {
            return Ok(path);
        }
    }
    let home = std::env::var_os("HOME").ok_or("Home directory is unavailable")?;
    let home = PathBuf::from(home);
    if !home.is_absolute() {
        return Err("Home directory must be absolute".into());
    }
    Ok(home.join(".local/share"))
}

impl Trash {
    pub(super) fn for_source(source: &Directory, data_home: &Path) -> Result<Self, String> {
        let home = create_private_path(data_home).map_err(|e| e.to_string())?;
        if source.same_mount(&home) {
            let directory = home.child("Trash", true).map_err(|e| e.to_string())?;
            return Self::open(directory, None);
        }
        let top = mount_top(source)?;
        Self::open_volume(&top)
    }

    fn open_volume(top: &Directory) -> Result<Self, String> {
        // SAFETY: geteuid has no arguments or memory requirements.
        let uid = unsafe { libc::geteuid() };
        let directory = top
            .child(".Trash", false)
            .and_then(|shared| {
                shared.sticky()?;
                let directory = shared.child(&uid.to_string(), true)?;
                directory.private()?;
                Ok(directory)
            })
            .or_else(|_| top.child(&format!(".Trash-{uid}"), true))
            .map_err(|e| e.to_string())?;
        Self::open(directory, Some(top.path.clone()))
    }

    fn open(directory: Directory, top: Option<PathBuf>) -> Result<Self, String> {
        directory.private().map_err(|e| e.to_string())?;
        directory.probe_write().map_err(|e| e.to_string())?;
        let files = directory.child("files", true).map_err(|e| e.to_string())?;
        let info = directory.child("info", true).map_err(|e| e.to_string())?;
        files.private().map_err(|e| e.to_string())?;
        info.private().map_err(|e| e.to_string())?;
        Ok(Self {
            directory,
            files,
            info,
            top,
        })
    }

    pub(super) fn recycle(&self, source: &Directory, repository: &Root) -> Result<(), String> {
        self.recycle_with(source, repository, |target, name, renamed| {
            source.move_to_tracked(target, name, renamed)
        })
    }

    fn recycle_with(
        &self,
        source: &Directory,
        repository: &Root,
        mut move_folder: impl FnMut(&Directory, &str, &mut bool) -> Result<PathBuf, Error>,
    ) -> Result<(), String> {
        let bytes = self.info_bytes(source)?;
        for _ in 0..128 {
            let name =
                crate::linux_guard::storage::unique_name("skein-").map_err(|e| e.to_string())?;
            if !self.files.missing(&name).map_err(|e| e.to_string())? {
                continue;
            }
            let info_name = format!("{name}.trashinfo");
            match self.info.write_info(&info_name, bytes.as_bytes()) {
                Ok(()) => (),
                Err(error) if error.code == Some(libc::EEXIST) => continue,
                Err(error) => return Err(error.to_string()),
            }
            let mut renamed = false;
            let result = repository
                .probe_write()
                .and_then(|()| move_folder(&self.files, &name, &mut renamed));
            let Err(error) = result else {
                return Ok(());
            };
            if !renamed {
                self.info.remove_info(&info_name).map_err(|cleanup| {
                    format!("{error}; could not remove trash info: {cleanup}")
                })?;
                if error.code == Some(libc::EEXIST) {
                    continue;
                }
            }
            return Err(format!(
                "{error}; inspect {} and {}",
                source.path.display(),
                self.files.path.join(name).display()
            ));
        }
        Err("Trash names are exhausted; source retained".into())
    }

    fn info_bytes(&self, source: &Directory) -> Result<String, String> {
        self.directory.private().map_err(|e| e.to_string())?;
        self.files.private().map_err(|e| e.to_string())?;
        self.info.private().map_err(|e| e.to_string())?;
        if !source.same_mount(&self.files) {
            return Err("Cross-device folder moves are refused".into());
        }
        if self.directory.path.starts_with(&source.path)
            || source.path.starts_with(&self.directory.path)
        {
            return Err("Trash storage overlaps the source folder".into());
        }
        let original = match &self.top {
            Some(top) => source
                .path
                .strip_prefix(top)
                .map_err(|_| "Trash source is outside its mount")?,
            None => &source.path,
        };
        Ok(format!(
            "[Trash Info]\nPath={}\nDeletionDate={}\n",
            encode_path(original),
            deletion_date()?
        ))
    }
}

fn mount_top(source: &Directory) -> Result<Directory, String> {
    let mut current = Directory::open(&source.path).map_err(|e| e.to_string())?;
    for _ in 0..64 {
        let Some(path) = current.path.parent() else {
            return Ok(current);
        };
        let parent = Directory::open(path).map_err(|e| e.to_string())?;
        if !source.same_mount(&parent) {
            return Ok(current);
        }
        current = parent;
    }
    Err("Mount ancestry exceeds the bound".into())
}

fn encode_path(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str()
        .as_bytes()
        .iter()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"/-_.~".contains(byte) {
                (*byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn deletion_date() -> Result<String, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "System clock is before the epoch")?
        .as_secs();
    let now: libc::time_t = now
        .try_into()
        .map_err(|_| "System clock exceeds the supported range")?;
    let mut local = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: localtime_r initializes the supplied tm on success; both pointers remain valid.
    let result = unsafe { libc::localtime_r(&now, local.as_mut_ptr()) };
    if result.is_null() {
        return Err("Local deletion date is unavailable".into());
    }
    // SAFETY: localtime_r returned a non-null pointer to the initialized tm.
    let local = unsafe { local.assume_init() };
    Ok(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        local.tm_year + 1900,
        local.tm_mon + 1,
        local.tm_mday,
        local.tm_hour,
        local.tm_min,
        local.tm_sec
    ))
}

pub(super) fn trash_folders(
    settings: &crate::settings::Settings,
    set_id: &str,
    data_home: &Path,
) -> Result<Vec<super::TrashOutcome>, String> {
    let roots = registered_roots(settings)?;
    let (own, _) = crate::compare::set_roots(settings, set_id)?;
    let workspace = settings.workspace["root"]
        .as_str()
        .ok_or("Missing workspace root")?;
    let workspace = Root::open(Path::new(workspace), &[]).map_err(|e| e.to_string())?;
    workspace.probe_write().map_err(|e| e.to_string())?;
    let captures = own
        .iter()
        .map(|(_, path)| capture_source(&workspace, path))
        .collect::<Vec<_>>();
    super::trash_folders(settings, set_id, |path| {
        if roots.iter().any(|(id, other)| {
            *id != set_id && (other.starts_with(path) || path.starts_with(other))
        }) {
            return Err("Another set uses an overlapping folder".into());
        }
        let index = own
            .iter()
            .position(|(_, root)| root == path)
            .ok_or("Unregistered trash folder")?;
        let (parent, repository, directory) = captures[index].as_ref().map_err(Clone::clone)?;
        workspace.probe_write().map_err(|e| e.to_string())?;
        parent.revalidate().map_err(|e| e.to_string())?;
        repository.probe_write().map_err(|e| e.to_string())?;
        let trash = Trash::for_source(directory, data_home)?;
        trash.recycle(directory, repository)
    })
}

fn registered_roots(settings: &crate::settings::Settings) -> Result<Vec<(&str, PathBuf)>, String> {
    let sets = settings.workspace["sets"]
        .as_array()
        .ok_or("No registered sets")?;
    let mut ids = std::collections::HashSet::new();
    let mut roots = Vec::new();
    for set in sets {
        let id = set["id"].as_str().ok_or("Missing set identity")?;
        if !ids.insert(id) {
            return Err("Duplicate set identity; folders retained".into());
        }
        let items = set["items"].as_array().ok_or("Missing set items")?;
        let (bound, _) = crate::compare::set_roots(settings, id)?;
        if bound.len() != items.len() {
            return Err("A saved folder cannot be registered safely; folders retained".into());
        }
        roots.extend(bound.into_iter().map(|(_, path)| (id, path)));
    }
    Ok(roots)
}

fn capture_source(
    workspace: &Root,
    path: &Path,
) -> Result<(crate::linux_guard::mutation::Parent, Root, Directory), String> {
    let value = workspace.value().map_err(|e| e.to_string())?;
    let relative = path
        .strip_prefix(&value.path)
        .map_err(|_| "Trash folder is outside its workspace")?
        .to_str()
        .ok_or("Unsupported trash path")?;
    let parent = workspace
        .parent(relative, false)
        .map_err(|e| e.to_string())?;
    crate::git::valid_root(path.to_str().ok_or("Unsupported trash path")?)?;
    let repository = Root::open(path, &[path.join(".git/config")]).map_err(|e| e.to_string())?;
    repository.probe_write().map_err(|e| e.to_string())?;
    let directory = Directory::open(path).map_err(|e| e.to_string())?;
    Ok((parent, repository, directory))
}

#[cfg(test)]
mod tests;
