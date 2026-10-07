use super::*;

const LEFT: &[u8] = b"e\nd\nc\nc\nc\nb\ne\nc\na\na\nb\nd\n";
const RIGHT: &[u8] = b"d\nc\ne\nc\nd\nb\ne\na\nb\nb\nb\ne\n";

#[tokio::test]
async fn autocrlf_storage_counts_match_git_with_and_without_normalized_eol() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    if std::env::var_os("SKEIN_ROUND2_AUTOCRLF").is_some() {
        let fixture = Fixture::new().await;
        fixture.write("file", b"old\n");
        fixture.commit("base").await;
        fixture.git(&["config", "core.autocrlf", "false"]).await;
        let mut job = fixture.job();
        let eligibility = count_eligibility::Eligibility::new(job.count_root.clone().unwrap());
        job.rust_counts = eligibility
            .configuration(&fixture.0.join("repo"), &job)
            .await
            .unwrap();
        for (left, right) in [
            (b"same\r\n".as_slice(), b"same\n".as_slice()),
            (b"same\r\nlast", b"same\nlast"),
            (b"old\r\n", b"new\nextra\n"),
            (b"old\r", b"new\r"),
            (b"one\rtwo\r\n", b"one\rtwo\n"),
            (b"old\n", b"new\nextra\n"),
            (b"\xef\xbb\xbfsame\r\n", b"\xef\xbb\xbfsame\n"),
        ] {
            for normalize_eol in [false, true] {
                let options = Options {
                    normalize_eol,
                    ignore_whitespace: false,
                };
                let (left, right) = (normalized(left, &options), normalized(right, &options));
                assert_eq!(
                    line_counts(&left, &right, &job).await.unwrap(),
                    legacy::counts(&left, &right, &job).await.unwrap(),
                    "normalize_eol={normalize_eol}"
                );
            }
        }
        #[cfg(target_os = "linux")]
        let storage = fixture.diff_data();
        #[cfg(not(target_os = "linux"))]
        let storage = fixture.0.join("repo/count-storage");
        let eligibility = count_eligibility::Eligibility::new(storage);
        assert!(eligibility
            .configuration(&fixture.0.join("repo"), &job)
            .await
            .unwrap()
            .is_some());
        return;
    }
    let fixture = Fixture::new().await;
    for setting in ["false", "true", "input"] {
        let config = fixture.0.join(format!("global-{setting}"));
        std::fs::write(&config, format!("[core]\nautocrlf = {setting}\n")).unwrap();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", "compare::tests::review_round2::autocrlf_storage_counts_match_git_with_and_without_normalized_eol", "--nocapture"])
            .env("SKEIN_ROUND2_AUTOCRLF", setting).env("GIT_CONFIG_GLOBAL", config);
        let output = tokio::task::spawn_blocking(move || command.output().unwrap())
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{setting}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[tokio::test]
async fn tree_and_bounded_metadata_honor_the_selected_diff_algorithm() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", LEFT);
    let base = fixture.commit("base").await;
    fixture.write("file", RIGHT);
    fixture.commit("head").await;
    for (algorithm, count) in [("myers", 5), ("histogram", 6)] {
        fixture.git(&["config", "diff.algorithm", algorithm]).await;
        fixture.git(&["config", "diff.renameLimit", "50000"]).await;
        let comparison = prepared(
            &fixture,
            CompareRef::Commit { sha: base.clone() },
            CompareRef::Head,
            Options::default(),
        )
        .await;
        let job = fixture.job();
        let expected = Some(Lines {
            added: count,
            removed: count,
        });
        assert_eq!(comparison.rows[0].raw_lines, expected, "{algorithm}");
        let mut left = comparison.left.clone();
        let entry = left.files["file"].clone();
        left.files.insert("x".repeat(git::CAPTURE_LIMIT), entry);
        let bounded = diff_metadata(&left, &comparison.right, &job).await.unwrap();
        assert_eq!(bounded.lines["file"], expected, "bounded {algorithm}");
        comparison.close_readers().await;
        close_readers(&job.readers).await;
    }
}

#[tokio::test]
async fn unrelated_source_attributes_do_not_disable_storage_counts() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", b"old\n");
    fixture.commit("base").await;
    fixture.write("nested/.gitattributes", b"*.png -diff\n");
    fixture.write(".git/info/attributes", b"*.png -diff\n");
    fixture.git(&["config", "core.autocrlf", "true"]).await;
    fixture
        .git(&["config", "core.attributesFile", "unused-attributes"])
        .await;
    #[cfg(target_os = "linux")]
    let storage = fixture.diff_data();
    #[cfg(not(target_os = "linux"))]
    let storage = fixture.0.join("repo/count-storage");
    std::fs::create_dir_all(&storage).unwrap();
    let eligibility = count_eligibility::Eligibility::new(storage);
    let mut job = fixture.job();
    job.rust_counts = eligibility
        .configuration(&fixture.0.join("repo"), &job)
        .await
        .unwrap();
    assert!(job.rust_counts.is_some());
    let before = git::activity_snapshot().len();
    assert_eq!(
        line_counts(b"old\n", b"new\n", &job).await.unwrap(),
        Some(Lines {
            added: 1,
            removed: 1
        })
    );
    assert_eq!(git::activity_snapshot().len(), before);
    assert!(eligibility
        .configuration(&fixture.0.join("repo"), &job)
        .await
        .unwrap()
        .is_some());
    assert_eq!(git::activity_snapshot().len(), before);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn attributes_for_materialized_child_paths_keep_the_git_fallback() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", b"old\n");
    fixture.commit("base").await;
    let storage = fixture.diff_data();
    std::fs::create_dir_all(&storage).unwrap();
    std::fs::write(
        storage.join(".gitattributes"),
        b"**/d-*/left -diff\n**/d-*/right -diff\n",
    )
    .unwrap();
    let eligibility = count_eligibility::Eligibility::new(storage);
    let mut job = fixture.job();
    job.rust_counts = eligibility
        .configuration(&fixture.0.join("repo"), &job)
        .await
        .unwrap();
    let expected = legacy::counts(b"old\n", b"new\n", &job).await.unwrap();
    assert!(expected.is_none());
    assert_eq!(
        line_counts(b"old\n", b"new\n", &job).await.unwrap(),
        expected
    );
}

#[tokio::test]
async fn same_repository_endpoints_share_the_reader_lifecycle() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", b"old\n");
    let base = fixture.commit("base").await;
    fixture.write("file", b"new\n");
    fixture.commit("head").await;
    let comparison = prepared(
        &fixture,
        CompareRef::Commit { sha: base },
        CompareRef::Head,
        Options::default(),
    )
    .await;
    let oid = comparison.right.files["file"].oid.as_deref().unwrap();
    assert_eq!(comparison.right.reader.read(oid).await.unwrap(), b"new\n");
    comparison.left.reader.close().await;
    let shared_closed = comparison.right.reader.read(oid).await.is_err();
    comparison.close_readers().await;
    assert!(shared_closed);
}

#[tokio::test]
async fn twelve_readers_leave_runner_capacity_available() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", b"blob\n");
    fixture.commit("base").await;
    let oid = decode(&fixture.git(&["rev-parse", "HEAD:file"]).await)
        .unwrap()
        .trim()
        .to_owned();
    let mut readers = Vec::new();
    let mut results = Vec::new();
    for _ in 0..12 {
        let reader =
            git::BatchReader::new(fixture.0.join("repo"), Arc::new(AtomicBool::new(false)));
        results.push(reader.read(&oid).await);
        readers.push(reader);
    }
    let extra = git::BatchReader::new(fixture.0.join("repo"), Arc::new(AtomicBool::new(false)));
    let overflow = extra.read(&oid).await;
    let metadata = fixture.git(&["rev-parse", "HEAD:file"]).await;
    git::BatchReader::close_root(&fixture.0.join("repo"))
        .await
        .unwrap();
    assert!(
        results
            .iter()
            .all(|result| result.as_deref() == Ok(b"blob\n".as_slice())),
        "{results:?}"
    );
    assert!(overflow.unwrap_err().contains("capacity"));
    assert_eq!(decode(&metadata).unwrap().trim(), oid);
}

#[tokio::test]
async fn release_drains_old_sessions_before_its_future_is_polled() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    let service = fixture.service();
    let settings = fixture.settings();
    let left = fixture.context(CompareRef::Head).endpoint;
    let right = fixture.context(CompareRef::WorkingTree).endpoint;
    let old = service
        .open(&settings, left.clone(), right.clone())
        .await
        .unwrap();
    let closing = service.release_sessions();
    let drained = !service.cancel(&old.id).await;
    let new = service.open(&settings, left, right).await.unwrap();
    closing.await;
    let survived = service.close(&new.id).await;
    assert!(drained);
    assert!(survived);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn opening_a_comparison_invalidates_storage_attribute_eligibility() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", b"old\n");
    fixture.commit("base").await;
    let service = fixture.service();
    let counts = service.counts.get().unwrap();
    let job = fixture.job();
    let root = fixture.0.join("repo");
    assert!(counts.configuration(&root, &job).await.unwrap().is_some());
    let storage = fixture.diff_data();
    std::fs::create_dir_all(&storage).unwrap();
    std::fs::write(storage.join(".gitattributes"), b"* -diff\n").unwrap();
    let opened = service
        .open(
            &fixture.settings(),
            fixture.context(CompareRef::Head).endpoint,
            fixture.context(CompareRef::WorkingTree).endpoint,
        )
        .await
        .unwrap();
    let allowed = counts.configuration(&root, &job).await.unwrap();
    service.close(&opened.id).await;
    assert!(allowed.is_none());
}

#[tokio::test]
async fn opening_a_comparison_reloads_the_diff_configuration() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", LEFT);
    let base = fixture.commit("base").await;
    fixture.write("file", RIGHT);
    fixture.commit("head").await;
    fixture.git(&["config", "diff.algorithm", "myers"]).await;
    let service = fixture.service();
    let contexts = [
        fixture.context(CompareRef::Commit { sha: base }),
        fixture.context(CompareRef::Head),
    ];
    let first_job = fixture.job();
    let first = service
        .prepare("first", 1, contexts.clone(), Options::default(), &first_job)
        .await
        .unwrap();
    assert_eq!(
        first.rows[0].raw_lines,
        Some(Lines {
            added: 5,
            removed: 5
        })
    );
    first.close_readers().await;
    close_readers(&first_job.readers).await;
    fixture
        .git(&["config", "diff.algorithm", "histogram"])
        .await;
    let opened = service
        .open(
            &fixture.settings(),
            contexts[0].endpoint.clone(),
            contexts[1].endpoint.clone(),
        )
        .await
        .unwrap();
    let job = fixture.job();
    let next = service
        .prepare("next", 1, contexts, Options::default(), &job)
        .await
        .unwrap();
    let lines = next.rows[0].raw_lines.clone();
    next.close_readers().await;
    close_readers(&job.readers).await;
    service.close(&opened.id).await;
    assert_eq!(
        lines,
        Some(Lines {
            added: 6,
            removed: 6
        })
    );
}

#[tokio::test]
async fn fetch_and_pull_revoke_readers_before_the_git_operation() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let remote = Fixture::new().await;
    remote.write("file", b"remote\n");
    remote.commit("remote").await;
    let fixture = Fixture::new().await;
    fixture.write("file", b"blob\n");
    fixture.commit("base").await;
    fixture
        .git(&[
            "remote",
            "add",
            "origin",
            remote.0.join("repo").to_str().unwrap(),
        ])
        .await;
    let oid = decode(&fixture.git(&["rev-parse", "HEAD:file"]).await)
        .unwrap()
        .trim()
        .to_owned();
    for operation in ["fetch", "pull"] {
        let reader =
            git::BatchReader::new(fixture.0.join("repo"), Arc::new(AtomicBool::new(false)));
        reader.read(&oid).await.unwrap();
        fixture
            .git(&["-c", "alias.unrelated=fetch", "rev-parse", "HEAD"])
            .await;
        assert!(reader.is_started());
        let result = fixture
            .job()
            .run(
                &fixture.0.join("repo"),
                &[operation, "--no-recurse-submodules", "origin", "main"],
                &[0, 1, 128],
            )
            .await;
        let closed = !reader.is_started() && reader.read(&oid).await.is_err();
        reader.close().await;
        result.unwrap();
        assert!(closed, "{operation}");
    }
}
