use super::*;

#[tokio::test]
async fn old_and_new_refuse_missing_index_objects_before_working_tree_comparison() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", b"base\n");
    fixture.commit("base").await;
    let object = "1".repeat(40);
    fixture
        .git(&[
            "update-index",
            "--add",
            "--cacheinfo",
            "100644",
            &object,
            "missing.txt",
        ])
        .await;
    fixture.write("missing.txt", b"raw bytes\n");
    equivalent(
        &fixture,
        &fixture,
        [CompareRef::Head, CompareRef::WorkingTree],
    )
    .await;
}

pub(super) fn result(prepared: &Prepared) -> serde_json::Value {
    let mut rows = serde_json::to_value(&prepared.rows).unwrap();
    for row in rows.as_array_mut().unwrap() {
        row.as_object_mut().unwrap().remove("id");
    }
    serde_json::json!({
        "raw": prepared.view.raw, "display": prepared.view.display,
        "history": prepared.view.history, "options": prepared.view.options, "files": rows,
    })
}

async fn equivalent(left: &Fixture, right: &Fixture, references: [CompareRef; 2]) {
    let service = left.service();
    for normalize_eol in [false, true] {
        for ignore_whitespace in [false, true] {
            let contexts = [
                left.context(references[0].clone()),
                right.context(references[1].clone()),
            ];
            let options = Options {
                normalize_eol,
                ignore_whitespace,
            };
            let job = left.job();
            let old = legacy::prepare(
                &service,
                "equivalence",
                1,
                contexts.clone(),
                options.clone(),
                &job,
            )
            .await;
            let new = service
                .prepare("equivalence", 1, contexts, options, &job)
                .await;
            match (old, new) {
                (Ok(old), Ok(new)) => {
                    assert_eq!(
                        serde_json::to_value(&new.view).unwrap(),
                        serde_json::to_value(&old.view).unwrap()
                    );
                    assert_eq!(result(&new), result(&old));
                    for (old_side, new_side) in [(&old.left, &new.left), (&old.right, &new.right)] {
                        for (path, old_entry) in &old_side.files {
                            if old_entry.kind == Kind::Directory {
                                continue;
                            }
                            let old_bytes = legacy::bytes(old_side, path, old_entry, &job).await;
                            let new_bytes =
                                content(new_side, path, &new_side.files[path], &job).await;
                            match (old_bytes, new_bytes) {
                                (Ok(old), Ok(new)) => assert_eq!(new, old, "{path}"),
                                (Err(old), Err(new)) => assert_eq!(
                                    serde_json::to_value(new).unwrap(),
                                    serde_json::to_value(old).unwrap(),
                                    "{path}"
                                ),
                                _ => panic!("Content availability changed: {path}"),
                            }
                        }
                    }
                    new.close_readers().await;
                }
                (Err(old), Err(new)) => assert_eq!(
                    serde_json::to_value(new).unwrap(),
                    serde_json::to_value(old).unwrap()
                ),
                _ => panic!("Comparison availability changed"),
            }
        }
    }
}

async fn adversarial(format: &str) -> (Fixture, String) {
    let fixture = Fixture::with_format(Some(format)).await;
    fixture.git(&["config", "core.autocrlf", "false"]).await;
    fixture
        .git(&["config", "diff.algorithm", "histogram"])
        .await;
    fixture.git(&["config", "diff.renameLimit", "50000"]).await;
    for (path, bytes) in [
        ("same.txt", b"same\n".as_slice()),
        ("deleted.txt", b"deleted\n"),
        ("renamed.txt", b"rename content\n"),
        ("binary", b"\0\xffold"),
        ("bom.txt", b"\xef\xbb\xbffirst\nalpha beta\n"),
        ("type", b"regular\n"),
    ] {
        fixture.write(path, bytes);
    }
    fixture.write("large.txt", &vec![b'x'; paths::CONTENT_LIMIT + 1]);
    #[cfg(unix)]
    std::os::unix::fs::symlink("same.txt", fixture.0.join("repo/link")).unwrap();
    let base = fixture.commit("base").await;
    fixture.git(&["rm", "deleted.txt"]).await;
    fixture.git(&["mv", "renamed.txt", "new-name.txt"]).await;
    fixture.write("binary", b"\0\xffnew");
    fixture.write("bom.txt", b"\xef\xbb\xbffirst\r\nalpha   beta\r\n");
    fixture.write("added.txt", b"added without newline");
    fixture.git(&["rm", "type"]).await;
    fixture.write("type/nested", b"nested\n");
    #[cfg(unix)]
    {
        std::fs::remove_file(fixture.0.join("repo/link")).unwrap();
        std::os::unix::fs::symlink("added.txt", fixture.0.join("repo/link")).unwrap();
    }
    fixture
        .git(&[
            "update-index",
            "--add",
            "--cacheinfo",
            "160000",
            &base,
            "module",
        ])
        .await;
    fixture.commit_staged("submodule").await;
    fixture.commit("changes").await;
    (fixture, base)
}

#[tokio::test]
async fn old_and_new_match_adversarial_refs_working_trees_and_object_formats() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    for format in ["sha1", "sha256"] {
        let (fixture, base) = adversarial(format).await;
        equivalent(
            &fixture,
            &fixture,
            [CompareRef::Commit { sha: base.clone() }, CompareRef::Head],
        )
        .await;
        equivalent(
            &fixture,
            &fixture,
            [CompareRef::Head, CompareRef::WorkingTree],
        )
        .await;
        equivalent(
            &fixture,
            &fixture,
            [CompareRef::WorkingTree, CompareRef::WorkingTree],
        )
        .await;
        fixture.write("same.txt", b"changed\r\n");
        equivalent(
            &fixture,
            &fixture,
            [CompareRef::Commit { sha: base }, CompareRef::WorkingTree],
        )
        .await;
    }
}

#[tokio::test]
async fn old_and_new_match_independent_repositories_including_mixed_formats() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let (left, _) = adversarial("sha1").await;
    for format in ["sha1", "sha256"] {
        let (right, _) = adversarial(format).await;
        right.write("same.txt", b"changed\n");
        right.commit("different").await;
        equivalent(&left, &right, [CompareRef::Head, CompareRef::Head]).await;
        equivalent(&left, &right, [CompareRef::Head, CompareRef::WorkingTree]).await;
    }
}

#[tokio::test]
async fn old_and_new_match_empty_trees_unborn_head_and_raw_checkout_filters() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    equivalent(
        &fixture,
        &fixture,
        [CompareRef::Head, CompareRef::WorkingTree],
    )
    .await;
    fixture
        .git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "empty",
        ])
        .await;
    equivalent(&fixture, &fixture, [CompareRef::Head, CompareRef::Head]).await;
    fixture.write(
        ".gitattributes",
        b"*.txt text eol=crlf\nfiltered filter=sentinel\n",
    );
    fixture.write("file.txt", b"\xef\xbb\xbffirst\nsecond\n");
    fixture.write("filtered", b"original\n");
    fixture.commit("text").await;
    fixture.git(&["config", "core.autocrlf", "true"]).await;
    std::fs::remove_file(fixture.0.join("repo/file.txt")).unwrap();
    fixture.git(&["checkout", "--", "file.txt"]).await;
    #[cfg(unix)]
    {
        fixture
            .git(&[
                "config",
                "filter.sentinel.clean",
                "echo ran >> filter-sentinel; cat",
            ])
            .await;
        fixture
            .git(&["config", "filter.sentinel.smudge", "cat"])
            .await;
    }
    fixture.write("filtered", b"changed\n");
    equivalent(
        &fixture,
        &fixture,
        [CompareRef::Head, CompareRef::WorkingTree],
    )
    .await;
    assert!(!fixture.0.join("repo/filter-sentinel").exists());
}

#[tokio::test]
async fn in_process_blob_ids_match_git_hash_object_without_filters() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    for format in ["sha1", "sha256"] {
        let fixture = Fixture::with_format(Some(format)).await;
        let object_format = ObjectFormat::read(&fixture.0.join("repo"), &fixture.job())
            .await
            .unwrap();
        for bytes in [
            b"".as_slice(),
            b"first\r\n\xef\xbb\xbf\0\xff",
            b"no newline",
        ] {
            let hash = fixture
                .job()
                .run_input(
                    &fixture.0.join("repo"),
                    &["hash-object", "--no-filters", "--stdin"],
                    &[0],
                    Some(bytes),
                )
                .await
                .unwrap();
            assert_eq!(
                object_format.blob(bytes),
                decode(&hash.stdout).unwrap().trim()
            );
        }
    }
}
