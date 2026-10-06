use super::ops::{drop_stash, push, restore, show};
use super::*;
use std::path::{Path, PathBuf};

pub(super) fn git_in(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub(super) fn repo() -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("paperwing-stash-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    git_in(&dir, &["init", "-q", "-b", "main"]);
    git_in(&dir, &["config", "user.name", "Test User"]);
    git_in(&dir, &["config", "user.email", "test@example.test"]);
    git_in(&dir, &["config", "commit.gpgsign", "false"]);
    std::fs::write(dir.join("a.txt"), "one\n").unwrap();
    git_in(&dir, &["add", "a.txt"]);
    git_in(&dir, &["commit", "-q", "-m", "initial"]);
    dir
}

pub(super) fn text(dir: &Path) -> String {
    dir.to_str().unwrap().to_string()
}

pub(super) fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).unwrap()
}

#[test]
fn list_parsing_reads_branch_message_and_time() {
    let entries = parse::parse_list("stash@{0}\taaa\t1700000000\tOn main: my note\nstash@{1}\tbbb\t1600000000\tWIP on dev: 1234567 subject\nbad line\n");
    assert_eq!(entries.len(), 2);
    assert_eq!(
        (
            entries[0].branch.as_deref(),
            entries[0].message.as_str(),
            entries[0].created_at
        ),
        (Some("main"), "my note", 1700000000)
    );
    assert_eq!(
        (entries[1].branch.as_deref(), entries[1].message.as_str()),
        (Some("dev"), "WIP on dev: 1234567 subject")
    );
    assert_eq!(entries[1].reference, "stash@{1}");
}

#[tokio::test]
async fn push_list_show_apply_pop_and_drop() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let dir = repo();
    let path = text(&dir);
    std::fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
    std::fs::write(dir.join("new.txt"), "fresh\n").unwrap();

    let first = push(&path, Some("first"), false)
        .await
        .unwrap()
        .stashed
        .unwrap();
    assert_eq!(read(&dir, "a.txt"), "one\n");
    assert!(dir.join("new.txt").exists());

    let listed = stash_list(path.clone()).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        (
            listed[0].oid.as_str(),
            listed[0].message.as_str(),
            listed[0].branch.as_deref()
        ),
        (first.as_str(), "first", Some("main"))
    );
    assert!(listed[0].created_at > 1_600_000_000);

    let diff = show(&path, &first).await.unwrap();
    assert!(diff.patch.contains("+two") && !diff.has_untracked && !diff.truncated);

    let applied = restore(&path, &first, Restore::Apply).await.unwrap();
    assert_eq!((applied.applied, applied.stash_kept), (true, true));
    assert_eq!(read(&dir, "a.txt"), "one\ntwo\n");
    git_in(&dir, &["checkout", "-q", "--", "a.txt"]);

    let popped = restore(&path, &first, Restore::Pop).await.unwrap();
    assert_eq!((popped.applied, popped.stash_kept), (true, false));
    assert!(stash_list(path.clone()).await.unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn untracked_files_are_stashed_shown_and_restored() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let dir = repo();
    let path = text(&dir);
    std::fs::write(dir.join("new.txt"), "fresh\n").unwrap();
    assert!(push(&path, None, false).await.unwrap().nothing_to_stash);
    let oid = push(&path, Some("with new"), true)
        .await
        .unwrap()
        .stashed
        .unwrap();
    assert!(!dir.join("new.txt").exists());
    let diff = show(&path, &oid).await.unwrap();
    assert!(
        diff.has_untracked && diff.patch.contains("+fresh"),
        "{}",
        diff.patch
    );
    assert!(restore(&path, &oid, Restore::Pop).await.unwrap().applied);
    assert_eq!(read(&dir, "new.txt"), "fresh\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn clean_repository_reports_nothing_to_stash() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let dir = repo();
    let outcome = stash_push(text(&dir), Some("note".into()), true)
        .await
        .unwrap();
    assert_eq!(
        outcome,
        PushOutcome {
            stashed: None,
            nothing_to_stash: true
        }
    );
    assert!(stash_list(text(&dir)).await.unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn stale_or_malformed_ids_are_refused() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let dir = repo();
    let path = text(&dir);
    std::fs::write(dir.join("a.txt"), "changed\n").unwrap();
    let old = push(&path, Some("old"), false)
        .await
        .unwrap()
        .stashed
        .unwrap();
    std::fs::write(dir.join("a.txt"), "newer\n").unwrap();
    let newer = push(&path, Some("newer"), false)
        .await
        .unwrap()
        .stashed
        .unwrap();
    drop_stash(&path, &newer).await.unwrap();
    assert!(drop_stash(&path, &newer).await.is_err());
    assert!(restore(&path, &"0".repeat(40), Restore::Pop).await.is_err());
    assert!(show(&path, "stash@{0}").await.is_err());
    assert!(drop_stash(&path, "--all").await.is_err());
    assert_eq!(stash_list(path.clone()).await.unwrap()[0].oid, old);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn concurrent_drops_of_one_stash_never_hit_its_neighbour() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let dir = repo();
    let path = text(&dir);
    std::fs::write(dir.join("a.txt"), "x\n").unwrap();
    let x = push(&path, Some("x"), false)
        .await
        .unwrap()
        .stashed
        .unwrap();
    std::fs::write(dir.join("a.txt"), "y\n").unwrap();
    let y = push(&path, Some("y"), false)
        .await
        .unwrap()
        .stashed
        .unwrap();
    let (first, second) = tokio::join!(drop_stash(&path, &x), drop_stash(&path, &x));
    assert_eq!(
        [first.is_ok(), second.is_ok()]
            .iter()
            .filter(|ok| **ok)
            .count(),
        1
    );
    let left = stash_list(path.clone()).await.unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].oid, y);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn oversized_stash_falls_back_to_a_summary() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let dir = repo();
    let path = text(&dir);
    std::fs::write(dir.join("big.txt"), "line of filler text\n".repeat(500_000)).unwrap();
    std::fs::write(dir.join("extra.txt"), "x\n").unwrap();
    let oid = push(&path, None, true).await.unwrap().stashed.unwrap();
    let diff = show(&path, &oid).await.unwrap();
    assert!(diff.truncated && diff.has_untracked);
    assert_eq!(diff.notice.as_deref(), Some("Stash too large to preview"));
    assert!(diff.patch.contains("big.txt") && diff.patch.contains("extra.txt"));
    assert!(diff.patch.len() < 10_000);
    let _ = std::fs::remove_dir_all(&dir);
}
