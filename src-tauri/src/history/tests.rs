use super::*;
use std::path::{Path, PathBuf};

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "skein-history-{name}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Fixture { root }
    }

    fn dir(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn path(&self, name: &str) -> String {
        self.dir(name).to_str().unwrap().to_string()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn git_in(dir: &Path, args: &[&str]) -> String {
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
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn init(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    git_in(dir, &["init", "-q", "-b", "main"]);
    for (key, value) in [
        ("user.name", "Test User"),
        ("user.email", "test@example.test"),
        ("commit.gpgsign", "false"),
        ("core.autocrlf", "false"),
    ] {
        git_in(dir, &["config", key, value]);
    }
}

fn commit(dir: &Path, name: &str) {
    std::fs::write(dir.join(format!("{name}.txt")), name).unwrap();
    git_in(dir, &["add", "."]);
    git_in(dir, &["commit", "-q", "-m", &format!("add {name}")]);
}

fn subjects(commits: &[HistoryCommit]) -> Vec<&str> {
    commits
        .iter()
        .map(|commit| commit.subject.as_str())
        .collect()
}

/// A bare remote plus two clones; `work` is the one under test, `other` publishes behind its back.
fn shared(fixture: &Fixture) -> (PathBuf, PathBuf) {
    let remote = fixture.dir("remote.git");
    std::fs::create_dir_all(&remote).unwrap();
    git_in(&remote, &["init", "-q", "--bare", "-b", "main"]);
    let work = fixture.dir("work");
    init(&work);
    git_in(
        &work,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    commit(&work, "one");
    commit(&work, "two");
    git_in(&work, &["push", "-q", "-u", "origin", "main"]);
    let other = fixture.dir("other");
    git_in(
        &fixture.root,
        &[
            "clone",
            "-q",
            remote.to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    for (key, value) in [
        ("user.name", "Other User"),
        ("user.email", "other@example.test"),
        ("commit.gpgsign", "false"),
    ] {
        git_in(&other, &["config", key, value]);
    }
    (work, other)
}

fn publish(other: &Path, work: &Path, names: &[&str]) {
    for name in names {
        commit(other, name);
    }
    git_in(other, &["push", "-q", "origin", "main"]);
    git_in(work, &["fetch", "-q", "origin"]);
}

#[tokio::test]
async fn in_sync_branch_has_a_base_and_empty_rails() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("sync");
    let (work, _) = shared(&fixture);
    let history = read_history(work.to_str().unwrap(), None).await.unwrap();
    assert_eq!(history.kind, HistoryKind::Tracking);
    assert_eq!(history.upstream.as_deref(), Some("origin/main"));
    assert!(history.local.is_empty() && history.origin.is_empty());
    assert_eq!(
        history.base.as_ref().map(|base| base.subject.as_str()),
        Some("add two")
    );
    assert_eq!(subjects(&history.below), ["add one"]);
    assert_eq!(history.uncommitted, 0);
}

#[tokio::test]
async fn ahead_lists_only_local_commits_newest_first() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("ahead");
    let (work, _) = shared(&fixture);
    commit(&work, "three");
    commit(&work, "four");
    let history = read_history(work.to_str().unwrap(), None).await.unwrap();
    assert_eq!(subjects(&history.local), ["add four", "add three"]);
    assert_eq!((history.local_total, history.origin_total), (2, 0));
    assert!(history.origin.is_empty());
    assert_eq!(history.base.unwrap().subject, "add two");
    assert_eq!(history.local[0].author, "Test User");
    assert_eq!(history.local[0].short.len(), 8);
    assert!(history.local[0].date.contains('T'));
}

#[tokio::test]
async fn behind_lists_only_origin_commits_after_a_fetch() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("behind");
    let (work, other) = shared(&fixture);
    publish(&other, &work, &["remote-a", "remote-b"]);
    let history = read_history(work.to_str().unwrap(), None).await.unwrap();
    assert_eq!(subjects(&history.origin), ["add remote-b", "add remote-a"]);
    assert_eq!(history.origin[0].author, "Other User");
    assert!(history.local.is_empty());
    assert_eq!(history.base.unwrap().subject, "add two");
}

#[tokio::test]
async fn diverged_has_both_rails_and_the_split_point_as_base() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("diverged");
    let (work, other) = shared(&fixture);
    commit(&work, "mine");
    publish(&other, &work, &["theirs"]);
    let history = read_history(work.to_str().unwrap(), None).await.unwrap();
    assert_eq!(subjects(&history.local), ["add mine"]);
    assert_eq!(subjects(&history.origin), ["add theirs"]);
    assert_eq!(history.base.as_ref().unwrap().subject, "add two");
}

#[tokio::test]
async fn uncommitted_counts_modified_and_untracked_files() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("dirty");
    let (work, _) = shared(&fixture);
    std::fs::write(work.join("one.txt"), "changed").unwrap();
    std::fs::write(work.join("new.txt"), "new").unwrap();
    let history = read_history(work.to_str().unwrap(), None).await.unwrap();
    assert_eq!(history.uncommitted, 2);
}

#[tokio::test]
async fn limit_caps_each_rail_but_totals_stay_exact() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("limit");
    let (work, other) = shared(&fixture);
    for name in ["l1", "l2", "l3"] {
        commit(&work, name);
    }
    publish(&other, &work, &["r1", "r2", "r3"]);
    let history = read_history(work.to_str().unwrap(), Some(2)).await.unwrap();
    assert_eq!((history.local.len(), history.origin.len()), (2, 2));
    assert_eq!((history.local_total, history.origin_total), (3, 3));
    let clamped = read_history(work.to_str().unwrap(), Some(0)).await.unwrap();
    assert_eq!(clamped.local.len(), 1);
}

#[tokio::test]
async fn no_upstream_shows_a_local_only_rail() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("local");
    let work = fixture.dir("work");
    init(&work);
    commit(&work, "one");
    commit(&work, "two");
    let history = read_history(work.to_str().unwrap(), None).await.unwrap();
    assert_eq!(history.kind, HistoryKind::NoUpstream);
    assert_eq!(history.branch.as_deref(), Some("main"));
    assert!(history.upstream.is_none() && history.base.is_none() && history.origin.is_empty());
    assert_eq!(subjects(&history.local), ["add two", "add one"]);
}

#[tokio::test]
async fn detached_head_is_local_only_without_a_branch() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("detached");
    let work = fixture.dir("work");
    init(&work);
    commit(&work, "one");
    commit(&work, "two");
    git_in(&work, &["checkout", "-q", "--detach", "HEAD~1"]);
    let history = read_history(work.to_str().unwrap(), None).await.unwrap();
    assert_eq!(history.kind, HistoryKind::Detached);
    assert!(history.branch.is_none() && history.upstream.is_none());
    assert_eq!(subjects(&history.local), ["add one"]);
}

#[tokio::test]
async fn unborn_branch_has_no_commits_but_counts_files() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("unborn");
    let work = fixture.dir("work");
    init(&work);
    std::fs::write(work.join("draft.txt"), "draft").unwrap();
    let history = read_history(work.to_str().unwrap(), None).await.unwrap();
    assert_eq!(history.kind, HistoryKind::Unborn);
    assert_eq!(history.branch.as_deref(), Some("main"));
    assert!(history.local.is_empty() && history.base.is_none());
    assert_eq!(history.uncommitted, 1);
}

#[tokio::test]
async fn rejects_paths_that_are_not_repository_roots() {
    let fixture = Fixture::new("invalid");
    std::fs::create_dir_all(fixture.dir("plain")).unwrap();
    assert!(read_history(&fixture.path("plain"), None).await.is_err());
    assert!(read_history("relative/path", None).await.is_err());
}

#[tokio::test]
async fn a_tracked_file_named_head_does_not_confuse_revisions() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("headfile");
    let (work, _) = shared(&fixture);
    commit(&work, "HEAD");
    commit(&work, "after");
    let history = read_history(work.to_str().unwrap(), None).await.unwrap();
    assert_eq!(subjects(&history.local), ["add after", "add HEAD"]);
    assert_eq!(history.local_total, 2);
}

#[tokio::test]
async fn local_only_excludes_commits_already_on_any_remote() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("remotes");
    let (work, _) = shared(&fixture);
    git_in(&work, &["checkout", "-q", "-b", "wip"]);
    commit(&work, "published");
    git_in(&work, &["push", "-q", "origin", "wip"]);
    commit(&work, "private");
    let history = read_history(work.to_str().unwrap(), None).await.unwrap();
    assert_eq!(history.kind, HistoryKind::NoUpstream);
    assert_eq!(subjects(&history.local), ["add private"]);
    assert_eq!(history.local_total, 1);
}

#[tokio::test]
async fn a_deleted_remote_branch_is_reported_as_gone_not_missing() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("gone");
    let (work, _) = shared(&fixture);
    git_in(&work, &["checkout", "-q", "-b", "topic"]);
    commit(&work, "topic-one");
    git_in(&work, &["push", "-q", "-u", "origin", "topic"]);
    git_in(&work, &["push", "-q", "origin", "--delete", "topic"]);
    git_in(&work, &["fetch", "-q", "--prune", "origin"]);
    let history = read_history(work.to_str().unwrap(), None).await.unwrap();
    assert_eq!(history.kind, HistoryKind::UpstreamGone);
    assert_eq!(history.upstream.as_deref(), Some("origin/topic"));
    assert_eq!(subjects(&history.local), ["add topic-one"]);
}

#[tokio::test]
async fn a_local_branch_upstream_drops_the_refs_heads_prefix() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("localup");
    let work = fixture.dir("work");
    init(&work);
    commit(&work, "one");
    git_in(
        &work,
        &["checkout", "-q", "-b", "feature", "--track", "main"],
    );
    commit(&work, "two");
    let history = read_history(work.to_str().unwrap(), None).await.unwrap();
    assert_eq!(history.kind, HistoryKind::Tracking);
    assert_eq!(history.upstream.as_deref(), Some("main"));
    assert_eq!(subjects(&history.local), ["add two"]);
}
