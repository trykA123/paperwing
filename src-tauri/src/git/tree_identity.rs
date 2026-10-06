use super::tree_output;
use crate::git::OutputPolicy;
use std::path::PathBuf;

pub(super) async fn read(path: &str) -> Result<String, String> {
    let directories = tree_output(
        path,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--absolute-git-dir",
            "--git-common-dir",
        ],
        &[0],
        OutputPolicy::Metadata,
    )
    .await?;
    let text =
        String::from_utf8(directories.stdout).map_err(|_| "Unsupported Git metadata path")?;
    let mut directories = text.lines();
    let git = PathBuf::from(directories.next().ok_or("Git directory is unavailable")?);
    let common = PathBuf::from(
        directories
            .next()
            .ok_or("Git common directory is unavailable")?,
    );
    let root = PathBuf::from(path);
    tauri::async_runtime::spawn_blocking(move || {
        let identities = [root.clone(), root.join(".git"), git, common]
            .iter()
            .map(|path| crate::platform::physical_identity(path))
            .collect::<Result<Vec<_>, _>>()?;
        serde_json::to_string(&("tree-v2", identities)).map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "Repository identity task failed".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(path: &std::path::Path, args: &[&str]) {
        let result = std::process::Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    #[tokio::test]
    async fn root_and_git_directory_replacements_change_identity() {
        let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
        let fixture = crate::platform::Fixture::new("tree-identity");
        let root = fixture.0.join("repo");
        std::fs::create_dir(&root).unwrap();
        git(&root, &["init", "-q"]);
        let first = read(root.to_str().unwrap()).await.unwrap();
        std::fs::rename(root.join(".git"), root.join("saved-git")).unwrap();
        git(&root, &["init", "-q"]);
        let changed_git = read(root.to_str().unwrap()).await.unwrap();
        assert_ne!(first, changed_git);
        std::fs::rename(&root, fixture.0.join("saved-root")).unwrap();
        std::fs::create_dir(&root).unwrap();
        git(&root, &["init", "-q"]);
        assert_ne!(changed_git, read(root.to_str().unwrap()).await.unwrap());
    }
}
