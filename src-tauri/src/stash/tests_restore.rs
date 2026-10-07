use super::ops::{push, restore};
use super::tests::{git_in, read, repo, text};
use super::*;

#[tokio::test]
async fn pop_conflict_keeps_the_stash_and_lists_conflicted_paths() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let dir = repo();
    let path = text(&dir);
    std::fs::write(dir.join("a.txt"), "stashed\n").unwrap();
    let oid = push(&path, Some("conflict"), false)
        .await
        .unwrap()
        .stashed
        .unwrap();
    std::fs::write(dir.join("a.txt"), "committed elsewhere\n").unwrap();
    git_in(&dir, &["commit", "-qam", "diverge"]);

    let outcome = restore(&path, &oid, Restore::Pop).await.unwrap();
    assert!(!outcome.applied && outcome.stash_kept);
    assert_eq!(outcome.conflicted, vec!["a.txt".to_string()]);
    assert!(outcome.error.is_some());
    assert_eq!(stash_list(path.clone()).await.unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn overlapping_local_changes_refuse_apply_without_losing_the_stash() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let dir = repo();
    let path = text(&dir);
    std::fs::write(dir.join("a.txt"), "stashed\n").unwrap();
    let oid = push(&path, None, false).await.unwrap().stashed.unwrap();
    std::fs::write(dir.join("a.txt"), "dirty\n").unwrap();
    let outcome = restore(&path, &oid, Restore::Pop).await.unwrap();
    assert!(!outcome.applied && outcome.stash_kept && outcome.conflicted.is_empty());
    assert!(outcome.error.is_some());
    assert_eq!(read(&dir, "a.txt"), "dirty\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn staged_changes_come_back_staged() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let dir = repo();
    let path = text(&dir);
    std::fs::write(dir.join("a.txt"), "staged\n").unwrap();
    git_in(&dir, &["add", "a.txt"]);
    let oid = push(&path, None, false).await.unwrap().stashed.unwrap();
    let outcome = restore(&path, &oid, Restore::Pop).await.unwrap();
    assert!(outcome.applied && outcome.index_restored);
    let staged = std::process::Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["diff", "--cached", "--name-only"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&staged.stdout).trim(), "a.txt");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn pop_without_index_when_staged_state_cannot_be_restored() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let dir = repo();
    let path = text(&dir);
    let numbers: String = (1..=10).map(|n| format!("{n}\n")).collect();
    std::fs::write(dir.join("a.txt"), &numbers).unwrap();
    git_in(&dir, &["commit", "-qam", "numbers"]);
    git_in(&dir, &["switch", "-q", "-c", "other"]);
    std::fs::write(dir.join("a.txt"), numbers.replacen("7\n", "seven\n", 1)).unwrap();
    git_in(&dir, &["commit", "-qam", "line seven"]);
    git_in(&dir, &["switch", "-q", "main"]);
    std::fs::write(dir.join("a.txt"), numbers.replacen("5\n", "five\n", 1)).unwrap();
    git_in(&dir, &["add", "a.txt"]);
    let oid = push(&path, None, false).await.unwrap().stashed.unwrap();
    git_in(&dir, &["switch", "-q", "other"]);

    let outcome = restore(&path, &oid, Restore::Pop).await.unwrap();
    assert!(outcome.applied && !outcome.index_restored, "{outcome:?}");
    assert!(!outcome.stash_kept);
    let merged = read(&dir, "a.txt");
    assert!(merged.contains("five") && merged.contains("seven"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn apply_is_refused_while_a_merge_is_in_conflict() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let dir = repo();
    let path = text(&dir);
    std::fs::write(dir.join("a.txt"), "stashed\n").unwrap();
    let oid = push(&path, None, false).await.unwrap().stashed.unwrap();
    git_in(&dir, &["switch", "-q", "-c", "side"]);
    std::fs::write(dir.join("a.txt"), "side\n").unwrap();
    git_in(&dir, &["commit", "-qam", "side"]);
    git_in(&dir, &["switch", "-q", "main"]);
    std::fs::write(dir.join("a.txt"), "main\n").unwrap();
    git_in(&dir, &["commit", "-qam", "main"]);
    let merge = std::process::Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["merge", "side"])
        .output()
        .unwrap();
    assert!(!merge.status.success());

    let error = restore(&path, &oid, Restore::Apply).await.unwrap_err();
    assert_eq!(error, "Resolve the current conflicts first");
    assert_eq!(stash_list(path.clone()).await.unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn an_existing_untracked_file_does_not_retry_a_partially_applied_stash_regression() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let dir = repo();
    let path = text(&dir);
    std::fs::write(dir.join("a.txt"), "staged\n").unwrap();
    git_in(&dir, &["add", "a.txt"]);
    std::fs::write(dir.join("untracked.txt"), "stashed untracked\n").unwrap();
    let oid = push(&path, None, true).await.unwrap().stashed.unwrap();
    std::fs::write(dir.join("untracked.txt"), "keep local untracked\n").unwrap();
    let outcome = restore(&path, &oid, Restore::Pop).await.unwrap();
    assert!(!outcome.applied && outcome.stash_kept, "{outcome:?}");
    let attempts: Vec<_> = crate::git::activity_snapshot().into_iter()
        .map(|entry| serde_json::to_value(entry).unwrap())
        .filter(|entry| entry["context"] == format!("Stash pop: {path}"))
        .collect();
    assert_eq!(attempts.len(), 1, "unexpected retry: {attempts:?}");
    assert_eq!(attempts[0]["exitCode"], 128);
    assert!(outcome.index_restored, "unexpected no-index retry: {outcome:?}");
    assert!(outcome.conflicted.is_empty());
    assert!(outcome.error.as_ref().unwrap().contains("untracked"), "{outcome:?}");
    assert_eq!(read(&dir, "untracked.txt"), "keep local untracked\n");
    assert_eq!(read(&dir, "a.txt"), "staged\n");
    let _ = std::fs::remove_dir_all(&dir);
}
