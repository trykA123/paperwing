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
    let mut admission = admit(&settings(&fixture, &job), &job, "clone").unwrap();
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
        let mut admission = admit(&settings(&fixture, &job), &job, "clone").unwrap();
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
    let admission = admit(&settings(&fixture, &job), &job, "clone").unwrap();
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
    std::fs::remove_dir(&fixture.0).unwrap();
    std::fs::rename(fixture.0.with_extension("moved"), &fixture.0).unwrap();
}

#[test]
fn clone_checks_the_saved_ref_before_any_filesystem_creation() {
    let fixture = Fixture::new("clone-ref-");
    let mut job = job(&fixture, "repo");
    let settings = settings(&fixture, &job);
    job.ref_name = "other".into();
    assert!(admit(&settings, &job, "clone").is_err());
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
    let admission = admit(&saved, &job, "clone").unwrap();
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
        let admission = admit(&saved, &job, "clone").unwrap();
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
async fn clone_verification_cleans_staging_with_changed_origin_or_local_commit() {
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
        let admission = admit(&saved, &job, "clone").unwrap();
        let stage = admission
            .directory
            .create_new(".skein-clone-test")
            .unwrap();
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
        assert!(!stage.path.exists());
    }
}

#[test]
fn refused_jobs_and_duplicate_destinations_do_not_block_eligible_jobs() {
    let fixture = Fixture::new("clone-partial-admission-");
    let first = job(&fixture, "Folder");
    let mut second = job(&fixture, "folder");
    second.id = "second".into();
    let mut saved = settings(&fixture, &first);
    let mut item = saved.workspace["sets"][0]["items"][0].clone();
    item["id"] = second.id.clone().into();
    item["name"] = "folder".into();
    saved.workspace["sets"][0]["items"]
        .as_array_mut()
        .unwrap()
        .push(item);
    let mut refused = first.clone();
    refused.ref_name = "unsaved-ref".into();
    let jobs = vec![refused, first.clone(), first, second];
    let results = admit_jobs(&saved, jobs, "clone");
    assert_eq!(
        results
            .iter()
            .map(|(_, result)| result.is_ok())
            .collect::<Vec<_>>(),
        [false, true, false, true]
    );
    assert!(results[2]
        .1
        .as_ref()
        .err()
        .unwrap()
        .contains("share a destination"));
    assert!(!fixture.0.join("Folder").exists());
    assert!(!fixture.0.join("folder").exists());
}

#[test]
fn only_clone_admission_creates_missing_parents() {
    let fixture = Fixture::new("clone-missing-parent-");
    let job = job(&fixture, "nested/repo");
    let mut saved = settings(&fixture, &job);
    saved.workspace["layout"] = "custom".into();
    saved.workspace["pathTemplate"] = "nested/{folder}".into();
    saved.workspace["sets"][0]["name"] = "Set".into();
    saved.workspace["sets"][0]["items"][0]["org"] = "org".into();
    saved.workspace["sets"][0]["items"][0]["repoId"] = "source:repo".into();
    for mode in ["fetch", "pull", "switch"] {
        assert!(admit(&saved, &job, mode).is_err());
        assert!(!fixture.0.join("nested").exists());
    }
    assert!(admit(&saved, &job, "clone").is_ok());
    assert!(fixture.0.join("nested").is_dir());
    assert!(!Path::new(&job.dest).exists());
}

#[tokio::test]
async fn existing_origin_accepts_equivalent_ssh_and_https_repository_keys() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("clone-origin-key-");
    let path = fixture.repository("repo");
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(&path)
        .args(["remote", "set-url", "origin", "git@example.test:repo.git"])
        .status()
        .unwrap()
        .success());
    let job = job(&fixture, "repo");
    let mut admission = admit(&settings(&fixture, &job), &job, "clone").unwrap();
    assert!(admission.existing().await.is_ok());
}

#[tokio::test]
async fn fresh_clone_with_instead_of_rewrite_verifies_and_publishes() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("clone-instead-of-");
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
    let job = job(&fixture, "new");
    let admission = admit(&settings(&fixture, &job), &job, "clone").unwrap();
    let stage = admission
        .directory
        .create_new(".skein-clone-rewrite")
        .unwrap();
    let rewrite = format!("url.{}.insteadOf={}", origin.display(), job.url);
    let output = crate::git::buffered(
        &[
            "-c",
            &rewrite,
            "clone",
            "-b",
            "main",
            "--",
            &job.url,
            &stage.fd_path(),
        ],
        "rewritten local clone fixture",
        &[0],
    )
    .await
    .unwrap();
    assert_eq!(output.code, Some(0));
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(&stage.path)
        .args([
            "config",
            &format!("url.{}.insteadOf", origin.display()),
            &job.url
        ])
        .status()
        .unwrap()
        .success());
    let expanded = crate::git::buffered(
        &["-C", &stage.fd_path(), "remote", "get-url", "origin"],
        "expanded origin fixture",
        &[0],
    )
    .await
    .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&expanded.stdout).trim(),
        origin.to_str().unwrap()
    );
    let repository = verify_clone(&admission, &stage).await.unwrap();
    publish(&admission, &stage, &repository).unwrap();
    assert_eq!(
        std::fs::read(Path::new(&job.dest).join("file")).unwrap(),
        b"restorable data"
    );
}

#[tokio::test]
async fn verification_cleanup_refuses_a_substituted_staging_directory() {
    let fixture = Fixture::new("clone-cleanup-substitution-");
    let job = job(&fixture, "new");
    let admission = admit(&settings(&fixture, &job), &job, "clone").unwrap();
    let stage = admission
        .directory
        .create_new(".skein-clone-test")
        .unwrap();
    std::fs::write(stage.path.join("file"), b"clone data").unwrap();
    let retained = fixture.0.join("retained-stage");
    std::fs::rename(&stage.path, &retained).unwrap();
    std::fs::create_dir(&stage.path).unwrap();
    std::fs::write(stage.path.join("sentinel"), b"external data").unwrap();
    let error = match verify_clone(&admission, &stage).await {
        Ok(_) => panic!("Substituted staging directory was verified"),
        Err(error) => error,
    };
    assert!(error.contains("Could not clean staging"));
    assert_eq!(
        std::fs::read(stage.path.join("sentinel")).unwrap(),
        b"external data"
    );
    assert_eq!(std::fs::read(retained.join("file")).unwrap(), b"clone data");
    assert!(!Path::new(&job.dest).exists());
}
