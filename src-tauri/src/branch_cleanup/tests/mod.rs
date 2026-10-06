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

static SERIAL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

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
    let base = crate::env_names::var_os("SKEIN_TEST_TMP")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let serial = SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = base.join(format!(
        "skein-cleanup-{}-{nonce}-{serial}",
        std::process::id()
    ));
    let bare = root.join("remote.git");
    let work = root.join("work");
    std::fs::create_dir_all(&work).unwrap();
    git_in(&root, &["init", "-q", "--bare", "-b", "main", "remote.git"]);
    git_in(&work, &["init", "-q", "-b", "main"]);
    for (key, value) in [
        ("user.name", "Test User"),
        ("user.email", "test@example.test"),
        ("commit.gpgsign", "false"),
        ("core.autocrlf", "false"),
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

mod list;
mod local;
mod remote;

fn names(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

fn error_of(outcome: &BranchOutcome) -> &str {
    outcome.error.as_deref().unwrap_or("")
}
