pub fn valid_url(u: &str) -> Result<(), String> {
    if u.is_empty() || u.starts_with('-') || u.chars().any(|c| c.is_control() || c.is_whitespace()) {
        Err("Invalid repository URL".into())
    } else {
        Ok(())
    }
}

pub fn valid_ref(name: &str) -> Result<(), String> {
    if name.is_empty() || name == "@" || name.starts_with('-') || name.ends_with('.') || name.contains("..") || name.contains("@{")
        || name.chars().any(|character| character.is_control() || character.is_whitespace() || "~^:?*[\\".contains(character))
        || name.split('/').any(|part| part.is_empty() || part.starts_with('.') || part.ends_with(".lock")) {
        return Err("Invalid ref name".into());
    }
    Ok(())
}

pub fn valid_path(path: &str, must_exist: bool) -> Result<(), String> {
    use std::path::{Component, Path, PathBuf};
    if path.contains('\0') { return Err("NUL is not supported in repository paths".into()); }
    if path.split(|character| character == '/' || (cfg!(windows) && character == '\\')).any(|part| part == "." || part == "..") {
        return Err("Traversal is not supported".into());
    }
    let path = Path::new(path);
    if !path.is_absolute() { return Err("Repository path must be absolute".into()); }
    let mut ancestor = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir | Component::CurDir => return Err("Traversal is not supported".into()),
            Component::Normal(part) => {
                let text = part.to_str().ok_or("Unsupported repository path")?;
                crate::paths::filename_component(text, cfg!(windows))?;
            }
            Component::Prefix(prefix) => {
                #[cfg(windows)]
                if !matches!(prefix.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::UNC(_, _)) {
                    return Err("Unsupported repository device prefix".into());
                }
                let _ = prefix;
            }
            _ => {}
        }
        ancestor.push(component.as_os_str());
        match std::fs::symlink_metadata(&ancestor) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() { return Err("Linked repository paths are not supported".into()); }
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if metadata.file_attributes() & 0x400 != 0 { return Err("Reparse repository paths are not supported".into()); }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !must_exist => {}
            Err(_) => return Err("Repository path is unavailable".into()),
        }
    }
    Ok(())
}

pub fn valid_root(path: &str) -> Result<(), String> {
    valid_path(path, true)?;
    if !std::path::Path::new(path).is_dir() { return Err("Repository root must be a directory".into()); }
    let metadata = std::path::Path::new(path).join(".git");
    valid_path(metadata.to_str().ok_or("Unsupported repository path")?, true)?;
    if !metadata.is_dir() { return Err("Linked worktree metadata is not supported for these reads".into()); }
    Ok(())
}
