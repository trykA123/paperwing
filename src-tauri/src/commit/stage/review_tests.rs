use super::*;
use crate::commit::{change_content, change_hunks, test_fixture::Fixture};

fn crlf_fixture(attributes: bool) -> Fixture {
    let fixture = Fixture::new();
    if attributes {
        fixture.commit(".gitattributes", b"* text eol=crlf\n");
    } else {
        fixture.git(&["config", "core.autocrlf", "true"]);
    }
    fixture.commit("file.txt", b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\n");
    fixture.write("file.txt", b"one\r\nTWO\r\nthree\r\nfour\r\nfive\r\n");
    fixture
}

async fn crlf_hunks(attributes: bool) {
    let _serial = crate::test_support::serial().await;
    let fixture = crlf_fixture(attributes);
    let diff = change_hunks(fixture.path(), "file.txt".into(), None, "unstaged".into())
        .await
        .unwrap();
    assert_eq!(diff.hunks.len(), 1);
    let value = serde_json::to_value(&diff.hunks[0]).unwrap();
    let changed = value["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|line| line["kind"] != "context")
        .count();
    assert_eq!(changed, 2);
}

async fn crlf_stage(attributes: bool) {
    let _serial = crate::test_support::serial().await;
    let fixture = crlf_fixture(attributes);
    let request = tests::request(&fixture, "file.txt", "unstaged", 0).await;
    stage_hunks(fixture.path(), request).await.unwrap();
    assert_eq!(
        fixture.git(&["show", ":file.txt"]),
        b"one\nTWO\nthree\nfour\nfive\n"
    );
    assert_eq!(
        fixture.git(&["diff", "--cached", "--numstat"]),
        b"1\t1\tfile.txt\n"
    );
    assert_eq!(
        std::fs::read(fixture.root.join("file.txt")).unwrap(),
        b"one\r\nTWO\r\nthree\r\nfour\r\nfive\r\n"
    );
}

#[tokio::test]
async fn autocrlf_lists_only_the_edited_line() {
    crlf_hunks(false).await;
}

#[tokio::test]
async fn text_eol_crlf_lists_only_the_edited_line() {
    crlf_hunks(true).await;
}

#[tokio::test]
async fn autocrlf_stages_only_the_edited_line_in_index_form() {
    crlf_stage(false).await;
}

#[tokio::test]
async fn text_eol_crlf_stages_only_the_edited_line_in_index_form() {
    crlf_stage(true).await;
}

#[tokio::test]
async fn change_content_uses_index_form_for_crlf_working_content() {
    let _serial = crate::test_support::serial().await;
    let fixture = crlf_fixture(true);
    let content = change_content(fixture.path(), "file.txt".into(), None, "unstaged".into())
        .await
        .unwrap();
    assert_eq!(content.original, "one\ntwo\nthree\nfour\nfive\n");
    assert_eq!(content.modified, "one\nTWO\nthree\nfour\nfive\n");
}

#[tokio::test]
async fn filter_attributes_refuse_hunk_staging_before_index_changes() {
    let _serial = crate::test_support::serial().await;
    for attribute in ["filter=lfs", "filter=custom", "filter"] {
        let fixture = Fixture::new();
        fixture.commit("file.txt", b"before\n");
        fixture.write("file.txt", b"after\n");
        let request = tests::request(&fixture, "file.txt", "unstaged", 0).await;
        fixture.write(
            ".gitattributes",
            format!("file.txt {attribute}\n").as_bytes(),
        );
        let error = stage_hunks(fixture.path(), request).await.unwrap_err();
        assert!(error.contains("filter"), "{error}");
        assert_eq!(fixture.git(&["show", ":file.txt"]), b"before\n");
    }
}

#[tokio::test]
async fn partial_unstage_keeps_the_staged_rename() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    let before = (0..24)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    let after = before
        .replace("line 2\n", "FIRST\n")
        .replace("line 20\n", "SECOND\n");
    fixture.commit("old name.txt", before.as_bytes());
    fixture.git(&["mv", "old name.txt", "new name.txt"]);
    fixture.write("new name.txt", after.as_bytes());
    fixture.git(&["add", "new name.txt"]);
    let diff = change_hunks(
        fixture.path(),
        "new name.txt".into(),
        Some("old name.txt".into()),
        "staged".into(),
    )
    .await
    .unwrap();
    unstage_hunks(
        fixture.path(),
        HunkRequest {
            file: "new name.txt".into(),
            orig_path: Some("old name.txt".into()),
            area: "staged".into(),
            content_hash: diff.content_hash,
            hunks: vec![patch::Selection {
                hunk: 0,
                ranges: None,
            }],
        },
    )
    .await
    .unwrap();
    assert_eq!(fixture.git(&["ls-files"]), b"new name.txt\n");
    assert_eq!(
        fixture.git(&["show", ":new name.txt"]),
        before.replace("line 20\n", "SECOND\n").into_bytes()
    );
    assert!(fixture
        .git(&["diff", "--cached", "--name-status", "-M"])
        .starts_with(b"R"));
    assert_eq!(
        std::fs::read(fixture.root.join("new name.txt")).unwrap(),
        after.as_bytes()
    );
}

#[tokio::test]
async fn one_mebibyte_file_stages_one_hunk_with_three_lines_of_context() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    let before = (0..16384)
        .map(|line| format!("{line:05} {}\n", "x".repeat(57)))
        .collect::<String>();
    assert_eq!(before.len(), 1024 * 1024);
    let after = before.replace(
        &format!("08192 {}\n", "x".repeat(57)),
        &format!("08192 {}\n", "y".repeat(57)),
    );
    fixture.commit("file.txt", before.as_bytes());
    fixture.write("file.txt", after.as_bytes());
    let request = tests::request(&fixture, "file.txt", "unstaged", 0).await;
    let snapshot = snapshot::read(&fixture.path(), "file.txt", None, "unstaged")
        .await
        .unwrap();
    let built = build(&snapshot, &request, false).unwrap();
    assert!(
        built.patch.len() < 1024,
        "patch size: {}",
        built.patch.len()
    );
    stage_hunks(fixture.path(), request).await.unwrap();
    assert_eq!(fixture.git(&["show", ":file.txt"]), after.as_bytes());
    assert_eq!(
        fixture.git(&["diff", "--cached", "--numstat"]),
        b"1\t1\tfile.txt\n"
    );
}

#[tokio::test]
async fn external_status_stat_refresh_does_not_invalidate_stage() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit("other.txt", b"other\n");
    fixture.commit("file.txt", b"before\n");
    fixture.write("file.txt", b"after\n");
    let request = tests::request(&fixture, "file.txt", "unstaged", 0).await;
    let raw_index = std::fs::read(fixture.root.join(".git/index")).unwrap();
    fixture.write("other.txt", b"other\n");
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
    std::fs::File::open(fixture.root.join("other.txt"))
        .unwrap()
        .set_modified(later)
        .unwrap();
    fixture.git(&["status", "--porcelain"]);
    assert_ne!(
        std::fs::read(fixture.root.join(".git/index")).unwrap(),
        raw_index
    );
    stage_hunks(fixture.path(), request).await.unwrap();
    assert_eq!(fixture.git(&["show", ":file.txt"]), b"after\n");
}

#[tokio::test]
async fn changed_same_path_index_entry_still_refuses_stage() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit("file.txt", b"before\n");
    fixture.write("file.txt", b"after\n");
    let request = tests::request(&fixture, "file.txt", "unstaged", 0).await;
    let oid = fixture.git(&["rev-parse", ":file.txt"]);
    let oid = std::str::from_utf8(&oid).unwrap().trim();
    fixture.git(&["update-index", "--cacheinfo", "100755", oid, "file.txt"]);
    assert!(stage_hunks(fixture.path(), request)
        .await
        .unwrap_err()
        .contains("changed"));
    assert_eq!(fixture.git(&["show", ":file.txt"]), b"before\n");
}

#[tokio::test]
async fn one_mebibyte_intent_to_add_file_supports_partial_staging() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit("base.txt", b"base\n");
    let line = "0123456789abcdef0123456789abcdef\n";
    let content = line.repeat(32 * 1024).into_bytes();
    fixture.write("large.txt", &content);
    fixture.git(&["add", "-N", "large.txt"]);

    let diff = change_hunks(fixture.path(), "large.txt".into(), None, "unstaged".into())
        .await
        .unwrap();
    assert_eq!(diff.hunks.len(), 1);
    let request = tests::request(&fixture, "large.txt", "unstaged", 0).await;
    stage_hunks(fixture.path(), request).await.unwrap();
    assert_eq!(fixture.git(&["show", ":large.txt"]), content);
}

#[tokio::test]
async fn crlf_already_in_index_retains_crlf_and_shows_one_changed_line() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.git(&["config", "core.autocrlf", "false"]);
    fixture.commit("file.txt", b"a\r\nb\r\nc\r\n");
    fixture.git(&["config", "core.autocrlf", "true"]);
    fixture.write("file.txt", b"a\r\nB\r\nc\r\n");

    let diff = change_hunks(fixture.path(), "file.txt".into(), None, "unstaged".into())
        .await
        .unwrap();
    assert_eq!(diff.hunks.len(), 1);
    let value = serde_json::to_value(&diff.hunks[0]).unwrap();
    let changed: Vec<_> = value["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|line| line["kind"] != "context")
        .collect();
    assert_eq!(changed.len(), 2);
    assert_eq!(changed[0]["kind"], "remove");
    assert_eq!(changed[0]["text"], "b\r");
    assert_eq!(changed[1]["kind"], "add");
    assert_eq!(changed[1]["text"], "B\r");

    let request = tests::request(&fixture, "file.txt", "unstaged", 0).await;
    stage_hunks(fixture.path(), request).await.unwrap();
    assert_eq!(fixture.git(&["show", ":file.txt"]), b"a\r\nB\r\nc\r\n");

    fixture.git(&["reset", "--hard", "HEAD"]);
    fixture.write("file.txt", b"a\r\nB\r\nc\r\n");
    let snapshot = snapshot::read(&fixture.path(), "file.txt", None, "unstaged")
        .await
        .unwrap();
    let discard_request = HunkRequest {
        file: "file.txt".into(),
        orig_path: None,
        area: "unstaged".into(),
        content_hash: snapshot.hash.clone(),
        hunks: vec![crate::commit::patch::Selection {
            hunk: 0,
            ranges: None,
        }],
    };
    let restored = crate::commit::discard::hunk_bytes(&fixture.path(), &snapshot, &discard_request)
        .await
        .unwrap();
    assert_eq!(restored, b"a\r\nb\r\nc\r\n");
}

#[tokio::test]
async fn clean_and_diff_do_not_grow_repository_objects_and_work_on_read_only_objects_dir() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit("file.txt", b"before\n");
    fixture.write("file.txt", b"modified content\n");

    let objects_dir = fixture.root.join(".git/objects");

    fn count_objects(dir: &std::path::Path) -> usize {
        let mut count = 0;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.len() == 2 && entry.path().is_dir() {
                    if let Ok(sub) = std::fs::read_dir(entry.path()) {
                        count += sub.count();
                    }
                }
            }
        }
        count
    }

    let before_count = count_objects(&objects_dir);

    struct PermGuard(std::path::PathBuf);
    impl Drop for PermGuard {
        fn drop(&mut self) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
            }
        }
    }
    let _perm_guard = PermGuard(objects_dir.clone());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&objects_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    }

    let content = change_content(fixture.path(), "file.txt".into(), None, "unstaged".into())
        .await
        .unwrap();
    assert_eq!(content.original, "before\n");
    assert_eq!(content.modified, "modified content\n");

    let diff = change_hunks(fixture.path(), "file.txt".into(), None, "unstaged".into())
        .await
        .unwrap();
    assert_eq!(diff.hunks.len(), 1);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&objects_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let after_count = count_objects(&objects_dir);
    assert_eq!(
        before_count, after_count,
        "object count grew in repo objects dir"
    );
}

#[tokio::test]
async fn text_auto_crlf_index_one_edited_line_is_one_hunk_and_stage_discard_preserve_bytes() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit(".gitattributes", b"* text=auto\n");
    let before = b"line 1\r\nline 2\r\nline 3\r\n";
    let oid = fixture.git_input(&["hash-object", "-w", "--stdin", "--no-filters"], before);
    let oid_str = std::str::from_utf8(&oid).unwrap().trim();
    fixture.git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        "100644",
        oid_str,
        "file.txt",
    ]);
    fixture.git(&["commit", "-m", "init-crlf-auto"]);
    assert_eq!(fixture.git(&["show", ":file.txt"]), before);

    let edited = b"line 1\r\nLINE 2\r\nline 3\r\n";
    fixture.write("file.txt", edited);

    let diff = change_hunks(fixture.path(), "file.txt".into(), None, "unstaged".into())
        .await
        .unwrap();
    assert_eq!(
        diff.hunks.len(),
        1,
        "Expected exactly 1 hunk for 1 edited line"
    );

    let request = tests::request(&fixture, "file.txt", "unstaged", 0).await;
    stage_hunks(fixture.path(), request).await.unwrap();
    assert_eq!(fixture.git(&["show", ":file.txt"]), edited);

    let unstage_req = tests::request(&fixture, "file.txt", "staged", 0).await;
    unstage_hunks(fixture.path(), unstage_req).await.unwrap();
    assert_eq!(fixture.git(&["show", ":file.txt"]), before);

    let snapshot = snapshot::read(&fixture.path(), "file.txt", None, "unstaged")
        .await
        .unwrap();
    let discard_request = HunkRequest {
        file: "file.txt".into(),
        orig_path: None,
        area: "unstaged".into(),
        content_hash: snapshot.hash.clone(),
        hunks: vec![crate::commit::patch::Selection {
            hunk: 0,
            ranges: None,
        }],
    };
    let restored = crate::commit::discard::hunk_bytes(&fixture.path(), &snapshot, &discard_request)
        .await
        .unwrap();
    assert_eq!(restored, before);
}

#[tokio::test]
async fn working_tree_encoding_and_ident_refuse_hunk_actions_and_fall_back_in_diff() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit("utf16.txt", b"before\n");
    fixture.write("utf16.txt", b"after\n");
    fixture.commit("ident.txt", b"before $\n");
    fixture.write("ident.txt", b"after $\n");
    fixture.commit(
        ".gitattributes",
        b"utf16.txt working-tree-encoding=UTF-16\nident.txt ident\n",
    );

    let utf16_content = change_content(fixture.path(), "utf16.txt".into(), None, "unstaged".into())
        .await
        .unwrap();
    assert_eq!(utf16_content.modified, "after\n");

    let ident_content = change_content(fixture.path(), "ident.txt".into(), None, "unstaged".into())
        .await
        .unwrap();
    assert_eq!(ident_content.modified, "after $\n");

    let err_stage_utf16 = stage_hunks(
        fixture.path(),
        HunkRequest {
            file: "utf16.txt".into(),
            orig_path: None,
            area: "unstaged".into(),
            content_hash: "dummy".into(),
            hunks: vec![],
        },
    )
    .await
    .unwrap_err();
    assert!(
        err_stage_utf16.contains("working-tree-encoding"),
        "{err_stage_utf16}"
    );

    let err_stage_ident = stage_hunks(
        fixture.path(),
        HunkRequest {
            file: "ident.txt".into(),
            orig_path: None,
            area: "unstaged".into(),
            content_hash: "dummy".into(),
            hunks: vec![],
        },
    )
    .await
    .unwrap_err();
    assert!(err_stage_ident.contains("ident"), "{err_stage_ident}");
}

#[cfg(unix)]
#[tokio::test]
async fn colon_in_repo_path_works_with_alternate_object_dir() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::with_root_name("colon:repo:test");
    fixture.commit("file.txt", b"first line\n");
    fixture.write("file.txt", b"modified in colon path\n");

    let diff = change_hunks(fixture.path(), "file.txt".into(), None, "unstaged".into())
        .await
        .unwrap();
    assert_eq!(diff.hunks.len(), 1);

    let request = tests::request(&fixture, "file.txt", "unstaged", 0).await;
    stage_hunks(fixture.path(), request).await.unwrap();
    assert_eq!(
        fixture.git(&["show", ":file.txt"]),
        b"modified in colon path\n"
    );
}

#[tokio::test]
async fn stage_holding_path_completes_within_timeout_without_deadlock() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit("file.txt", b"before\n");
    fixture.write("file.txt", b"after\n");

    let request = tests::request(&fixture, "file.txt", "unstaged", 0).await;
    tokio::time::timeout(
        std::time::Duration::from_secs(20),
        stage_hunks(fixture.path(), request),
    )
    .await
    .expect("stage_hunks deadlocked")
    .unwrap();
}
