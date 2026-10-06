use super::*;

#[test]
fn content_ipc_body_contains_only_the_exact_binary_bytes() {
    use tauri::ipc::{InvokeResponseBody, IpcResponse};
    let bytes: Vec<_> = (0..=255).collect();
    let content = Content {
        bytes: bytes.clone(),
    };
    assert!(matches!(content.body().unwrap(), InvokeResponseBody::Raw(result) if result == bytes));
}

#[tokio::test]
async fn historical_readers_use_the_existing_path_after_repository_replacement() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", b"original\n");
    fixture.commit("base").await;
    let job = fixture.job();
    let comparison = prepared(
        &fixture,
        CompareRef::Head,
        CompareRef::Head,
        Options::default(),
    )
    .await;
    let entry = &comparison.left.files["file"];
    assert_eq!(
        content(&comparison.left, "file", entry, &job)
            .await
            .unwrap(),
        b"original\n"
    );
    std::fs::rename(fixture.0.join("repo"), fixture.0.join("original-repo")).unwrap();
    std::fs::create_dir(fixture.0.join("repo")).unwrap();
    fixture.git(&["init", "--initial-branch=main"]).await;
    assert_eq!(
        content(&comparison.left, "file", entry, &job)
            .await
            .unwrap_err()
            .kind,
        "gitError"
    );
    comparison.close_readers().await;
}

#[tokio::test]
async fn gitdir_markers_keep_content_reads_fresh_after_in_place_pointer_changes() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", b"original\n");
    fixture.commit("base").await;
    let repo = fixture.0.join("repo");
    std::fs::rename(repo.join(".git"), repo.join("stored-git")).unwrap();
    std::fs::write(repo.join(".git"), b"gitdir: stored-git\n").unwrap();
    let comparison = prepared(
        &fixture,
        CompareRef::Head,
        CompareRef::Head,
        Options::default(),
    )
    .await;
    let job = fixture.job();
    let entry = &comparison.left.files["file"];
    assert_eq!(
        content(&comparison.left, "file", entry, &job)
            .await
            .unwrap(),
        b"original\n"
    );
    let identity = crate::platform::physical_identity(&repo.join(".git")).unwrap();
    let other = Fixture::new().await;
    std::fs::write(
        repo.join(".git"),
        format!("gitdir: {}\n", other.0.join("repo/.git").display()),
    )
    .unwrap();
    assert_eq!(
        crate::platform::physical_identity(&repo.join(".git")).unwrap(),
        identity
    );
    assert_eq!(
        content(&comparison.left, "file", entry, &job)
            .await
            .unwrap_err()
            .kind,
        "gitError"
    );
    comparison.close_readers().await;
    #[cfg(target_os = "linux")]
    assert!(git::runner_idle());
}

#[tokio::test]
async fn cancellation_during_repeated_blob_reads_joins_the_reader_process() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", &vec![b'x'; paths::CONTENT_LIMIT]);
    fixture.commit("base").await;
    let oid = decode(&fixture.git(&["rev-parse", "HEAD:file"]).await)
        .unwrap()
        .trim()
        .to_owned();
    let cancelled = Arc::new(AtomicBool::new(false));
    let reader = git::BatchReader::new(fixture.0.join("repo"), cancelled.clone());
    let active = reader.clone();
    let task = tokio::spawn(async move {
        loop {
            if let Err(error) = active.read(&oid).await {
                return error;
            }
        }
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        while !reader.is_started() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    cancelled.store(true, Ordering::Relaxed);
    reader.close().await;
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
    #[cfg(target_os = "linux")]
    assert!(git::runner_idle());
}

#[tokio::test]
async fn session_close_cancel_refresh_and_release_reap_endpoint_readers() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", b"old\n");
    let base = fixture.commit("base").await;
    fixture.write("file", b"new\n");
    fixture.commit("head").await;
    let service = fixture.service();
    let settings = fixture.settings();
    for action in ["close", "cancel", "refresh", "release"] {
        let opened = service
            .open(
                &settings,
                fixture
                    .context(CompareRef::Commit { sha: base.clone() })
                    .endpoint,
                fixture.context(CompareRef::Head).endpoint,
            )
            .await
            .unwrap();
        let result = service
            .refresh(&settings, &opened.id, Options::default())
            .await
            .unwrap();
        assert!(matches!(result, RefreshResult::Ready { .. }));
        #[cfg(target_os = "linux")]
        assert!(!git::runner_idle());
        match action {
            "close" => assert!(service.close(&opened.id).await),
            "cancel" => assert!(service.cancel(&opened.id).await),
            "refresh" => {
                let old = service.snapshot(&settings, &opened.id, 1).await.unwrap().0;
                service
                    .refresh(&settings, &opened.id, Options::default())
                    .await
                    .unwrap();
                assert!(old
                    .left
                    .reader
                    .read(old.left.files["file"].oid.as_deref().unwrap())
                    .await
                    .is_err());
                service.close(&opened.id).await;
            }
            "release" => service.release_sessions().await,
            _ => unreachable!(),
        }
        #[cfg(target_os = "linux")]
        assert!(git::runner_idle());
    }
}

#[tokio::test]
async fn cancellation_and_dropped_reader_owners_reap_git_without_extra_reads() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", &vec![b'x'; paths::CONTENT_LIMIT]);
    fixture.commit("base").await;
    let oid = decode(&fixture.git(&["rev-parse", "HEAD:file"]).await)
        .unwrap()
        .trim()
        .to_owned();
    for cancel in [false, true] {
        let cancelled = Arc::new(AtomicBool::new(false));
        let reader = git::BatchReader::new(fixture.0.join("repo"), cancelled.clone());
        assert_eq!(reader.read(&oid).await.unwrap().len(), paths::CONTENT_LIMIT);
        if cancel {
            cancelled.store(true, Ordering::Relaxed);
        }
        drop(reader);
        #[cfg(target_os = "linux")]
        tokio::time::timeout(Duration::from_secs(2), async {
            while !git::runner_idle() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn rust_line_counts_match_git_for_simple_edits_and_fall_back_for_shared_middle_lines() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    let job = fixture.job();
    for (left, right) in [
        (b"".as_slice(), b"added\n".as_slice()),
        (b"left\n", b"right\nextra\n"),
        (b"prefix\nold\nsuffix\n", b"prefix\nnew\nsuffix\n"),
        (b"same\n", b"same"),
        (b"\xef\xbb\xbffirst\r\n", b"\xef\xbb\xbffirst\n"),
        (b"a\nb\na\nc\n", b"b\na\nc\na\n"),
    ] {
        if let Some(rust) = super::super::line_counts::count(left, right, &job).unwrap() {
            assert_eq!(Some(rust), legacy::counts(left, right, &job).await.unwrap());
        }
        assert_eq!(
            line_counts(left, right, &job).await.unwrap(),
            legacy::counts(left, right, &job).await.unwrap()
        );
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn materialization_attributes_preserve_git_binary_line_counts() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    let job = fixture.job();
    let data = fixture.diff_data();
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join(".gitattributes"), b"* -diff\n").unwrap();
    let expected = legacy::counts(b"old\n", b"new\n", &job).await.unwrap();
    assert_eq!(expected, None);
    assert_eq!(
        line_counts(b"old\n", b"new\n", &job).await.unwrap(),
        expected
    );
}
