use super::*;
use crate::git::repository_tree;
use std::path::{Path, PathBuf};
use std::process::Command;

fn run(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success());
}

fn repository(name: &str) -> PathBuf {
    let dir = crate::test_support::tmp_root()
        .join("repo-command")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    run(&dir, &["init", "-q", "-b", "main"]);
    run(&dir, &["config", "commit.gpgsign", "false"]);
    run(
        &dir,
        &[
            "-c",
            "user.name=admin",
            "-c",
            "user.email=admin@example.invalid",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "fixture",
        ],
    );
    dir
}

#[tokio::test]
async fn every_wrapper_runs_git_with_the_hardening_flags() {
    let _serial = crate::test_support::serial().await;
    let dir = repository("wrappers");
    let path = dir.to_str().unwrap();
    repository_tree(path.into()).await.unwrap();
    crate::local::local_status(vec![path.into()]).await;
    crate::stash::stash_list(path.into()).await.unwrap();
    crate::history::repository_history(path.into(), Some(5))
        .await
        .unwrap();
    crate::commit::quick(path, &["status", "--porcelain"], "wrapper-commit", &[0])
        .await
        .unwrap();
    for context in [
        format!("Tree: {path}"),
        format!("Status: {path}"),
        format!("Stash list: {path}"),
        format!("History: {path}"),
        "wrapper-commit".to_string(),
    ] {
        assert_hardened(&argv_of(&context), path);
    }
    assert!(argv_of("wrapper-commit").contains(&"--literal-pathspecs".to_string()));
    assert_eq!(
        argv_of(&format!("History: {path}"))[0],
        "--no-optional-locks"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn local_status_does_not_run_a_repository_fsmonitor_hook() {
    let _serial = crate::test_support::serial().await;
    let dir = repository("fsmonitor");
    let hook = dir.parent().unwrap().join("fsmonitor-hook.sh");
    let marker = dir.parent().unwrap().join("fsmonitor-ran");
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\necho ran >> {}\nprintf '\\0'\n",
            marker.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    run(&dir, &["config", "core.fsmonitor", hook.to_str().unwrap()]);
    let _ = std::fs::remove_file(&marker);
    run(&dir, &["status", "--porcelain"]);
    assert!(
        marker.exists(),
        "fixture hook must fire for plain git status"
    );
    std::fs::remove_file(&marker).unwrap();
    let status = crate::local::local_status(vec![dir.to_str().unwrap().into()]).await;
    assert_eq!(serde_json::to_value(status).unwrap()[0]["repo"], true);
    assert!(
        !marker.exists(),
        "local_status ran the repository fsmonitor hook"
    );
}
