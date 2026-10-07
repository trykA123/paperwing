use super::*;

#[tokio::test]
async fn attribute_free_counts_skip_git_and_materialization_without_environment_opt_in() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", b"old\n");
    fixture.commit("base").await;
    let storage = fixture.0.join("unused-diff-storage");
    let eligibility = count_eligibility::Eligibility::new(storage.clone());
    let mut job = fixture.job();
    job.rust_counts = eligibility
        .allows(&fixture.0.join("repo"), &job)
        .await
        .unwrap();
    assert!(job.rust_counts);
    #[cfg(target_os = "linux")]
    {
        job.diff = None;
    }
    let before = git::activity_snapshot().len();
    assert_eq!(
        line_counts(b"old\n", b"new\nextra\n", &job).await.unwrap(),
        Some(Lines {
            added: 2,
            removed: 1
        })
    );
    assert_eq!(git::activity_snapshot().len(), before);
    assert!(!storage.exists());
    assert!(eligibility
        .allows(&fixture.0.join("repo"), &job)
        .await
        .unwrap());
    assert_eq!(git::activity_snapshot().len(), before);
}

#[tokio::test]
async fn only_applicable_storage_attributes_require_git_counts() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    for placement in [
        "nested/.gitattributes",
        ".git/info/attributes",
        "configured",
        "storage",
    ] {
        let fixture = Fixture::new().await;
        fixture.write("file", b"old\n");
        fixture.commit("base").await;
        #[cfg(target_os = "linux")]
        let storage = fixture.diff_data();
        #[cfg(not(target_os = "linux"))]
        let storage = fixture.0.join("storage");
        let job = fixture.job();
        match placement {
            "configured" => {
                fixture
                    .git(&["config", "core.attributesFile", "attributes"])
                    .await;
            }
            "storage" => {
                std::fs::create_dir_all(&storage).unwrap();
                std::fs::write(storage.join(".gitattributes"), b"* -diff\n").unwrap();
            }
            path => fixture.write(path, b"* -diff\n"),
        }
        let eligibility = count_eligibility::Eligibility::new(storage);
        assert!(
            eligibility
                .allows(&fixture.0.join("repo"), &job)
                .await
                .unwrap()
                == (placement != "storage"),
            "{placement}"
        );
        assert_eq!(
            line_counts(b"old\n", b"new\n", &job).await.unwrap(),
            legacy::counts(b"old\n", b"new\n", &job).await.unwrap()
        );
    }
}

#[tokio::test]
async fn linked_worktrees_read_from_the_shared_object_database() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", b"shared blob\n");
    fixture.commit("base").await;
    let repo = fixture.0.join("repo");
    std::fs::rename(repo.join(".git"), fixture.0.join("git-store")).unwrap();
    std::fs::write(repo.join(".git"), b"gitdir: ../git-store\n").unwrap();
    let linked = fixture.0.join("linked");
    fixture
        .git(&[
            "worktree",
            "add",
            "--detach",
            linked.to_str().unwrap(),
            "HEAD",
        ])
        .await;
    let oid = decode(&fixture.git(&["rev-parse", "HEAD:file"]).await)
        .unwrap()
        .trim()
        .to_owned();
    let reader = git::BatchReader::new(linked.clone(), Arc::new(AtomicBool::new(false)));
    assert_eq!(reader.read(&oid).await.unwrap(), b"shared blob\n");
    assert!(reader.is_started());
    fixture.git(&["worktree", "remove", "../linked"]).await;
    assert!(!reader.is_started());
    assert!(reader.read(&oid).await.is_err());
}

#[tokio::test]
async fn root_revocation_closes_every_reader_before_the_directory_moves() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", b"blob\n");
    fixture.commit("base").await;
    let oid = decode(&fixture.git(&["rev-parse", "HEAD:file"]).await)
        .unwrap()
        .trim()
        .to_owned();
    let mut readers = Vec::new();
    for _ in 0..12 {
        let reader =
            git::BatchReader::new(fixture.0.join("repo"), Arc::new(AtomicBool::new(false)));
        assert_eq!(reader.read(&oid).await.unwrap(), b"blob\n");
        readers.push(reader);
    }
    let extra = git::BatchReader::new(fixture.0.join("repo"), Arc::new(AtomicBool::new(false)));
    assert!(extra.read(&oid).await.unwrap_err().contains("capacity"));
    assert_eq!(
        decode(&fixture.git(&["rev-parse", "HEAD:file"]).await)
            .unwrap()
            .trim(),
        oid
    );
    git::BatchReader::close_root(&fixture.0.join("repo"))
        .await
        .unwrap();
    assert!(readers.iter().all(|reader| !reader.is_started()));
    std::fs::rename(fixture.0.join("repo"), fixture.0.join("moved")).unwrap();
    #[cfg(target_os = "linux")]
    assert!(git::runner_idle());
}

#[tokio::test]
async fn batch_read_errors_restart_only_once() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", b"blob\n");
    fixture.commit("base").await;
    let reader = git::BatchReader::new(fixture.0.join("repo"), Arc::new(AtomicBool::new(false)));
    assert!(reader.read(&"0".repeat(40)).await.is_err());
    let oid = decode(&fixture.git(&["rev-parse", "HEAD:file"]).await)
        .unwrap()
        .trim()
        .to_owned();
    assert!(reader.read(&oid).await.is_err());
    reader.close().await;
}

#[tokio::test]
async fn idle_readers_close_after_thirty_seconds_and_can_start_again() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", b"blob\n");
    fixture.commit("base").await;
    let oid = decode(&fixture.git(&["rev-parse", "HEAD:file"]).await)
        .unwrap()
        .trim()
        .to_owned();
    let reader = git::BatchReader::new(fixture.0.join("repo"), Arc::new(AtomicBool::new(false)));
    reader.read(&oid).await.unwrap();
    let started = std::time::Instant::now();
    tokio::time::timeout(Duration::from_secs(32), reader.wait_idle())
        .await
        .unwrap();
    assert!(started.elapsed() >= Duration::from_secs(30));
    assert!(!reader.is_started());
    assert_eq!(reader.read(&oid).await.unwrap(), b"blob\n");
    reader.close().await;
}

#[tokio::test]
async fn attribute_probes_match_no_index_configuration() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    if let Ok(case) = std::env::var("SKEIN_COUNT_ATTRIBUTE_CASE") {
        let fixture = Fixture::new().await;
        fixture.write("file", b"old\n");
        fixture.commit("base").await;
        let eligibility = count_eligibility::Eligibility::new(fixture.0.join("storage"));
        assert!(
            eligibility
                .allows(&fixture.0.join("repo"), &fixture.job())
                .await
                .unwrap()
                == (case != "system"),
            "{case}"
        );
        return;
    }
    let fixture = Fixture::new().await;
    let attributes = fixture.0.join("attributes");
    std::fs::write(&attributes, b"* -diff\n").unwrap();
    let config = fixture.0.join("global-config");
    std::fs::write(
        &config,
        format!("[core]\nattributesFile = {}\n", attributes.display()),
    )
    .unwrap();
    #[cfg(target_os = "linux")]
    let cases = ["global", "system", "system-unrelated"];
    #[cfg(not(target_os = "linux"))]
    let cases = ["global"];
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        let bin = fixture.0.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let git = bin.join("git");
        std::fs::write(&git, b"#!/usr/bin/python3\nimport os, sys\nargs = sys.argv[1:]\nfor command in ('check-attr', 'diff'):\n    if command in args:\n        index = args.index(command)\n        args[index:index] = ['-c', 'core.attributesFile=' + os.environ['SKEIN_SYSTEM_ATTRIBUTES']]\n        break\nos.execv('/usr/bin/git', ['git'] + args)\n").unwrap();
        std::fs::set_permissions(git, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    for case in cases {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "compare::tests::review_fixes::attribute_probes_match_no_index_configuration",
                "--nocapture",
            ])
            .env("SKEIN_COUNT_ATTRIBUTE_CASE", case)
            .env_remove("GIT_ATTR_NOSYSTEM");
        if case.starts_with("system") {
            let system_attributes = fixture.0.join(format!("attributes-{case}"));
            std::fs::write(
                &system_attributes,
                if case == "system" {
                    b"* -diff\n".as_slice()
                } else {
                    b"*.png -diff\n".as_slice()
                },
            )
            .unwrap();
            command.env("SKEIN_SYSTEM_ATTRIBUTES", &system_attributes);
            let mut path = vec![fixture.0.join("bin")];
            path.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
            command.env("PATH", std::env::join_paths(path).unwrap());
        } else {
            command.env("GIT_CONFIG_GLOBAL", &config);
        }
        let output = tokio::task::spawn_blocking(move || command.output().unwrap())
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        );
    }
}

#[tokio::test]
async fn branch_cleanup_releases_readers_before_deleting_refs() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", b"blob\n");
    let commit = fixture.commit("base").await;
    fixture.git(&["branch", "cleanup"]).await;
    let oid = decode(&fixture.git(&["rev-parse", "HEAD:file"]).await)
        .unwrap()
        .trim()
        .to_owned();
    let root = fixture.0.join("repo");
    let reader = git::BatchReader::new(root.clone(), Arc::new(AtomicBool::new(false)));
    reader.read(&oid).await.unwrap();
    let outcomes = crate::branch_cleanup::delete_merged_branches(
        root.to_str().unwrap().into(),
        vec!["cleanup".into()],
        vec![commit],
        Some("main".into()),
    )
    .await
    .unwrap();
    assert!(serde_json::to_value(outcomes).unwrap()[0]["deleted"]
        .as_bool()
        .unwrap());
    assert!(!reader.is_started());
    assert!(reader.read(&oid).await.is_err());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn refresh_rechecks_attributes_added_to_the_storage_root() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", b"old\n");
    fixture.commit("base").await;
    fixture.write("file", b"new\n");
    let service = fixture.service();
    let settings = fixture.settings();
    let opened = service
        .open(
            &settings,
            fixture.context(CompareRef::Head).endpoint,
            fixture.context(CompareRef::WorkingTree).endpoint,
        )
        .await
        .unwrap();
    let first = service
        .refresh(&settings, &opened.id, Options::default())
        .await
        .unwrap();
    assert!(matches!(first, RefreshResult::Ready { .. }));
    let prepared = service.snapshot(&settings, &opened.id, 1).await.unwrap().0;
    assert!(prepared
        .rows
        .iter()
        .find(|row| row.path == "file")
        .unwrap()
        .raw_lines
        .is_some());
    let storage = fixture.diff_data();
    std::fs::create_dir_all(&storage).unwrap();
    std::fs::write(storage.join(".gitattributes"), b"* -diff\n").unwrap();
    let second = service
        .refresh(&settings, &opened.id, Options::default())
        .await
        .unwrap();
    assert!(matches!(second, RefreshResult::Ready { .. }));
    let prepared = service.snapshot(&settings, &opened.id, 2).await.unwrap().0;
    assert!(prepared
        .rows
        .iter()
        .find(|row| row.path == "file")
        .unwrap()
        .raw_lines
        .is_none());
    service.close(&opened.id).await;
}
