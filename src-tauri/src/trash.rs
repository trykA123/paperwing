use crate::settings::Settings;
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrashOutcome {
    item_id: String,
    path: String,
    /// `trashed`, `missing`, `skipped` or `failed`.
    state: String,
    reason: Option<String>,
}

fn outcome(item_id: &str, path: &Path, state: &str, reason: Option<String>) -> TrashOutcome {
    TrashOutcome { item_id: item_id.into(), path: path.to_string_lossy().into_owned(), state: state.into(), reason }
}

/// Moves each cloned folder of a saved set to the Recycle Bin. Only registered, plain Git repositories
/// that no other set uses are touched; anything else is reported and left alone.
fn trash_folders(settings: &Settings, set_id: &str, recycle: impl Fn(&Path) -> Result<(), String>) -> Result<Vec<TrashOutcome>, String> {
    let (own, others) = crate::compare::set_roots(settings, set_id)?;
    let mut seen = HashSet::new();
    let mut outcomes = Vec::new();
    for (item_id, root) in own {
        if !seen.insert(root.to_string_lossy().to_lowercase()) { continue; }
        if !root.exists() {
            outcomes.push(outcome(&item_id, &root, "missing", None));
        } else if others.contains(&root.to_string_lossy().to_lowercase()) {
            outcomes.push(outcome(&item_id, &root, "skipped", Some("Another set uses this folder".into())));
        } else if let Err(reason) = root.to_str().ok_or("Unsupported folder path".to_string()).and_then(crate::git::valid_root) {
            outcomes.push(outcome(&item_id, &root, "skipped", Some(reason)));
        } else {
            match recycle(&root) {
                Ok(()) => outcomes.push(outcome(&item_id, &root, "trashed", None)),
                Err(reason) => outcomes.push(outcome(&item_id, &root, "failed", Some(reason))),
            }
        }
    }
    Ok(outcomes)
}

#[cfg(windows)]
fn recycle(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW};
    // SHFileOperation wants a double-null-terminated list.
    let mut from: Vec<u16> = path.as_os_str().encode_wide().collect();
    from.extend([0, 0]);
    let mut operation: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
    operation.wFunc = FO_DELETE;
    operation.pFrom = from.as_ptr();
    operation.fFlags = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT) as u16;
    let code = unsafe { SHFileOperationW(&mut operation) };
    let aborted = operation.fAnyOperationsAborted != 0;
    if code != 0 || aborted || path.exists() {
        return Err(format!("Windows could not move the folder to the Recycle Bin (code {code:#x})"));
    }
    Ok(())
}

#[cfg(not(windows))]
fn recycle(_path: &Path) -> Result<(), String> {
    Err("The Recycle Bin is only available on Windows".into())
}

#[tauri::command]
pub async fn trash_set_folders(app: tauri::AppHandle, set_id: String) -> Result<Vec<TrashOutcome>, String> {
    if crate::clone::busy() { return Err("A clone, fetch or pull is running; try again when it finishes".into()); }
    let settings = crate::settings::load_settings(app)?;
    let _exclusive = crate::git::filesystem_gate().write().await;
    tauri::async_runtime::spawn_blocking(move || trash_folders(&settings, &set_id, recycle))
        .await
        .map_err(|_| "Could not move the folders".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn repo(root: &Path, name: &str) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(std::process::Command::new("git").arg("-C").arg(&dir).args(["init", "-q"]).status().unwrap().success());
    }

    fn settings(root: &Path) -> Settings {
        let item = |id: &str, name: &str| serde_json::json!({ "id": id, "name": name, "org": "o", "url": "u", "repoId": "s:1", "ref": { "type": "branch", "name": "main" } });
        Settings {
            sources: vec![],
            workspace: serde_json::json!({
                "root": root.to_string_lossy(), "layout": "flat",
                "sets": [
                    { "id": "one", "name": "One", "items": [item("i1", "alpha"), item("i2", "beta"), item("i3", "gamma"), item("i4", "plain"), item("i5", "alpha")] },
                    { "id": "two", "name": "Two", "items": [item("j1", "beta")] },
                ],
            }),
        }
    }

    #[test]
    fn only_registered_unshared_git_repositories_are_moved() {
        let root = std::env::temp_dir().join(format!("paperwing-trash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for name in ["alpha", "beta"] { repo(&root, name); }
        std::fs::create_dir_all(root.join("plain")).unwrap();
        let moved = RefCell::new(Vec::new());
        let outcomes = trash_folders(&settings(&root), "one", |path| { moved.borrow_mut().push(path.to_path_buf()); Ok(()) }).unwrap();
        let state = |id: &str| outcomes.iter().find(|item| item.item_id == id).map(|item| item.state.as_str());
        assert_eq!(state("i1"), Some("trashed"));
        assert_eq!(state("i2"), Some("skipped"), "shared with another set");
        assert_eq!(state("i3"), Some("missing"));
        assert_eq!(state("i4"), Some("skipped"), "not a git repository");
        assert_eq!(state("i5"), None, "a duplicate row is handled once");
        assert_eq!(moved.borrow().as_slice(), [root.join("alpha")]);
        assert!(trash_folders(&settings(&root), "unknown", |_| Ok(())).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "puts one temporary folder in the Recycle Bin"]
    fn recycle_bin_removes_a_folder() {
        let dir = std::env::temp_dir().join(format!("paperwing-recycle-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::write(dir.join("nested").join("file.txt"), "x").unwrap();
        recycle(&dir).unwrap();
        assert!(!dir.exists());
    }
}
