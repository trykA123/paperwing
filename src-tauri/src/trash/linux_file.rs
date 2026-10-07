use super::linux::Trash;
use crate::linux_guard::{
    folders::{Directory, FileMove},
    Root,
};
use std::path::Path;

pub(super) fn recycle_file(
    root: &Root,
    file: &str,
    bytes: &[u8],
    data: &Path,
) -> Result<(), String> {
    crate::paths::relative(file)?;
    let parent = root
        .parent(file, false)
        .map_err(|error| error.to_string())?;
    let expected = parent.snapshot().map_err(|error| error.to_string())?;
    if expected.bytes() != Some(bytes) {
        return Err("File changed since the diff was read; file retained".into());
    }
    let path = root
        .value()
        .map_err(|error| error.to_string())?
        .path
        .join(file);
    let source = Directory::open(path.parent().ok_or("Missing trash source parent")?)
        .map_err(|error| error.to_string())?;
    let leaf = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Unsupported trash filename")?;
    let trash = Trash::for_source(&source, data)?;
    trash.recycle_entry(&source, root, &path, |target, name, renamed| {
        source.move_file_to_tracked(
            FileMove {
                leaf,
                target,
                name,
                parent: &parent,
                expected: &expected,
            },
            renamed,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untracked_crlf_file_moves_with_trash_info_and_exact_bytes() {
        let fixture = crate::linux_guard::folders::Fixture::new("discard-trash");
        let repo = fixture.repository("repo");
        let root = Root::open(&repo, &[repo.join(".git")]).unwrap();
        let bytes = b"discard\r\nlast";
        std::fs::write(repo.join("with spaces.txt"), bytes).unwrap();
        let data = fixture.0.join("data");
        recycle_file(&root, "with spaces.txt", bytes, &data).unwrap();
        assert!(!repo.join("with spaces.txt").exists());
        let moved = std::fs::read_dir(data.join("Trash/files"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(std::fs::read(moved.path()).unwrap(), bytes);
        let info = std::fs::read_dir(data.join("Trash/info"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        assert!(std::fs::read_to_string(info.path())
            .unwrap()
            .contains("with%20spaces.txt"));
    }

    #[test]
    fn unavailable_trash_and_stale_bytes_retain_the_file() {
        let fixture = crate::linux_guard::folders::Fixture::new("discard-trash-refuse");
        let repo = fixture.repository("repo");
        let root = Root::open(&repo, &[repo.join(".git")]).unwrap();
        std::fs::write(repo.join("file.txt"), b"current").unwrap();
        let data = fixture.0.join("not-a-directory");
        std::fs::write(&data, b"blocked").unwrap();
        assert!(recycle_file(&root, "file.txt", b"stale", &data)
            .unwrap_err()
            .contains("changed"));
        assert!(recycle_file(&root, "file.txt", b"current", &data).is_err());
        assert_eq!(std::fs::read(repo.join("file.txt")).unwrap(), b"current");
    }

    #[tokio::test]
    async fn intent_to_add_discard_moves_exact_file_bytes_to_desktop_trash() {
        let _serial = crate::test_support::serial().await;
        let fixture = crate::commit::test_fixture::Fixture::new();
        fixture.commit("base.txt", b"base\n");
        let bytes = b"keep these bytes\nlast";
        fixture.write("file.txt", bytes);
        fixture.git(&["add", "-N", "file.txt"]);
        let diff =
            crate::commit::change_hunks(fixture.path(), "file.txt".into(), None, "unstaged".into())
                .await
                .unwrap();
        let plan = crate::commit::prepare_file(
            &fixture.path(),
            &crate::commit::DiscardFile {
                file: "file.txt".into(),
                orig_path: None,
                content_hash: diff.content_hash,
            },
        )
        .await
        .unwrap();
        let crate::commit::DiscardPlan::Trash(write) = plan else {
            panic!("Expected desktop Trash");
        };

        let repo_name = fixture.root.file_name().unwrap().to_str().unwrap();
        let config_dir = fixture.base.join("config");
        std::fs::create_dir_all(&config_dir).unwrap();
        let settings_json = serde_json::json!({
            "sources": [],
            "workspace": {
                "root": fixture.base,
                "layout": "flat",
                "sets": [{
                    "id": "set1",
                    "name": "Set 1",
                    "items": [{
                        "id": "repo1",
                        "name": repo_name
                    }]
                }]
            }
        });
        std::fs::write(
            config_dir.join("settings.json"),
            serde_json::to_vec(&settings_json).unwrap(),
        )
        .unwrap();

        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().app.app_directories_override =
            Some(serde_json::from_value(serde_json::json!({ "config": config_dir })).unwrap());
        let trash_data = fixture.base.join("trash-data");
        std::fs::create_dir_all(&trash_data).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&trash_data, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        std::env::set_var("XDG_DATA_HOME", &trash_data);
        struct ResetEnv;
        impl Drop for ResetEnv {
            fn drop(&mut self) {
                std::env::remove_var("XDG_DATA_HOME");
            }
        }
        let _reset = ResetEnv;

        let app = tauri::test::mock_builder().build(context).unwrap();

        // 20s test timeout wrapper ensuring no deadlock under the gate
        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            crate::trash::untracked::recycle(app.handle().clone(), write),
        )
        .await
        .expect("Operation deadlocked while holding filesystem_gate")
        .unwrap();

        assert!(outcome.contains("Moved to the desktop Trash"));
        assert!(!fixture.root.join("file.txt").exists());
        assert_eq!(fixture.git(&["status", "--porcelain"]), b"");
    }

    #[tokio::test]
    async fn intent_to_add_rm_cached_failure_reports_file_in_trash_for_restore() {
        let _serial = crate::test_support::serial().await;
        let fixture = crate::commit::test_fixture::Fixture::new();
        fixture.commit("base.txt", b"base\n");
        let bytes = b"keep these bytes\nlast";
        fixture.write("file.txt", bytes);
        fixture.git(&["add", "-N", "file.txt"]);
        let diff =
            crate::commit::change_hunks(fixture.path(), "file.txt".into(), None, "unstaged".into())
                .await
                .unwrap();
        let plan = crate::commit::prepare_file(
            &fixture.path(),
            &crate::commit::DiscardFile {
                file: "file.txt".into(),
                orig_path: None,
                content_hash: diff.content_hash,
            },
        )
        .await
        .unwrap();
        let crate::commit::DiscardPlan::Trash(write) = plan else {
            panic!("Expected desktop Trash");
        };

        let repo_name = fixture.root.file_name().unwrap().to_str().unwrap();
        let config_dir = fixture.base.join("config");
        std::fs::create_dir_all(&config_dir).unwrap();
        let settings_json = serde_json::json!({
            "sources": [],
            "workspace": {
                "root": fixture.base,
                "layout": "flat",
                "sets": [{
                    "id": "set1",
                    "name": "Set 1",
                    "items": [{
                        "id": "repo1",
                        "name": repo_name
                    }]
                }]
            }
        });
        std::fs::write(
            config_dir.join("settings.json"),
            serde_json::to_vec(&settings_json).unwrap(),
        )
        .unwrap();

        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().app.app_directories_override =
            Some(serde_json::from_value(serde_json::json!({ "config": config_dir })).unwrap());
        let trash_data = fixture.base.join("trash-data");
        std::fs::create_dir_all(&trash_data).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&trash_data, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        std::env::set_var("XDG_DATA_HOME", &trash_data);
        struct ResetEnv;
        impl Drop for ResetEnv {
            fn drop(&mut self) {
                std::env::remove_var("XDG_DATA_HOME");
            }
        }
        let _reset = ResetEnv;

        let app = tauri::test::mock_builder().build(context).unwrap();

        let lock_path = fixture.root.join(".git/index.lock");
        std::fs::write(&lock_path, b"").unwrap();

        let err = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            crate::trash::untracked::recycle(app.handle().clone(), write),
        )
        .await
        .expect("Operation deadlocked")
        .unwrap_err();

        assert!(
            err.contains("file.txt"),
            "Error should name the file: {err}"
        );
        assert!(
            err.contains("Trash"),
            "Error should say file is in the Trash: {err}"
        );
        assert!(
            err.to_lowercase().contains("restore"),
            "Error should tell user they can restore it: {err}"
        );
        assert!(
            !fixture.root.join("file.txt").exists(),
            "File was moved to trash"
        );
    }
}
