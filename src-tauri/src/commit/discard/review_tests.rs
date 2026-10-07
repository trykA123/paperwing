use super::*;
use crate::commit::{change_hunks, test_fixture::Fixture};

async fn crlf_discard(attributes: bool) {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    if attributes {
        fixture.commit(".gitattributes", b"* text eol=crlf\n");
    } else {
        fixture.git(&["config", "core.autocrlf", "true"]);
    }
    let before = b"one\r\ntwo\r\nlast";
    fixture.commit("file.txt", before);
    fixture.write("file.txt", b"one\r\nTWO\r\nlast");
    let diff = change_hunks(fixture.path(), "file.txt".into(), None, "unstaged".into())
        .await
        .unwrap();
    let plan = prepare_file(
        &fixture.path(),
        &DiscardFile {
            file: "file.txt".into(),
            orig_path: None,
            content_hash: diff.content_hash,
        },
    )
    .await
    .unwrap();
    let DiscardPlan::Restore(write) = plan else {
        panic!("Tracked discard must restore the index");
    };
    assert_eq!(write.bytes, before);
    assert_eq!(fixture.git(&["show", ":file.txt"]), b"one\ntwo\nlast");
}

#[tokio::test]
async fn autocrlf_discard_restores_smudged_index_bytes() {
    crlf_discard(false).await;
}

#[tokio::test]
async fn text_eol_crlf_discard_restores_smudged_index_bytes() {
    crlf_discard(true).await;
}

#[tokio::test]
async fn filter_attributes_refuse_discard_before_writing() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit("file.txt", b"before\n");
    fixture.write("file.txt", b"after\n");
    let diff = change_hunks(fixture.path(), "file.txt".into(), None, "unstaged".into())
        .await
        .unwrap();
    fixture.write(".gitattributes", b"file.txt filter=custom\n");
    let error = read_file(
        &fixture.path(),
        &DiscardFile {
            file: "file.txt".into(),
            orig_path: None,
            content_hash: diff.content_hash,
        },
    )
    .await
    .err()
    .unwrap();
    assert!(error.contains("filter"), "{error}");
    assert_eq!(
        std::fs::read(fixture.root.join("file.txt")).unwrap(),
        b"after\n"
    );
}

#[tokio::test]
async fn intent_to_add_discard_has_no_index_version_to_overwrite_with() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit("base.txt", b"base\n");
    fixture.write("file.txt", b"keep these bytes\r\n");
    fixture.git(&["add", "-N", "file.txt"]);
    let diff = change_hunks(fixture.path(), "file.txt".into(), None, "unstaged".into())
        .await
        .unwrap();
    let snapshot = read_file(
        &fixture.path(),
        &DiscardFile {
            file: "file.txt".into(),
            orig_path: None,
            content_hash: diff.content_hash,
        },
    )
    .await
    .unwrap();
    assert!(snapshot.index.is_none());
    assert_eq!(snapshot.working.unwrap(), b"keep these bytes\r\n");
    let plan = prepare_file(
        &fixture.path(),
        &DiscardFile {
            file: "file.txt".into(),
            orig_path: None,
            content_hash: snapshot.hash,
        },
    )
    .await
    .unwrap();
    let DiscardPlan::Trash(write) = plan else {
        panic!("Intent-to-add discard must use Trash");
    };
    assert_eq!(write.expected.unwrap(), b"keep these bytes\r\n");
}

#[tokio::test]
async fn hunk_discard_preserves_mixed_lf_and_crlf_line_endings_of_kept_lines() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.git(&["config", "core.autocrlf", "true"]);
    let before = b"line 1\nline 2\r\nline 3\nline 4\r\n";
    fixture.commit("file.txt", before);
    let after = b"line 1\nLINE 2\r\nline 3\nline 4\r\n";
    fixture.write("file.txt", after);

    let snapshot = snapshot::read(&fixture.path(), "file.txt", None, "unstaged")
        .await
        .unwrap();
    let request = stage::HunkRequest {
        file: "file.txt".into(),
        orig_path: None,
        area: "unstaged".into(),
        content_hash: snapshot.hash.clone(),
        hunks: vec![crate::commit::patch::Selection {
            hunk: 0,
            ranges: None,
        }],
    };
    let bytes = hunk_bytes(&fixture.path(), &snapshot, &request)
        .await
        .unwrap();
    assert_eq!(bytes, before);
}

#[tokio::test]
async fn autocrlf_false_mixed_file_discarding_deleted_lf_restores_lf() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.git(&["config", "core.autocrlf", "false"]);
    let before = b"header\r\nfoo\nfooter\r\n";
    fixture.commit("mixed.txt", before);
    let after = b"header\r\nfooter\r\n";
    fixture.write("mixed.txt", after);

    let snapshot = snapshot::read(&fixture.path(), "mixed.txt", None, "unstaged")
        .await
        .unwrap();
    let request = stage::HunkRequest {
        file: "mixed.txt".into(),
        orig_path: None,
        area: "unstaged".into(),
        content_hash: snapshot.hash.clone(),
        hunks: vec![crate::commit::patch::Selection {
            hunk: 0,
            ranges: None,
        }],
    };
    let bytes = hunk_bytes(&fixture.path(), &snapshot, &request)
        .await
        .unwrap();
    assert_eq!(bytes, before);
}

#[tokio::test]
async fn autocrlf_true_discarding_deleted_line_restores_crlf() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.git(&["config", "core.autocrlf", "true"]);
    fixture.commit("file.txt", b"foo\n");
    fixture.write("file.txt", b"bar\r\n");

    let snapshot = snapshot::read(&fixture.path(), "file.txt", None, "unstaged")
        .await
        .unwrap();
    let request = stage::HunkRequest {
        file: "file.txt".into(),
        orig_path: None,
        area: "unstaged".into(),
        content_hash: snapshot.hash.clone(),
        hunks: vec![crate::commit::patch::Selection {
            hunk: 0,
            ranges: None,
        }],
    };
    let bytes = hunk_bytes(&fixture.path(), &snapshot, &request)
        .await
        .unwrap();
    assert_eq!(bytes, b"foo\r\n");
}

#[tokio::test]
async fn multiple_hunks_with_different_conventions_preserve_their_own_endings() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.git(&["config", "core.autocrlf", "false"]);
    let before = b"c1\nc2\nc3\nc4\nfoo_lf\nm1\nm2\nm3\nm4\nm5\nm6\nm7\nm8\nm9\nm10\nm11\nm12\nbar_crlf\r\ne1\ne2\ne3\ne4\n";
    fixture.commit("multi.txt", before);
    let after =
        b"c1\nc2\nc3\nc4\nm1\nm2\nm3\nm4\nm5\nm6\nm7\nm8\nm9\nm10\nm11\nm12\ne1\ne2\ne3\ne4\n";
    fixture.write("multi.txt", after);

    let snapshot = snapshot::read(&fixture.path(), "multi.txt", None, "unstaged")
        .await
        .unwrap();
    let diff = change_hunks(fixture.path(), "multi.txt".into(), None, "unstaged".into())
        .await
        .unwrap();
    assert_eq!(diff.hunks.len(), 2);

    let req0 = stage::HunkRequest {
        file: "multi.txt".into(),
        orig_path: None,
        area: "unstaged".into(),
        content_hash: snapshot.hash.clone(),
        hunks: vec![crate::commit::patch::Selection {
            hunk: 0,
            ranges: None,
        }],
    };
    let bytes0 = hunk_bytes(&fixture.path(), &snapshot, &req0).await.unwrap();
    assert!(bytes0.windows(b"foo_lf\n".len()).any(|w| w == b"foo_lf\n"));
    assert!(!bytes0
        .windows(b"foo_lf\r\n".len())
        .any(|w| w == b"foo_lf\r\n"));

    let req1 = stage::HunkRequest {
        file: "multi.txt".into(),
        orig_path: None,
        area: "unstaged".into(),
        content_hash: snapshot.hash.clone(),
        hunks: vec![crate::commit::patch::Selection {
            hunk: 1,
            ranges: None,
        }],
    };
    let bytes1 = hunk_bytes(&fixture.path(), &snapshot, &req1).await.unwrap();
    assert!(bytes1
        .windows(b"bar_crlf\r\n".len())
        .any(|w| w == b"bar_crlf\r\n"));
}

#[tokio::test]
async fn autocrlf_true_mixed_cr_index_discard_deleted_line_restores_lf_and_clean_status() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.git(&["config", "core.autocrlf", "false"]);
    let before = b"a\r\nb\nc\r\n";
    fixture.commit("file.txt", before);
    fixture.git(&["config", "core.autocrlf", "true"]);
    let after = b"a\r\nc\r\n";
    fixture.write("file.txt", after);

    let snapshot = snapshot::read(&fixture.path(), "file.txt", None, "unstaged")
        .await
        .unwrap();
    let request = stage::HunkRequest {
        file: "file.txt".into(),
        orig_path: None,
        area: "unstaged".into(),
        content_hash: snapshot.hash.clone(),
        hunks: vec![crate::commit::patch::Selection {
            hunk: 0,
            ranges: None,
        }],
    };
    let bytes = hunk_bytes(&fixture.path(), &snapshot, &request)
        .await
        .unwrap();
    assert_eq!(bytes, before);
    fixture.write("file.txt", &bytes);
    assert_eq!(fixture.git(&["status", "--porcelain"]), b"");
    assert_eq!(fixture.git(&["diff"]), b"");
}

#[tokio::test]
async fn text_auto_core_eol_crlf_discard_deleted_line_restores_crlf() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit(".gitattributes", b"* text=auto\n");
    fixture.git(&["config", "core.eol", "crlf"]);
    let before = b"one\ntwo\nthree\n";
    fixture.commit("file.txt", before);
    let smudged = fixture.git(&["cat-file", "--filters", "--path=file.txt", ":0:file.txt"]);
    assert_eq!(smudged, b"one\r\ntwo\r\nthree\r\n");
    fixture.write("file.txt", b"one\r\nthree\r\n");

    let snapshot = snapshot::read(&fixture.path(), "file.txt", None, "unstaged")
        .await
        .unwrap();
    let request = stage::HunkRequest {
        file: "file.txt".into(),
        orig_path: None,
        area: "unstaged".into(),
        content_hash: snapshot.hash.clone(),
        hunks: vec![crate::commit::patch::Selection {
            hunk: 0,
            ranges: None,
        }],
    };
    let bytes = hunk_bytes(&fixture.path(), &snapshot, &request)
        .await
        .unwrap();
    assert_eq!(bytes, b"one\r\ntwo\r\nthree\r\n");
    fixture.write("file.txt", &bytes);
    assert_eq!(fixture.git(&["diff"]), b"");
}

#[tokio::test]
async fn minus_text_eol_crlf_discard_deleted_line_restores_lf() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit(".gitattributes", b"* -text eol=crlf\n");
    let before = b"one\ntwo\nthree\n";
    fixture.commit("file.txt", before);
    fixture.write("file.txt", b"one\nthree\n");

    let snapshot = snapshot::read(&fixture.path(), "file.txt", None, "unstaged")
        .await
        .unwrap();
    let request = stage::HunkRequest {
        file: "file.txt".into(),
        orig_path: None,
        area: "unstaged".into(),
        content_hash: snapshot.hash.clone(),
        hunks: vec![crate::commit::patch::Selection {
            hunk: 0,
            ranges: None,
        }],
    };
    let bytes = hunk_bytes(&fixture.path(), &snapshot, &request)
        .await
        .unwrap();
    assert_eq!(bytes, b"one\ntwo\nthree\n");
    fixture.write("file.txt", &bytes);
    assert_eq!(fixture.git(&["status", "--porcelain"]), b"");
    assert_eq!(fixture.git(&["diff"]), b"");
}

#[tokio::test]
async fn crlf_in_index_with_eol_lf_discard_deleted_line_restores_crlf() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit(".gitattributes", b"* text=auto eol=lf\n");
    let before = b"one\r\ntwo\r\nthree\r\n";
    fixture.git(&["config", "core.autocrlf", "false"]);
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
    fixture.git(&["commit", "-qm", "Add file with crlf in index"]);
    fixture.write("file.txt", b"one\r\nthree\r\n");

    let snapshot = snapshot::read(&fixture.path(), "file.txt", None, "unstaged")
        .await
        .unwrap();
    let request = stage::HunkRequest {
        file: "file.txt".into(),
        orig_path: None,
        area: "unstaged".into(),
        content_hash: snapshot.hash.clone(),
        hunks: vec![crate::commit::patch::Selection {
            hunk: 0,
            ranges: None,
        }],
    };
    let bytes = hunk_bytes(&fixture.path(), &snapshot, &request)
        .await
        .unwrap();
    assert_eq!(bytes, b"one\r\ntwo\r\nthree\r\n");
    fixture.write("file.txt", &bytes);
    assert_eq!(fixture.git(&["diff"]), b"");
}
