use std::path::{Path, PathBuf};

#[derive(Clone, PartialEq)]
pub(super) struct Repository {
    pub(super) directory: PathBuf,
    identity: [String; 3],
}

pub(super) fn resolve(root: &Path) -> Result<Repository, String> {
    let marker = root.join(".git");
    let metadata =
        std::fs::symlink_metadata(&marker).map_err(|_| "Batch metadata is unavailable")?;
    let directory = if metadata.is_dir() {
        marker
    } else if metadata.is_file() && metadata.len() <= 4096 {
        let text = std::fs::read_to_string(&marker).map_err(|_| "Invalid Git directory marker")?;
        let path = text
            .strip_prefix("gitdir: ")
            .ok_or("Invalid Git directory marker")?
            .trim();
        root.join(path)
    } else {
        return Err("Unsupported Git metadata".into());
    };
    let directory = directory
        .canonicalize()
        .map_err(|_| "Git directory is unavailable")?;
    let marker = directory.join("commondir");
    if std::fs::symlink_metadata(&marker)
        .is_ok_and(|metadata| !metadata.is_file() || metadata.len() > 4096)
    {
        return Err("Unsupported Git common directory marker".into());
    }
    let common = match std::fs::read_to_string(marker) {
        Ok(path) => directory
            .join(path.trim())
            .canonicalize()
            .map_err(|_| "Git common directory is unavailable")?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => directory.clone(),
        Err(_) => return Err("Git common directory is unavailable".into()),
    };
    Ok(Repository {
        identity: [
            crate::platform::physical_identity(root)?,
            crate::platform::physical_identity(&directory)?,
            crate::platform::physical_identity(&common)?,
        ],
        directory,
    })
}
