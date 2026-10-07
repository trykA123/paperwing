use super::*;
use crate::commit::test_fixture::{two_hunks, Fixture};

#[tokio::test]
async fn hunk_discard_content_preserves_other_hunk_and_index() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    let (before, after) = two_hunks();
    fixture.commit("file.txt", &before);
    fixture.write("file.txt", &after);
    let snapshot = snapshot::read(&fixture.path(), "file.txt", None, "unstaged")
        .await
        .unwrap();
    let request = stage::HunkRequest {
        file: "file.txt".into(),
        orig_path: None,
        area: "unstaged".into(),
        content_hash: snapshot.hash.clone(),
        hunks: vec![super::super::patch::Selection {
            hunk: 0,
            ranges: None,
        }],
    };
    let built = stage::build(&snapshot, &request, true)
        .unwrap()
        .content
        .unwrap();
    assert!(!built
        .windows(b"first change".len())
        .any(|line| line == b"first change"));
    assert!(built
        .windows(b"second change".len())
        .any(|line| line == b"second change"));
    assert_eq!(fixture.git(&["show", ":file.txt"]), before);
    fixture.write("file.txt", b"later\n");
    let current = snapshot::read(&fixture.path(), "file.txt", None, "unstaged")
        .await
        .unwrap();
    assert!(snapshot::require_hash(&current, &request.content_hash).is_err());
}

#[tokio::test]
async fn renamed_file_discard_accepts_the_snapshot_from_its_diff() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit("old name.txt", b"one\r\ntwo\r\nlast");
    fixture.git(&["mv", "old name.txt", "new name.txt"]);
    fixture.write("new name.txt", b"ONE\r\ntwo\r\nlast");
    let diff = crate::commit::change_hunks(
        fixture.path(),
        "new name.txt".into(),
        Some("old name.txt".into()),
        "unstaged".into(),
    )
    .await
    .unwrap();
    let request = DiscardFile {
        file: "new name.txt".into(),
        orig_path: Some("old name.txt".into()),
        content_hash: diff.content_hash,
    };
    let snapshot = read_file(&fixture.path(), &request).await.unwrap();
    assert_eq!(snapshot.index.unwrap(), b"one\r\ntwo\r\nlast");
    assert_eq!(snapshot.working.unwrap(), b"ONE\r\ntwo\r\nlast");
}
