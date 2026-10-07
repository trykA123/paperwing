use super::*;
use crate::commit::{
    change_hunks,
    test_fixture::{two_hunks, Fixture},
};

pub(crate) async fn request(fixture: &Fixture, file: &str, area: &str, hunk: usize) -> HunkRequest {
    let diff = change_hunks(fixture.path(), file.into(), None, area.into())
        .await
        .unwrap();
    HunkRequest {
        file: file.into(),
        orig_path: None,
        area: area.into(),
        content_hash: diff.content_hash,
        hunks: vec![patch::Selection { hunk, ranges: None }],
    }
}

#[tokio::test]
async fn one_of_two_hunks_commits_and_the_other_stays_in_working_tree() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    let (before, after) = two_hunks();
    fixture.commit("file.txt", &before);
    fixture.write("file.txt", &after);
    let request = request(&fixture, "file.txt", "unstaged", 0).await;
    stage_hunks(fixture.path(), request).await.unwrap();
    crate::commit::commit_staged(fixture.path(), "first hunk".into())
        .await
        .unwrap();
    let head = fixture.git(&["show", "HEAD:file.txt"]);
    assert!(head
        .windows(b"first change".len())
        .any(|line| line == b"first change"));
    assert!(!head
        .windows(b"second change".len())
        .any(|line| line == b"second change"));
    assert_eq!(std::fs::read(fixture.root.join("file.txt")).unwrap(), after);
    assert!(fixture
        .git(&["diff"])
        .windows(b"second change".len())
        .any(|line| line == b"second change"));
}

#[tokio::test]
async fn unstage_hunk_leaves_other_staged_hunk_and_working_bytes() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    let (before, after) = two_hunks();
    fixture.commit("file.txt", &before);
    fixture.write("file.txt", &after);
    fixture.git(&["add", "file.txt"]);
    let request = request(&fixture, "file.txt", "staged", 0).await;
    unstage_hunks(fixture.path(), request).await.unwrap();
    let index = fixture.git(&["show", ":file.txt"]);
    assert!(!index
        .windows(b"first change".len())
        .any(|line| line == b"first change"));
    assert!(index
        .windows(b"second change".len())
        .any(|line| line == b"second change"));
    assert_eq!(std::fs::read(fixture.root.join("file.txt")).unwrap(), after);
}

#[tokio::test]
async fn changed_working_bytes_or_index_refuse_stage_and_unstage() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit("file.txt", b"before\n");
    fixture.write("file.txt", b"after\n");
    let stale = request(&fixture, "file.txt", "unstaged", 0).await;
    fixture.write("file.txt", b"later\n");
    assert!(stage_hunks(fixture.path(), stale)
        .await
        .unwrap_err()
        .contains("changed"));
    assert!(fixture.git(&["diff", "--cached"]).is_empty());
    fixture.git(&["add", "file.txt"]);
    let stale = request(&fixture, "file.txt", "staged", 0).await;
    fixture.write("file.txt", b"newer\n");
    assert!(unstage_hunks(fixture.path(), stale)
        .await
        .unwrap_err()
        .contains("changed"));
    let stale = request(&fixture, "file.txt", "staged", 0).await;
    fixture.git(&["add", "file.txt"]);
    assert!(unstage_hunks(fixture.path(), stale)
        .await
        .unwrap_err()
        .contains("changed"));
    assert_eq!(fixture.git(&["show", ":file.txt"]), b"newer\n");
}

#[tokio::test]
async fn added_unborn_file_and_staged_rename_support_hunk_commands() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.write("old name.txt", b"one\r\ntwo\r\nthree\r\nfour");
    stage_hunks(
        fixture.path(),
        request(&fixture, "old name.txt", "untracked", 0).await,
    )
    .await
    .unwrap();
    unstage_hunks(
        fixture.path(),
        request(&fixture, "old name.txt", "staged", 0).await,
    )
    .await
    .unwrap();
    assert!(fixture.git(&["ls-files"]).is_empty());
    fixture.git(&["add", "old name.txt"]);
    fixture.git(&["commit", "-qm", "base"]);
    fixture.git(&["mv", "old name.txt", "new name.txt"]);
    fixture.write("new name.txt", b"ONE\r\ntwo\r\nthree\r\nfour");
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
    assert_eq!(
        fixture.git(&["show", ":old name.txt"]),
        b"one\r\ntwo\r\nthree\r\nfour"
    );
    assert_eq!(
        std::fs::read(fixture.root.join("new name.txt")).unwrap(),
        b"ONE\r\ntwo\r\nthree\r\nfour"
    );
}

#[tokio::test]
async fn stage_and_unstage_line_ranges_leave_adjacent_replacements() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit("file.txt", b"a\nb\nc\nd\n");
    fixture.write("file.txt", b"a\nB\nC\nd\n");
    let mut request = request(&fixture, "file.txt", "unstaged", 0).await;
    request.hunks[0].ranges = Some(vec![patch::LineRange { start: 1, end: 2 }]);
    stage_hunks(fixture.path(), request).await.unwrap();
    assert_eq!(fixture.git(&["show", ":file.txt"]), b"a\nB\nc\nd\n");
    fixture.git(&["add", "file.txt"]);
    let mut request = self::request(&fixture, "file.txt", "staged", 0).await;
    request.hunks[0].ranges = Some(vec![patch::LineRange { start: 1, end: 2 }]);
    unstage_hunks(fixture.path(), request).await.unwrap();
    assert_eq!(fixture.git(&["show", ":file.txt"]), b"a\nb\nC\nd\n");
    assert_eq!(
        std::fs::read(fixture.root.join("file.txt")).unwrap(),
        b"a\nB\nC\nd\n"
    );
}

#[tokio::test]
async fn binary_hunk_commands_refuse_before_index_mutation() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit("file.txt", b"before\0");
    fixture.write("file.txt", b"after\0");
    let diff = change_hunks(fixture.path(), "file.txt".into(), None, "unstaged".into())
        .await
        .unwrap();
    assert!(diff.binary);
    assert!(diff.hunks.is_empty());
    let request = HunkRequest {
        file: "file.txt".into(),
        orig_path: None,
        area: "unstaged".into(),
        content_hash: diff.content_hash,
        hunks: vec![patch::Selection {
            hunk: 0,
            ranges: None,
        }],
    };
    assert!(stage_hunks(fixture.path(), request)
        .await
        .unwrap_err()
        .contains("Binary"));
    assert_eq!(fixture.git(&["show", ":file.txt"]), b"before\0");
}

#[tokio::test]
async fn unstage_deleted_executable_restores_its_index_mode_and_final_line() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit("file.txt", b"one\r\nlast");
    fixture.git(&["update-index", "--chmod=+x", "file.txt"]);
    fixture.git(&["commit", "-qm", "executable"]);
    fixture.git(&["rm", "-f", "file.txt"]);
    unstage_hunks(
        fixture.path(),
        request(&fixture, "file.txt", "staged", 0).await,
    )
    .await
    .unwrap();
    assert_eq!(fixture.git(&["show", ":file.txt"]), b"one\r\nlast");
    assert!(fixture
        .git(&["ls-files", "--stage", "file.txt"])
        .starts_with(b"100755"));
    assert!(!fixture.root.join("file.txt").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn working_link_is_refused_without_changing_index() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit("file.txt", b"before\n");
    fixture.write("target.txt", b"target\n");
    std::fs::remove_file(fixture.root.join("file.txt")).unwrap();
    std::os::unix::fs::symlink("target.txt", fixture.root.join("file.txt")).unwrap();
    assert!(
        change_hunks(fixture.path(), "file.txt".into(), None, "unstaged".into())
            .await
            .unwrap_err()
            .contains("Linked")
    );
    assert_eq!(fixture.git(&["show", ":file.txt"]), b"before\n");
}
