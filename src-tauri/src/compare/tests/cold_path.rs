use super::*;

#[tokio::test]
async fn git_clean_crlf_checkout_still_requires_raw_byte_comparison() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.git(&["config", "core.autocrlf", "false"]).await;
    fixture.write(".gitattributes", b"*.txt text eol=crlf\n");
    fixture.write("file.txt", b"first\nsecond\n");
    fixture.commit("base").await;
    std::fs::remove_file(fixture.0.join("repo/file.txt")).unwrap();
    fixture.git(&["checkout", "--", "file.txt"]).await;
    fixture.git(&["update-index", "--refresh"]).await;

    let status = fixture
        .git(&[
            "-c",
            "core.preloadIndex=true",
            "-c",
            "core.untrackedCache=true",
            "status",
            "--porcelain=v2",
            "-z",
        ])
        .await;
    let raw = fixture.git(&["diff", "--raw", "-z"]).await;
    assert!(status.is_empty());
    assert!(raw.is_empty());
    assert_eq!(
        std::fs::read(fixture.0.join("repo/file.txt")).unwrap(),
        b"first\r\nsecond\r\n"
    );

    for normalize_eol in [false, true] {
        let comparison = prepared(
            &fixture,
            CompareRef::Head,
            CompareRef::WorkingTree,
            Options {
                normalize_eol,
                ignore_whitespace: false,
            },
        )
        .await;
        let row = comparison
            .rows
            .iter()
            .find(|row| row.path == "file.txt")
            .unwrap();
        assert_eq!(row.raw_status, Status::Different);
        assert_eq!(
            row.raw_lines,
            Some(Lines {
                added: 2,
                removed: 2,
            })
        );
        assert_eq!(
            row.display_status,
            if normalize_eol {
                Status::Same
            } else {
                Status::Different
            }
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn working_status_and_raw_diff_execute_clean_filters_that_comparison_must_not_run() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.git(&["config", "core.autocrlf", "false"]).await;
    fixture.write(".gitattributes", b"*.txt filter=sentinel\n");
    fixture.write("file.txt", b"old\n");
    fixture.commit("base").await;
    fixture
        .git(&[
            "config",
            "filter.sentinel.clean",
            "echo ran >> filter-sentinel; cat",
        ])
        .await;
    fixture
        .git(&["config", "filter.sentinel.required", "true"])
        .await;
    fixture.write("file.txt", b"new\n");
    std::fs::File::open(fixture.0.join("repo/file.txt"))
        .unwrap()
        .set_times(
            std::fs::FileTimes::new()
                .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(86_400)),
        )
        .unwrap();

    let comparison = prepared(
        &fixture,
        CompareRef::Head,
        CompareRef::WorkingTree,
        Options::default(),
    )
    .await;
    let row = comparison
        .rows
        .iter()
        .find(|row| row.path == "file.txt")
        .unwrap();
    assert_eq!(row.raw_status, Status::Different);
    let sentinel = fixture.0.join("repo/filter-sentinel");
    assert!(!sentinel.exists());

    let status = fixture
        .git(&[
            "-c",
            "core.preloadIndex=true",
            "-c",
            "core.untrackedCache=true",
            "status",
            "--porcelain=v2",
            "-z",
        ])
        .await;
    assert!(!status.is_empty());
    let status_filter_bytes = std::fs::metadata(&sentinel).unwrap().len();
    assert!(status_filter_bytes > 0);

    let raw = fixture
        .git(&[
            "-c",
            "core.preloadIndex=true",
            "-c",
            "core.untrackedCache=true",
            "diff",
            "--raw",
            "-z",
            "--no-ext-diff",
            "--no-textconv",
        ])
        .await;
    assert!(!raw.is_empty());
    assert!(std::fs::metadata(&sentinel).unwrap().len() > status_filter_bytes);
}

#[tokio::test]
async fn cross_format_tree_comparison_preserves_raw_and_normalized_results() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let left = Fixture::with_format(Some("sha1")).await;
    let right = Fixture::with_format(Some("sha256")).await;
    left.write("same.txt", b"equal\n");
    right.write("same.txt", b"equal\n");
    left.write("changed.txt", b"first\nalpha beta\n");
    right.write("changed.txt", b"first\r\nalpha   beta\r\n");
    left.commit("left").await;
    right.commit("right").await;
    for normalize_eol in [false, true] {
        for ignore_whitespace in [false, true] {
            let comparison = left
                .service()
                .prepare(
                    "mixed-format",
                    1,
                    [
                        left.context(CompareRef::Head),
                        right.context(CompareRef::Head),
                    ],
                    Options {
                        normalize_eol,
                        ignore_whitespace,
                    },
                    &left.job(),
                )
                .await
                .unwrap();
            let same = comparison
                .rows
                .iter()
                .find(|row| row.path == "same.txt")
                .unwrap();
            assert_eq!(same.raw_status, Status::Same);
            assert_eq!(
                same.raw_lines,
                Some(Lines {
                    added: 0,
                    removed: 0
                })
            );
            let changed = comparison
                .rows
                .iter()
                .find(|row| row.path == "changed.txt")
                .unwrap();
            assert_eq!(changed.raw_status, Status::Different);
            assert_eq!(
                changed.raw_lines,
                Some(Lines {
                    added: 2,
                    removed: 2
                })
            );
            assert_eq!(
                changed.display_status,
                if normalize_eol && ignore_whitespace {
                    Status::Same
                } else {
                    Status::Different
                }
            );
            assert_eq!(
                changed.display_lines,
                Some(if normalize_eol && ignore_whitespace {
                    Lines {
                        added: 0,
                        removed: 0,
                    }
                } else if normalize_eol {
                    Lines {
                        added: 1,
                        removed: 1,
                    }
                } else {
                    Lines {
                        added: 2,
                        removed: 2,
                    }
                })
            );
            assert_eq!(
                comparison.view.history.reason.as_deref(),
                Some("historyObjectsUnavailable")
            );
            for side in [&comparison.left, &comparison.right] {
                let entry = &side.files["changed.txt"];
                let bytes = content(side, "changed.txt", entry, &left.job())
                    .await
                    .unwrap();
                assert_eq!(
                    bytes,
                    if side.commit == comparison.left.commit {
                        b"first\nalpha beta\n".as_slice()
                    } else {
                        b"first\r\nalpha   beta\r\n".as_slice()
                    }
                );
            }
        }
    }
}

#[tokio::test]
async fn one_git_object_database_cannot_diff_or_batch_read_independent_hash_formats() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let left = Fixture::with_format(Some("sha1")).await;
    let right = Fixture::with_format(Some("sha256")).await;
    left.write("changed.txt", b"left\n");
    right.write("changed.txt", b"right\n");
    let commits = [left.commit("left").await, right.commit("right").await];
    let objects = [
        decode(&left.git(&["rev-parse", "HEAD:changed.txt"]).await)
            .unwrap()
            .trim()
            .to_string(),
        decode(&right.git(&["rev-parse", "HEAD:changed.txt"]).await)
            .unwrap()
            .trim()
            .to_string(),
    ];
    assert_eq!(objects[0].len(), 40);
    assert_eq!(objects[1].len(), 64);
    for (host, alternate) in [(&left, &right), (&right, &left)] {
        let host_is_sha1 = host.0 == left.0;
        let root = host.0.join("repo");
        let alternate = alternate.0.join("repo/.git/objects");
        let commits = commits.clone();
        let objects = objects.clone();
        tokio::task::spawn_blocking(move || {
            let command = |args: &[&str], input: Option<String>| {
                let mut child = std::process::Command::new("git")
                    .current_dir(&root)
                    .env("GIT_ALTERNATE_OBJECT_DIRECTORIES", &alternate)
                    .args(args)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .unwrap();
                if let Some(input) = input {
                    use std::io::Write;
                    child
                        .stdin
                        .take()
                        .unwrap()
                        .write_all(input.as_bytes())
                        .unwrap();
                } else {
                    drop(child.stdin.take());
                }
                child.wait_with_output().unwrap()
            };
            for format in ["--raw", "--numstat"] {
                let output = command(
                    &[
                        "diff-tree",
                        "-r",
                        "-z",
                        format,
                        "-M",
                        &commits[0],
                        &commits[1],
                    ],
                    None,
                );
                assert!(!output.status.success());
                assert!(output.stdout.is_empty());
            }
            let output = command(&["cat-file", "--batch"], Some(objects.join("\n") + "\n"));
            assert!(output.status.success());
            let expected = format!(
                "{} missing\n",
                if host_is_sha1 {
                    &objects[1]
                } else {
                    &objects[0]
                }
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains(&expected));
            let local = if host_is_sha1 {
                format!("{} blob 5\nleft\n\n", objects[0])
            } else {
                format!("{} blob 6\nright\n\n", objects[1])
            };
            assert!(String::from_utf8_lossy(&output.stdout).contains(&local));
        })
        .await
        .unwrap();
    }
}
