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
