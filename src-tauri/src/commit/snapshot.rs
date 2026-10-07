use super::{patch, run};
use crate::git::{valid_root, OutputPolicy};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::time::Duration;

pub(crate) struct IndexState {
    path: std::path::PathBuf,
    hash: Option<String>,
}

impl IndexState {
    fn current(&self) -> Result<Option<String>, String> {
        match std::fs::read(&self.path) {
            Ok(bytes) => Ok(Some(format!("{:x}", Sha256::digest(bytes)))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("Could not verify the index: {error}")),
        }
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.current()? != self.hash {
            return Err("Index changed since the diff was read; refresh before discarding".into());
        }
        Ok(())
    }
}

async fn read_index(path: &str) -> Result<IndexState, String> {
    let output = super::quick(
        path,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        "Locate partial staging index",
        &[0],
    )
    .await?;
    let bytes = output.stdout.strip_suffix(b"\n").unwrap_or(&output.stdout);
    let name = std::str::from_utf8(bytes).map_err(|_| "Unsupported index path encoding")?;
    let index = IndexState {
        path: name.into(),
        hash: None,
    };
    tauri::async_runtime::spawn_blocking(move || {
        let hash = index.current()?;
        Ok(IndexState { hash, ..index })
    })
    .await
    .map_err(|_| "Could not verify the index")?
}

pub(crate) struct Snapshot {
    pub before: Option<Vec<u8>>,
    pub after: Option<Vec<u8>>,
    pub working: Option<Vec<u8>>,
    pub index: Option<Vec<u8>>,
    pub hash: String,
    pub mode: String,
    pub old_path: String,
    pub index_state: IndexState,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeHunks {
    pub content_hash: String,
    pub binary: bool,
    pub hunks: Vec<patch::Hunk>,
}

pub(crate) async fn read(
    path: &str,
    file: &str,
    original: Option<&str>,
    area: &str,
) -> Result<Snapshot, String> {
    valid_root(path)?;
    crate::paths::relative(file)?;
    if !matches!(area, "staged" | "unstaged" | "untracked") {
        return Err("Unknown diff area".into());
    }
    if let Some(original) = original {
        crate::paths::relative(original)?;
        let changes = super::repo_changes(path.into()).await?;
        if !changes.files.iter().any(|entry| {
            entry.path == file
                && entry.orig_path.as_deref() == Some(original)
                && entry.kind == "renamed"
        }) {
            return Err("Rename changed; refresh the diff".into());
        }
    }
    let index_state = read_index(path).await?;
    let stage = run(
        path,
        &["ls-files", "--stage", "-z", "--", file],
        "Read partial staging index",
        &[0],
        OutputPolicy::Metadata,
        None,
        Duration::from_secs(45),
    )
    .await?;
    let mut mode = "100644".to_string();
    for entry in stage
        .stdout
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let metadata = entry
            .split(|byte| *byte == b'\t')
            .next()
            .ok_or("Invalid index entry")?;
        let text = std::str::from_utf8(metadata).map_err(|_| "Invalid index metadata")?;
        let fields: Vec<_> = text.split(' ').collect();
        if fields.len() != 3 || fields[2] != "0" || !matches!(fields[0], "100644" | "100755") {
            return Err("Unmerged, linked or submodule files do not support this action".into());
        }
        mode = fields[0].into();
    }
    let index = super::blob(path, &format!(":0:{file}")).await?;
    let head_path = original.unwrap_or(file);
    let head = super::blob(path, &format!("HEAD:{head_path}")).await?;
    if head.is_some() {
        let tree = run(
            path,
            &["ls-tree", "-z", "HEAD", "--", head_path],
            "Read partial staging base mode",
            &[0],
            OutputPolicy::Metadata,
            None,
            Duration::from_secs(45),
        )
        .await?;
        let head_mode = tree
            .stdout
            .split(|byte| *byte == b' ')
            .next()
            .ok_or("Missing base file mode")?;
        if !matches!(head_mode, b"100644" | b"100755") {
            return Err("Linked or submodule files do not support this action".into());
        }
        if index.is_none() {
            mode = std::str::from_utf8(head_mode)
                .map_err(|_| "Invalid base file mode")?
                .into();
        }
    }
    let root = path.to_string();
    let relative = file.to_string();
    let working = tauri::async_runtime::spawn_blocking(move || {
        let value = crate::paths::ReadRoot::new(
            std::path::Path::new(&root),
            vec![std::path::Path::new(&root).join(".git")],
        )?
        .read(&relative)?;
        if value.as_ref().is_some_and(|value| value.symlink) {
            return Err("Linked files do not support hunk or discard actions".to_string());
        }
        Ok(value)
    })
    .await
    .map_err(|_| "Could not read partial staging content")??
    .map(|value| value.bytes);
    if area == "untracked" && index.is_some() {
        return Err("File is now tracked; refresh the diff".into());
    }
    let (before, after, old_path) = if area == "staged" {
        (head.clone(), index.clone(), head_path.to_string())
    } else {
        (index.clone(), working.clone(), file.to_string())
    };
    let mut hash = Sha256::new();
    hash.update(index_state.hash.as_deref().unwrap_or("missing").as_bytes());
    for bytes in [
        Some(path.as_bytes()),
        Some(file.as_bytes()),
        Some(head_path.as_bytes()),
        Some(if area == "staged" {
            b"staged".as_slice()
        } else {
            b"working".as_slice()
        }),
        Some(stage.stdout.as_slice()),
        head.as_deref(),
        index.as_deref(),
        working.as_deref(),
    ] {
        hash.update([u8::from(bytes.is_some())]);
        if let Some(bytes) = bytes {
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
    }
    tauri::async_runtime::spawn_blocking(move || {
        index_state.validate()?;
        Ok(Snapshot {
            before,
            after,
            working,
            index,
            hash: format!("{:x}", hash.finalize()),
            mode,
            old_path,
            index_state,
        })
    })
    .await
    .map_err(|_| "Could not verify the index")?
}

pub(crate) fn require_hash(snapshot: &Snapshot, expected: &str) -> Result<(), String> {
    if expected.len() != 64 || snapshot.hash != expected {
        return Err(
            "File or index changed since the diff was read; refresh before applying".into(),
        );
    }
    Ok(())
}

#[tauri::command]
pub async fn change_hunks(
    path: String,
    file: String,
    orig_path: Option<String>,
    area: String,
) -> Result<ChangeHunks, String> {
    let snapshot = read(&path, &file, orig_path.as_deref(), &area).await?;
    let before = snapshot.before.as_deref().unwrap_or_default();
    let after = snapshot.after.as_deref().unwrap_or_default();
    let binary = patch::binary(before) || patch::binary(after);
    let hunks = if binary {
        Vec::new()
    } else {
        patch::Diff::new(before, after)?.hunks()
    };
    Ok(ChangeHunks {
        content_hash: snapshot.hash,
        binary,
        hunks,
    })
}
