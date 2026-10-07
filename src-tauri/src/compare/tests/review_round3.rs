use super::*;

#[tokio::test]
async fn opening_a_comparison_does_not_wait_for_a_fetch() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    let service = fixture.service();
    let state = service.fetch_state(&fixture.0.join("repo")).await.unwrap();
    let fetch = state.lock().await;
    let result = tokio::time::timeout(
        Duration::from_millis(250),
        service.open(
            &fixture.settings(),
            fixture.context(CompareRef::Head).endpoint,
            fixture.context(CompareRef::WorkingTree).endpoint,
        ),
    )
    .await;
    drop(fetch);
    let opened = result
        .expect("open waited for the held fetch lock")
        .unwrap();
    service.close(&opened.id).await;
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn rejected_open_keeps_the_configuration_cache() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    let service = fixture.service();
    let settings = fixture.settings();
    for _ in 0..16 {
        service
            .open(
                &settings,
                fixture.context(CompareRef::Head).endpoint,
                fixture.context(CompareRef::WorkingTree).endpoint,
            )
            .await
            .unwrap();
    }
    let counts = service.counts.get().unwrap();
    let job = fixture.job();
    let root = fixture.0.join("repo");
    assert!(counts.configuration(&root, &job).await.unwrap().is_some());
    let storage = fixture.diff_data();
    std::fs::create_dir_all(&storage).unwrap();
    std::fs::write(storage.join(".gitattributes"), b"* -diff\n").unwrap();
    let rejected = service
        .open(
            &settings,
            fixture.context(CompareRef::Head).endpoint,
            fixture.context(CompareRef::WorkingTree).endpoint,
        )
        .await;
    assert!(matches!(rejected, Err(problem) if problem.kind == "limitExceeded"));
    let cached = counts.configuration(&root, &job).await.unwrap();
    service.release_sessions().await;
    assert!(cached.is_some(), "rejected open cleared eligibility");
}

async fn autocrlf_matrix(test: &str) {
    let fixture = Fixture::new().await;
    let mut failures = Vec::new();
    for setting in ["false", "true", "input"] {
        let config = fixture.0.join(format!("global-{setting}"));
        std::fs::write(&config, format!("[core]\nautocrlf = {setting}\n")).unwrap();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", test, "--nocapture"])
            .env("SKEIN_ROUND3_AUTOCRLF", setting)
            .env("GIT_CONFIG_GLOBAL", config);
        let output = tokio::task::spawn_blocking(move || command.output().unwrap())
            .await
            .unwrap();
        if !output.status.success() {
            failures.push(format!(
                "{setting}: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn crlf_counts_run_without_git_using_the_storage_autocrlf() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let Ok(setting) = std::env::var("SKEIN_ROUND3_AUTOCRLF") else {
        autocrlf_matrix(
            "compare::tests::review_round3::crlf_counts_run_without_git_using_the_storage_autocrlf",
        )
        .await;
        return;
    };
    let fixture = Fixture::new().await;
    fixture
        .git(&[
            "config",
            "core.autocrlf",
            if setting == "false" { "true" } else { "false" },
        ])
        .await;
    let mut job = fixture.job();
    let eligibility = count_eligibility::Eligibility::new(job.count_root.clone().unwrap());
    job.rust_counts = eligibility
        .configuration(&fixture.0.join("repo"), &job)
        .await
        .unwrap();
    for (left, right) in [
        (b"same\r\n".as_slice(), b"same\n".as_slice()),
        (b"same\r\nlast", b"same\nlast"),
        (b"old\r\n", b"new\r\nextra\r\n"),
        (b"prefix\r\nold\r\nsuffix\n", b"prefix\nnew\nsuffix\n"),
        (b"\xef\xbb\xbfold\r\n", b"\xef\xbb\xbfnew\r\n"),
    ] {
        let expected = legacy::counts(left, right, &job).await.unwrap();
        assert_eq!(
            super::super::line_counts::count(left, right, &job).unwrap(),
            expected,
            "{setting}"
        );
        let before = git::activity_snapshot().len();
        #[cfg(target_os = "linux")]
        let direct_job = Job {
            diff: None,
            ..job.clone()
        };
        #[cfg(not(target_os = "linux"))]
        let direct_job = job.clone();
        assert_eq!(
            line_counts(left, right, &direct_job).await.unwrap(),
            expected
        );
        assert_eq!(git::activity_snapshot().len(), before);
    }
    for (left, right) in [
        (b"old\r".as_slice(), b"new\r".as_slice()),
        (b"one\rtwo\r\n", b"one\rtwo\n"),
    ] {
        let expected = legacy::counts(left, right, &job).await.unwrap();
        let rust = super::super::line_counts::count(left, right, &job).unwrap();
        assert_eq!(
            rust,
            if setting == "false" {
                expected.clone()
            } else {
                None
            }
        );
        assert_eq!(line_counts(left, right, &job).await.unwrap(), expected);
    }
    assert!(
        super::super::line_counts::count(b"old\0\r\n", b"new\r\n", &job)
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn crlf_working_tree_fingerprints_match_legacy_for_every_autocrlf() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let Ok(setting) = std::env::var("SKEIN_ROUND3_AUTOCRLF") else {
        autocrlf_matrix("compare::tests::review_round3::crlf_working_tree_fingerprints_match_legacy_for_every_autocrlf").await;
        return;
    };
    let fixture = Fixture::new().await;
    fixture.git(&["config", "core.autocrlf", &setting]).await;
    for (path, bytes) in [
        ("edit", b"old\r\n".as_slice()),
        ("same", b"same\r\n"),
        ("lone", b"one\rtwo\r\n"),
        ("binary", b"old\0\r\n"),
        ("bom", b"\xef\xbb\xbfold\r\n"),
        ("incomplete", b"old\r\nlast"),
        ("control", b"\x01\x01a\r\nb\r\n"),
        ("delete", b"\x7f\r\nb\r\n"),
    ] {
        fixture.write(path, bytes);
    }
    fixture.commit("base").await;
    fixture.write("edit", b"new\r\nextra\r\n");
    fixture.write("same", b"same\r\n");
    fixture.write("lone", b"one\rtwo\n");
    fixture.write("binary", b"new\0\r\n");
    fixture.write("bom", b"\xef\xbb\xbfnew\r\n");
    fixture.write("incomplete", b"new\r\nlast");
    fixture.write("control", b"\x01\x01a\nb\n");
    fixture.write("delete", b"\x7f\nb\n");
    let service = fixture.service();
    let job = fixture.job();
    for normalize_eol in [false, true] {
        let contexts = [
            fixture.context(CompareRef::Head),
            fixture.context(CompareRef::WorkingTree),
        ];
        let options = Options {
            normalize_eol,
            ignore_whitespace: false,
        };
        let old = legacy::prepare(
            &service,
            "fingerprint",
            1,
            contexts.clone(),
            options.clone(),
            &job,
        )
        .await
        .unwrap();
        let new = service
            .prepare("fingerprint", 1, contexts, options, &job)
            .await
            .unwrap();
        assert_eq!(
            equivalence::result(&new),
            equivalence::result(&old),
            "{setting}, normalize_eol={normalize_eol}"
        );
        new.close_readers().await;
        close_readers(&job.readers).await;
    }
}
