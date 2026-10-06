use super::ops::restore;
use super::tests::{git_in, read, repo, text};
use super::*;
use std::path::{Path, PathBuf};

fn remote_with() -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let bare = std::env::temp_dir().join(format!(
        "paperwing-stash-remote-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&bare).unwrap();
    git_in(&bare, &["init", "-q", "--bare", "-b", "main"]);
    bare
}

fn publish(dir: &Path, remote: &Path, name: &str, branch: &str) {
    git_in(dir, &["remote", "add", name, remote.to_str().unwrap()]);
    git_in(dir, &["push", "-q", name, &format!("main:{branch}")]);
    git_in(dir, &["fetch", "-q", name]);
}

async fn stash_count(path: &str) -> usize {
    stash_list(path.to_string()).await.unwrap().len()
}

#[tokio::test]
async fn dirty_switch_stashes_tracked_and_untracked_with_a_named_message() {
    let dir = repo();
    let path = text(&dir);
    git_in(&dir, &["branch", "other"]);
    std::fs::write(dir.join("a.txt"), "one\nlocal\n").unwrap();
    std::fs::write(dir.join("loose.txt"), "loose\n").unwrap();

    let outcome = switch_with_stash(path.clone(), "other".into())
        .await
        .unwrap();
    assert!(outcome.switched && outcome.error.is_none());
    assert_eq!(read(&dir, "a.txt"), "one\n");
    assert!(!dir.join("loose.txt").exists());
    let entries = stash_list(path.clone()).await.unwrap();
    assert_eq!(entries[0].message, "Skein: before switching to other");

    let popped = restore(&path, &outcome.stashed.unwrap(), Restore::Pop)
        .await
        .unwrap();
    assert!(popped.applied);
    assert_eq!(read(&dir, "a.txt"), "one\nlocal\n");
    assert_eq!(read(&dir, "loose.txt"), "loose\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn clean_switch_creates_no_stash_and_bad_targets_are_refused() {
    let dir = repo();
    let path = text(&dir);
    git_in(&dir, &["branch", "other"]);
    let clean = switch_with_stash(path.clone(), "other".into())
        .await
        .unwrap();
    assert_eq!(
        clean,
        SwitchOutcome {
            stashed: None,
            switched: true,
            error: None
        }
    );
    assert!(switch_with_stash(path.clone(), "missing".into())
        .await
        .is_err());
    assert!(switch_with_stash(path.clone(), "-x".into()).await.is_err());
    assert!(switch_with_stash(path.clone(), "HEAD".into())
        .await
        .is_err());
    assert_eq!(stash_count(&path).await, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn remote_only_branch_is_tracked_after_stashing() {
    let dir = repo();
    let path = text(&dir);
    let remote = remote_with();
    publish(&dir, &remote, "origin", "feature");
    std::fs::write(dir.join("a.txt"), "dirty\n").unwrap();

    let outcome = switch_with_stash(path.clone(), "feature".into())
        .await
        .unwrap();
    assert!(outcome.switched && outcome.stashed.is_some(), "{outcome:?}");
    let head = std::process::Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["rev-parse", "--abbrev-ref", "feature@{upstream}"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&head.stdout).trim(),
        "origin/feature"
    );
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&remote);
}

#[tokio::test]
async fn ambiguous_remote_branch_is_refused_before_stashing() {
    let dir = repo();
    let path = text(&dir);
    let (first, second) = (remote_with(), remote_with());
    publish(&dir, &first, "origin", "feature");
    publish(&dir, &second, "backup", "feature");
    std::fs::write(dir.join("a.txt"), "dirty\n").unwrap();

    let error = switch_with_stash(path.clone(), "feature".into())
        .await
        .unwrap_err();
    assert!(error.contains("Several remotes"), "{error}");
    assert_eq!(stash_count(&path).await, 0);
    assert_eq!(read(&dir, "a.txt"), "dirty\n");
    for remote in [&dir, &first, &second] {
        let _ = std::fs::remove_dir_all(remote);
    }
}
