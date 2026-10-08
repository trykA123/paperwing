use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub fn requires_git(root: &Path, files: &[PathBuf], cancel: &AtomicBool) -> bool {
    if std::env::var_os("GIT_ATTR_SOURCE").is_some() {
        return true;
    }
    let metadata = match crate::git::batch_repository::resolve(root) {
        Ok(metadata) => metadata,
        Err(_) => return true,
    };
    let mut locations = global_locations();
    for directory in [metadata.directory, metadata.common] {
        locations.attributes.push(directory.join("info/attributes"));
        locations.config.push(directory.join("config"));
        locations.config.push(directory.join("config.worktree"));
    }
    if locations.requires_git() {
        return true;
    }
    let mut checked = HashSet::new();
    for path in files {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        if path
            .file_name()
            .is_some_and(|name| name == ".gitattributes")
        {
            return true;
        }
        for directory in path.ancestors().skip(1) {
            if !checked.insert(directory.to_path_buf()) {
                break;
            }
            if present(&root.join(directory).join(".gitattributes")) {
                return true;
            }
        }
    }
    false
}

#[derive(Default)]
struct Locations {
    attributes: Vec<PathBuf>,
    config: Vec<PathBuf>,
}

impl Locations {
    fn requires_git(&self) -> bool {
        self.attributes.iter().any(|path| present(path))
            || self.config.iter().any(|path| configured(path))
    }
}

fn present(path: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Ok(_) => true,
        Err(error) => error.kind() != std::io::ErrorKind::NotFound,
    }
}

fn configured(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.len() > 1024 * 1024 => return true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        _ => {}
    }
    match std::fs::read(path) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
            text.contains("attributesfile") || text.contains("[include")
        }
        Err(_) => true,
    }
}

fn global_locations() -> Locations {
    let mut locations = Locations::default();
    let home = std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|home| home.join(".config")));
    if let Some(xdg) = xdg {
        locations.attributes.push(xdg.join("git/attributes"));
        locations.config.push(xdg.join("git/config"));
    }
    if let Some(global) = std::env::var_os("GIT_CONFIG_GLOBAL") {
        locations.config.push(global.into());
    } else if let Some(home) = home {
        locations.config.push(home.join(".gitconfig"));
    }
    if let Some(system) = std::env::var_os("GIT_CONFIG_SYSTEM") {
        locations.config.push(system.into());
    }
    add_system_locations(&mut locations);
    locations
}

fn add_system_locations(locations: &mut Locations) {
    #[cfg(not(windows))]
    for directory in ["/etc", "/usr/etc", "/usr/local/etc"] {
        locations
            .attributes
            .push(Path::new(directory).join("gitattributes"));
        locations
            .config
            .push(Path::new(directory).join("gitconfig"));
    }
    #[cfg(windows)]
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(directory) = std::env::var_os(variable) {
            for suffix in ["Git/etc", "Git/mingw64/etc", "Git/mingw32/etc"] {
                let directory = Path::new(&directory).join(suffix);
                locations.attributes.push(directory.join("gitattributes"));
                locations.config.push(directory.join("gitconfig"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_configuration_and_global_attribute_files_trigger_fallback() {
        let fixture = crate::platform::Fixture::new("attribute-locations");
        let config = fixture.0.join("config");
        let attributes = fixture.0.join("attributes");
        let locations = Locations {
            attributes: vec![attributes.clone()],
            config: vec![config.clone()],
        };
        assert!(!locations.requires_git());
        std::fs::write(&config, b"[core]\n autocrlf = false\n").unwrap();
        assert!(!locations.requires_git());
        std::fs::write(&config, b"[CORE]\n attributesFile = elsewhere\n").unwrap();
        assert!(locations.requires_git());
        std::fs::write(&config, b"[includeIf \"gitdir:work\"]\n path = elsewhere\n").unwrap();
        assert!(locations.requires_git());
        std::fs::write(&config, b"").unwrap();
        std::fs::write(&attributes, b"*.txt binary\n").unwrap();
        assert!(locations.requires_git());
    }
}
