use super::*;
use std::path::{Path, PathBuf};

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

struct Fixture {
    root: PathBuf,
    work: PathBuf,
    bare: PathBuf,
}

impl Fixture {
    fn path(&self) -> String {
        self.work.to_str().unwrap().to_string()
    }

    fn commit(&self, file: &str, message: &str) {
        std::fs::write(self.work.join(file), message).unwrap();
        git_in(&self.work, &["add", file]);
        git_in(&self.work, &["commit", "-q", "-m", message]);
    }

    fn tip(&self, branch: &str) -> String {
        git_in(&self.work, &["rev-parse", &format!("refs/heads/{branch}")])
    }

    fn remote_tip(&self, branch: &str) -> Option<String> {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&self.bare)
            .args([
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ])
            .output()
            .unwrap();
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn fixture() -> Fixture {
    let base = std::env::var_os("PAPERWING_TEST_TMP")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = base.join(format!("paperwing-cleanup-{}-{nonce}", std::process::id()));
    let bare = root.join("remote.git");
    let work = root.join("work");
    std::fs::create_dir_all(&work).unwrap();
    git_in(&root, &["init", "-q", "--bare", "-b", "main", "remote.git"]);
    git_in(&work, &["init", "-q", "-b", "main"]);
    for (key, value) in [
        ("user.name", "Test User"),
        ("user.email", "test@example.test"),
        ("commit.gpgsign", "false"),
    ] {
        git_in(&work, &["config", key, value]);
    }
    git_in(&work, &["remote", "add", "origin", bare.to_str().unwrap()]);
    let fixture = Fixture { root, work, bare };
    fixture.commit("a.txt", "initial");
    git_in(&fixture.work, &["push", "-q", "-u", "origin", "main"]);
    git_in(&fixture.work, &["remote", "set-head", "origin", "main"]);
    fixture
}

fn branch_with_commit(fixture: &Fixture, name: &str, merge: bool) {
    git_in(&fixture.work, &["switch", "-q", "-c", name]);
    fixture.commit(&format!("{name}.txt"), name);
    git_in(&fixture.work, &["switch", "-q", "main"]);
    if merge {
        git_in(
            &fixture.work,
            &[
                "merge",
                "-q",
                "--no-ff",
                "-m",
                &format!("merge {name}"),
                name,
            ],
        );
    }
}

fn find<'a>(list: &'a MergedBranches, name: &str) -> &'a LocalCandidate {
    list.local
        .iter()
        .find(|branch| branch.name == name)
        .unwrap_or_else(|| panic!("{name} missing"))
}

#[tokio::test]
async fn lists_merged_and_unmerged_without_current_or_base() {
    let fixture = fixture();
    branch_with_commit(&fixture, "done", true);
    branch_with_commit(&fixture, "wip", false);
    git_in(&fixture.work, &["switch", "-q", "-c", "here"]);
    git_in(&fixture.work, &["push", "-q", "origin", "main"]);
    let list = merged_branches(fixture.path(), None, None).await.unwrap();
    assert_eq!(
        (list.base.as_str(), list.base_name.as_str()),
        ("origin/main", "main")
    );
    assert_eq!(list.remote.as_deref(), Some("origin"));
    assert_eq!(list.current.as_deref(), Some("here"));
    assert!(find(&list, "done").merged);
    assert!(!find(&list, "wip").merged);
    assert!(list
        .local
        .iter()
        .all(|branch| branch.name != "here" && branch.name != "main"));
    assert_eq!(find(&list, "done").oid, fixture.tip("done"));
    assert_eq!(find(&list, "done").subject, "done");
    assert!(find(&list, "done").last_commit > 0);
}

#[tokio::test]
async fn base_falls_back_to_local_main_and_accepts_override() {
    let fixture = fixture();
    git_in(&fixture.work, &["remote", "remove", "origin"]);
    branch_with_commit(&fixture, "done", true);
    let list = merged_branches(fixture.path(), None, None).await.unwrap();
    assert_eq!((list.base.as_str(), list.remote.clone()), ("main", None));
    assert!(find(&list, "done").merged);
    let custom = merged_branches(fixture.path(), Some("done".into()), None)
        .await
        .unwrap();
    assert_eq!(custom.base_name, "done");
    assert!(merged_branches(fixture.path(), Some("nope".into()), None)
        .await
        .is_err());
    assert!(merged_branches(fixture.path(), Some("-x".into()), None)
        .await
        .is_err());
    assert!(merged_branches(fixture.path(), None, Some("origin".into()))
        .await
        .is_err());
}

#[tokio::test]
async fn gone_upstream_and_remote_listing() {
    let fixture = fixture();
    branch_with_commit(&fixture, "done", true);
    branch_with_commit(&fixture, "other", true);
    git_in(
        &fixture.work,
        &["push", "-q", "-u", "origin", "done", "other", "main"],
    );
    git_in(&fixture.work, &["push", "-q", "origin", "--delete", "done"]);
    git_in(&fixture.work, &["fetch", "-q", "--prune", "origin"]);
    let list = merged_branches(fixture.path(), None, Some("origin".into()))
        .await
        .unwrap();
    let done = find(&list, "done");
    assert_eq!(done.upstream.as_deref(), Some("origin/done"));
    assert!(done.upstream_gone);
    let other = find(&list, "other");
    assert!(!other.upstream_gone);
    let names: Vec<&str> = list
        .remote_branches
        .iter()
        .map(|branch| branch.name.as_str())
        .collect();
    assert_eq!(names, vec!["other"]);
    assert_eq!(list.remote_branches[0].oid, fixture.tip("other"));
}

#[tokio::test]
async fn local_delete_checks_oid_merge_and_protected_branches() {
    let fixture = fixture();
    branch_with_commit(&fixture, "done", true);
    branch_with_commit(&fixture, "wip", false);
    let stale = fixture.tip("done");
    git_in(&fixture.work, &["branch", "stale", &stale]);
    let stale_expected = fixture.tip("stale");
    git_in(&fixture.work, &["switch", "-q", "stale"]);
    fixture.commit("s.txt", "moved");
    git_in(&fixture.work, &["switch", "-q", "main"]);
    git_in(
        &fixture.work,
        &["merge", "-q", "--no-ff", "-m", "merge stale", "stale"],
    );
    let names: Vec<String> = ["done", "wip", "stale", "main", "HEAD", "-D"]
        .iter()
        .map(|name| name.to_string())
        .collect();
    let expected = vec![
        fixture.tip("done"),
        fixture.tip("wip"),
        stale_expected,
        fixture.tip("main"),
        fixture.tip("main"),
        fixture.tip("main"),
    ];
    let results = delete_merged_branches(fixture.path(), names, expected, Some("main".into()))
        .await
        .unwrap();
    assert!(results[0].deleted, "{:?}", results[0].error);
    assert!(results[1].error.as_deref().unwrap().contains("not merged"));
    assert!(results[2].error.as_deref().unwrap().contains("changed"));
    assert!(results[3].error.is_some() && results[4].error.is_some() && results[5].error.is_some());
    let branches = git_in(&fixture.work, &["branch", "--format=%(refname:short)"]);
    assert!(!branches.lines().any(|line| line == "done"));
    assert!(
        branches.lines().any(|line| line == "wip") && branches.lines().any(|line| line == "stale")
    );
    assert!(
        delete_merged_branches(fixture.path(), vec!["wip".into()], vec![], None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn current_branch_is_never_deleted() {
    let fixture = fixture();
    git_in(&fixture.work, &["switch", "-q", "-c", "feature"]);
    let results = delete_merged_branches(
        fixture.path(),
        vec!["feature".into()],
        vec![fixture.tip("feature")],
        Some("main".into()),
    )
    .await
    .unwrap();
    assert!(results[0].error.as_deref().unwrap().contains("current"));
}

#[tokio::test]
async fn remote_delete_uses_lease_and_removes_branch() {
    let fixture = fixture();
    branch_with_commit(&fixture, "done", true);
    branch_with_commit(&fixture, "wip", false);
    branch_with_commit(&fixture, "raced", true);
    git_in(
        &fixture.work,
        &["push", "-q", "origin", "main", "done", "wip", "raced"],
    );
    let raced_seen = fixture.tip("raced");
    git_in(&fixture.work, &["switch", "-q", "raced"]);
    fixture.commit("r.txt", "extra");
    git_in(&fixture.work, &["push", "-q", "origin", "raced"]);
    git_in(&fixture.work, &["switch", "-q", "main"]);
    let names: Vec<String> = ["done", "wip", "raced", "main", "master"]
        .iter()
        .map(|name| name.to_string())
        .collect();
    let expected = vec![
        fixture.tip("done"),
        fixture.tip("wip"),
        raced_seen,
        fixture.tip("main"),
        fixture.tip("main"),
    ];
    let results = delete_remote_branches(fixture.path(), "origin".into(), names, expected, None)
        .await
        .unwrap();
    assert!(results[0].deleted, "{:?}", results[0].error);
    assert!(results[1].error.as_deref().unwrap().contains("not merged"));
    assert!(!results[2].deleted);
    assert!(results[3].error.as_deref().unwrap().contains("protected"));
    assert!(results[4].error.as_deref().unwrap().contains("protected"));
    assert_eq!(fixture.remote_tip("done"), None);
    assert!(
        fixture.remote_tip("wip").is_some()
            && fixture.remote_tip("raced").is_some()
            && fixture.remote_tip("main").is_some()
    );
}

#[tokio::test]
async fn remote_lease_mismatch_is_refused_by_git() {
    let fixture = fixture();
    branch_with_commit(&fixture, "done", true);
    git_in(&fixture.work, &["push", "-q", "origin", "done"]);
    let other = git_in(&fixture.work, &["rev-parse", "main~1"]);
    let results = delete_remote_branches(
        fixture.path(),
        "origin".into(),
        vec!["done".into()],
        vec![other],
        None,
    )
    .await
    .unwrap();
    assert!(!results[0].deleted);
    assert!(fixture.remote_tip("done").is_some());
}

#[tokio::test]
async fn remote_delete_validates_inputs() {
    let fixture = fixture();
    let call = |remote: &str, name: &str| {
        delete_remote_branches(
            fixture.path(),
            remote.into(),
            vec![name.into()],
            vec![fixture.tip("main")],
            None,
        )
    };
    assert!(call("nowhere", "x").await.is_err());
    assert!(call("--all", "x").await.is_err());
    for name in ["HEAD", "-f", "refs/heads/x", "a..b"] {
        assert!(
            call("origin", name).await.unwrap()[0].error.is_some(),
            "{name}"
        );
    }
}
