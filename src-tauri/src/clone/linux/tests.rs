use super::*;
use crate::linux_guard::folders::Fixture;

fn job(fixture: &Fixture, name: &str) -> Job {
    Job {
        id: "item".into(),
        url: "https://example.test/repo".into(),
        dest: fixture.0.join(name).to_str().unwrap().into(),
        ref_type: "branch".into(),
        ref_name: "main".into(),
    }
}

fn settings(fixture: &Fixture, job: &Job) -> Settings {
    Settings {
        sources: vec![],
        workspace: serde_json::json!({"root": fixture.0, "layout": "flat", "sets": [{"id":"set", "items":[{"id":job.id, "name":Path::new(&job.dest).file_name().unwrap().to_str().unwrap(), "url":job.url, "ref":{"type":job.ref_type, "name":job.ref_name}}]}]}),
    }
}

#[tokio::test]
async fn reclone_preserves_and_restores_original_with_a_unique_collision_path() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("reclone-");
    let original = fixture.repository("repo");
    let job = job(&fixture, "repo");
    let mut admission = admit(&settings(&fixture, &job), &job).unwrap();
    let source = admission.existing().await.unwrap();
    let inode = std::os::unix::fs::MetadataExt::ino(&std::fs::metadata(&original).unwrap());
    std::fs::create_dir(fixture.0.join("repo.bak-collision")).unwrap();
    let path = admission.preserve(&source, "repo.bak-collision").unwrap();
    assert_eq!(path, fixture.0.join("repo.bak-collision-1"));
    assert_eq!(
        std::fs::read(path.join("file")).unwrap(),
        b"restorable data"
    );
    assert!(fixture.0.join("repo.bak-collision").is_dir());
    let parent = Directory::open(&fixture.0).unwrap();
    Directory::open(&path)
        .unwrap()
        .move_to(&parent, "repo")
        .unwrap();
    assert_eq!(
        std::os::unix::fs::MetadataExt::ino(&std::fs::metadata(&original).unwrap()),
        inode
    );
    assert_eq!(
        std::fs::read(original.join("file")).unwrap(),
        b"restorable data"
    );
}

#[tokio::test]
async fn reclone_refuses_changed_root_metadata_and_origin() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    for change in ["root", "metadata", "origin"] {
        let fixture = Fixture::new("clone-conflict-");
        let path = fixture.repository("repo");
        let job = job(&fixture, "repo");
        let mut admission = admit(&settings(&fixture, &job), &job).unwrap();
        let source = admission.existing().await.unwrap();
        match change {
            "root" => {
                std::fs::rename(&path, fixture.0.join("moved")).unwrap();
                std::fs::create_dir(&path).unwrap();
            }
            "metadata" => {
                std::fs::rename(path.join(".git"), path.join("saved-git")).unwrap();
                std::fs::create_dir(path.join(".git")).unwrap();
            }
            _ => {
                assert!(std::process::Command::new("git")
                    .arg("-C")
                    .arg(&path)
                    .args([
                        "remote",
                        "set-url",
                        "origin",
                        "https://example.test/changed"
                    ])
                    .status()
                    .unwrap()
                    .success());
            }
        }
        assert!(admission.preserve(&source, "backup").is_err());
        assert!(!fixture.0.join("backup").exists());
    }
}

#[test]
fn clone_publication_refuses_nonempty_and_swapped_parents_without_overwriting() {
    let fixture = Fixture::new("clone-publication-");
    let job = job(&fixture, "repo");
    let admission = admit(&settings(&fixture, &job), &job).unwrap();
    let stage = fixture.repository("stage");
    let directory = Directory::open(&stage).unwrap();
    let repository = Root::open(&directory.path, &[]).unwrap();
    std::fs::create_dir(&job.dest).unwrap();
    std::fs::write(Path::new(&job.dest).join("sentinel"), b"keep").unwrap();
    assert!(publish(&admission, &directory, &repository).is_err());
    assert_eq!(
        std::fs::read(Path::new(&job.dest).join("sentinel")).unwrap(),
        b"keep"
    );
    assert_eq!(
        std::fs::read(stage.join("file")).unwrap(),
        b"restorable data"
    );
    std::fs::rename(&fixture.0, fixture.0.with_extension("moved")).unwrap();
    std::fs::create_dir(&fixture.0).unwrap();
    assert!(publish(&admission, &directory, &repository).is_err());
    assert_eq!(
        std::fs::read(fixture.0.with_extension("moved").join("stage/file")).unwrap(),
        b"restorable data"
    );
}

#[test]
fn clone_checks_the_saved_ref_before_any_filesystem_creation() {
    let fixture = Fixture::new("clone-ref-");
    let mut job = job(&fixture, "repo");
    let settings = settings(&fixture, &job);
    job.ref_name = "other".into();
    assert!(admit(&settings, &job).is_err());
    assert!(!Path::new(&job.dest).exists());
}

#[tokio::test]
async fn local_clone_uses_pinned_staging_and_publishes_only_after_success() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("clone-native-");
    let origin = fixture.repository("origin");
    for args in [
        vec!["add", "file"],
        vec![
            "-c",
            "user.name=admin",
            "-c",
            "user.email=admin@example.test",
            "commit",
            "-qm",
            "fixture",
        ],
    ] {
        assert!(std::process::Command::new("git")
            .arg("-C")
            .arg(&origin)
            .args(args)
            .status()
            .unwrap()
            .success());
    }
    let mut job = job(&fixture, "new");
    job.url = origin.to_str().unwrap().into();
    let saved = settings(&fixture, &job);
    let admission = admit(&saved, &job).unwrap();
    let stage = admission.directory.create_new("stage").unwrap();
    let path = stage.fd_path();
    let output = crate::git::buffered(
        &["clone", "-b", "main", "--", &job.url, &path],
        "local clone fixture",
        &[0],
    )
    .await
    .unwrap();
    assert_eq!(output.code, Some(0));
    assert!(!Path::new(&job.dest).exists());
    let repository = verify_clone(&admission, &stage).await.unwrap();
    publish(&admission, &stage, &repository).unwrap();
    assert_eq!(
        std::fs::read(Path::new(&job.dest).join("file")).unwrap(),
        b"restorable data"
    );
    assert_eq!(
        serde_json::to_value(saved.workspace["sets"].clone()).unwrap()[0]["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn failed_and_cancelled_clones_retain_staging_without_publishing() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    for cancelled in [false, true] {
        let fixture = Fixture::new("clone-failed-");
        let mut job = job(&fixture, "new");
        job.url = fixture.0.join("missing-origin").to_str().unwrap().into();
        let saved = settings(&fixture, &job);
        let admission = admit(&saved, &job).unwrap();
        let stage = admission.directory.create_new("stage").unwrap();
        let path = stage.fd_path();
        let output = crate::git::execute_cancellable(
            crate::git::Request {
                args: &["clone", "--", &job.url, &path],
                context: "failed clone fixture",
                timeout: std::time::Duration::from_secs(5),
                expected: &[0],
                policy: crate::git::OutputPolicy::Text,
            },
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(cancelled)),
        )
        .await;
        if cancelled {
            assert!(matches!(output, Err(error) if error.contains("cancelled")));
        } else {
            assert_ne!(output.unwrap().code, Some(0));
        }
        assert!(!Path::new(&job.dest).exists());
        assert!(stage.path.exists());
        assert_eq!(
            saved.workspace["sets"][0]["items"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}

#[tokio::test]
async fn clone_verification_refuses_changed_origin_or_local_commit_before_publication() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    for change in ["origin", "ref"] {
        let fixture = Fixture::new("clone-verify-");
        let origin = fixture.repository("origin");
        for args in [
            vec!["add", "file"],
            vec![
                "-c",
                "user.name=admin",
                "-c",
                "user.email=admin@example.test",
                "commit",
                "-qm",
                "fixture",
            ],
        ] {
            assert!(std::process::Command::new("git")
                .arg("-C")
                .arg(&origin)
                .args(args)
                .status()
                .unwrap()
                .success());
        }
        let mut job = job(&fixture, "new");
        job.url = origin.to_str().unwrap().into();
        let saved = settings(&fixture, &job);
        let admission = admit(&saved, &job).unwrap();
        let stage = admission.directory.create_new("stage").unwrap();
        let output = crate::git::buffered(
            &["clone", "-b", "main", "--", &job.url, &stage.fd_path()],
            "clone verification fixture",
            &[0],
        )
        .await
        .unwrap();
        assert_eq!(output.code, Some(0));
        if change == "origin" {
            assert!(std::process::Command::new("git")
                .arg("-C")
                .arg(&stage.path)
                .args([
                    "remote",
                    "set-url",
                    "origin",
                    "https://example.test/changed"
                ])
                .status()
                .unwrap()
                .success());
        } else {
            std::fs::write(stage.path.join("file"), b"later edit").unwrap();
            for args in [
                vec!["add", "file"],
                vec![
                    "-c",
                    "user.name=admin",
                    "-c",
                    "user.email=admin@example.test",
                    "commit",
                    "-qm",
                    "later edit",
                ],
            ] {
                assert!(std::process::Command::new("git")
                    .arg("-C")
                    .arg(&stage.path)
                    .args(args)
                    .status()
                    .unwrap()
                    .success());
            }
        }
        assert!(verify_clone(&admission, &stage).await.is_err());
        assert!(!Path::new(&job.dest).exists());
        assert!(stage.path.join("file").exists());
    }
}
