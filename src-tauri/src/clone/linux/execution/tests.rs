use super::*;
use crate::clone::Job;
use crate::linux_guard::folders::Fixture;

fn job(fixture: &Fixture) -> Job {
    Job {
        id: "item".into(),
        url: "https://example.test/repo".into(),
        dest: fixture.0.join("repo").to_str().unwrap().into(),
        ref_type: "branch".into(),
        ref_name: "main".into(),
    }
}

#[test]
fn checked_out_branch_reports_done_when_local_changes_or_divergence_refuse_fast_forward() {
    for divergence in [false, true] {
        let fixture = Fixture::new("clone-switch-merge-");
        let path = fixture.repository("repo");
        commit(&path);
        git(&path, &["checkout", "-qb", "upstream"]);
        std::fs::write(path.join("file"), b"upstream change").unwrap();
        commit(&path);
        git(&path, &["checkout", "main"]);
        git(&path, &["config", "branch.main.remote", "."]);
        git(
            &path,
            &["config", "branch.main.merge", "refs/heads/upstream"],
        );
        std::fs::write(path.join("file"), b"local change").unwrap();
        if divergence {
            commit(&path);
        }
        git(&path, &["checkout", "main"]);
        let merge = std::process::Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(["merge", "--ff-only", "@{u}"])
            .output()
            .unwrap();
        assert!(!merge.status.success());
        let error = String::from_utf8(merge.stderr).unwrap();
        let job = job(&fixture);
        for mode in ["switch", "clone"] {
            assert_eq!(
                finish_update(&job, mode, Err(error.clone())).unwrap(),
                (
                    "done",
                    "On main (not fast-forwarded: local changes or diverged)".into()
                )
            );
        }
        assert_eq!(
            finish_update(&job, "pull", Err(error.clone())).unwrap_err(),
            format!("Pull needs a fast-forward: {error}")
        );
        assert_eq!(std::fs::read(path.join("file")).unwrap(), b"local change");
    }
}

fn git(path: &std::path::Path, args: &[&str]) {
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .unwrap()
        .status
        .success());
}

fn commit(path: &std::path::Path) {
    git(path, &["add", "file"]);
    git(
        path,
        &[
            "-c",
            "user.name=admin",
            "-c",
            "user.email=admin@example.test",
            "commit",
            "-qm",
            "fixture",
        ],
    );
}

#[test]
fn interrupted_fast_forwards_still_fail_switch_and_pull() {
    let fixture = Fixture::new("clone-switch-interrupted-");
    let job = job(&fixture);
    for error in ["Git command cancelled", "Git command timed out"] {
        assert_eq!(
            finish_update(&job, "switch", Err(error.into())).unwrap_err(),
            error
        );
        assert_eq!(
            finish_update(&job, "pull", Err(error.into())).unwrap_err(),
            format!("Pull needs a fast-forward: {error}")
        );
    }
}
