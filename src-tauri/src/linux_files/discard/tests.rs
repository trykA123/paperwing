use super::*;
use crate::commit::{
    change_hunks, snapshot, stage,
    test_fixture::{two_hunks, Fixture},
};

fn environment(fixture: &Fixture) -> Environment {
    let value = serde_json::json!({ "sources": [], "workspace": { "root": fixture.base, "layout": "flat", "sets": [{ "id": "set", "name": "Fixture", "items": [{ "id": "repo", "name": "repo with spaces" }] }] } });
    Environment::fixture(fixture.base.join("data"), move || {
        serde_json::from_value(value.clone()).map_err(|error| error.to_string())
    })
}

async fn write(fixture: &Fixture, bytes: &[u8]) -> DiscardWrite {
    let snapshot = snapshot::read(&fixture.path(), "with spaces.txt", None, "unstaged")
        .await
        .unwrap();
    DiscardWrite {
        root: fixture.root.clone(),
        file: "with spaces.txt".into(),
        expected: snapshot.working,
        bytes: bytes.to_vec(),
        index_state: snapshot.index_state,
    }
}

#[tokio::test]
async fn discarded_file_has_normal_record_exact_backup_and_recovery_undo() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    let index = b"index\r\nlast";
    let previous = b"discard\r\nlast\0";
    fixture.commit("with spaces.txt", index);
    fixture.write("with spaces.txt", previous);
    let environment = environment(&fixture);
    let recovery = replace(&environment, write(&fixture, index).await)
        .await
        .unwrap();
    assert!(recovery.warning.unwrap().contains("Another process"));
    assert_eq!(
        std::fs::read(fixture.root.join("with spaces.txt")).unwrap(),
        index
    );
    assert_eq!(fixture.git(&["show", ":with spaces.txt"]), index);
    {
        let journal = Journal::open_existing(&environment.app_data)
            .unwrap()
            .unwrap();
        assert_eq!(journal.get(&recovery.id).unwrap().stage, "applied");
        assert_eq!(journal.export(&recovery.id, true).unwrap(), previous);
    }
    let record = super::super::Service::default()
        .undo(&environment, &recovery.id)
        .await
        .unwrap();
    assert_eq!(record.stage, "undone");
    assert_eq!(
        std::fs::read(fixture.root.join("with spaces.txt")).unwrap(),
        previous
    );
    assert_eq!(fixture.git(&["show", ":with spaces.txt"]), index);
}

#[tokio::test]
async fn discarded_hunk_undo_restores_both_hunks_byte_for_byte() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    let (before, after) = two_hunks();
    fixture.commit("with spaces.txt", &before);
    fixture.write("with spaces.txt", &after);
    let snapshot = snapshot::read(&fixture.path(), "with spaces.txt", None, "unstaged")
        .await
        .unwrap();
    let diff = change_hunks(
        fixture.path(),
        "with spaces.txt".into(),
        None,
        "unstaged".into(),
    )
    .await
    .unwrap();
    let request = stage::HunkRequest {
        file: "with spaces.txt".into(),
        orig_path: None,
        area: "unstaged".into(),
        content_hash: diff.content_hash,
        hunks: vec![crate::commit::patch::Selection {
            hunk: 0,
            ranges: None,
        }],
    };
    let discarded = crate::commit::hunk_bytes(&fixture.path(), &snapshot, &request)
        .await
        .unwrap();
    let environment = environment(&fixture);
    let recovery = replace(&environment, write(&fixture, &discarded).await)
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(fixture.root.join("with spaces.txt")).unwrap(),
        discarded
    );
    assert_ne!(discarded, before);
    super::super::Service::default()
        .undo(&environment, &recovery.id)
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(fixture.root.join("with spaces.txt")).unwrap(),
        after
    );
}

#[tokio::test]
async fn git_line_ending_conversions_restore_crlf_and_recovery_undo_exact_bytes() {
    let _serial = crate::test_support::serial().await;
    for attributes in [false, true] {
        let fixture = Fixture::new();
        if attributes {
            fixture.commit(".gitattributes", b"* text eol=crlf\n");
        } else {
            fixture.git(&["config", "core.autocrlf", "true"]);
        }
        let (before, after) = two_hunks();
        fixture.commit("with spaces.txt", &before);
        fixture.write("with spaces.txt", &after);
        let diff = change_hunks(
            fixture.path(),
            "with spaces.txt".into(),
            None,
            "unstaged".into(),
        )
        .await
        .unwrap();
        let plan = crate::commit::prepare_file(
            &fixture.path(),
            &crate::commit::DiscardFile {
                file: "with spaces.txt".into(),
                orig_path: None,
                content_hash: diff.content_hash,
            },
        )
        .await
        .unwrap();
        let crate::commit::DiscardPlan::Restore(write) = plan else {
            panic!("Expected tracked restore");
        };
        let environment = environment(&fixture);
        let recovery = replace(&environment, write).await.unwrap();
        assert_eq!(
            std::fs::read(fixture.root.join("with spaces.txt")).unwrap(),
            before
        );
        super::super::Service::default()
            .undo(&environment, &recovery.id)
            .await
            .unwrap();
        assert_eq!(
            std::fs::read(fixture.root.join("with spaces.txt")).unwrap(),
            after
        );

        let snapshot = snapshot::read(&fixture.path(), "with spaces.txt", None, "unstaged")
            .await
            .unwrap();
        let request = stage::HunkRequest {
            file: "with spaces.txt".into(),
            orig_path: None,
            area: "unstaged".into(),
            content_hash: snapshot.hash.clone(),
            hunks: vec![crate::commit::patch::Selection {
                hunk: 0,
                ranges: None,
            }],
        };
        let bytes = crate::commit::hunk_bytes(&fixture.path(), &snapshot, &request)
            .await
            .unwrap();
        let expected = String::from_utf8(before.clone())
            .unwrap()
            .replace("line 20\r\n", "second change\r\n");
        assert_eq!(bytes, expected.as_bytes());
        let recovery = replace(
            &environment,
            DiscardWrite {
                root: fixture.root.clone(),
                file: "with spaces.txt".into(),
                expected: snapshot.working,
                bytes,
                index_state: snapshot.index_state,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read(fixture.root.join("with spaces.txt")).unwrap(),
            expected.as_bytes()
        );
        super::super::Service::default()
            .undo(&environment, &recovery.id)
            .await
            .unwrap();
        assert_eq!(
            std::fs::read(fixture.root.join("with spaces.txt")).unwrap(),
            after
        );
        assert_eq!(
            fixture.git(&["show", ":with spaces.txt"]),
            String::from_utf8(before)
                .unwrap()
                .replace("\r\n", "\n")
                .into_bytes()
        );
    }
}

#[tokio::test]
async fn external_status_refresh_does_not_block_recoverable_discard() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit("other.txt", b"other\n");
    fixture.commit("with spaces.txt", b"before\n");
    fixture.write("with spaces.txt", b"after\n");
    let pending = write(&fixture, b"before\n").await;
    let raw_index = std::fs::read(fixture.root.join(".git/index")).unwrap();
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
    replace(&environment(&fixture), pending).await.unwrap();
    assert_eq!(
        std::fs::read(fixture.root.join("with spaces.txt")).unwrap(),
        b"before\n"
    );
}

#[tokio::test]
async fn unavailable_recovery_storage_means_no_discard() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit("with spaces.txt", b"index");
    fixture.write("with spaces.txt", b"previous");
    let environment = environment(&fixture);
    std::fs::write(&environment.app_data, b"blocked storage").unwrap();
    assert!(replace(&environment, write(&fixture, b"index").await)
        .await
        .is_err());
    assert_eq!(
        std::fs::read(fixture.root.join("with spaces.txt")).unwrap(),
        b"previous"
    );
    assert_eq!(fixture.git(&["show", ":with spaces.txt"]), b"index");
}

#[tokio::test]
async fn changed_file_or_index_refuses_discard_before_record_or_write() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit("with spaces.txt", b"index");
    fixture.write("with spaces.txt", b"previous");
    let environment = environment(&fixture);
    let stale = write(&fixture, b"index").await;
    fixture.write("with spaces.txt", b"later");
    assert!(replace(&environment, stale)
        .await
        .unwrap_err()
        .contains("changed"));
    let stale = write(&fixture, b"index").await;
    fixture.git(&["add", "with spaces.txt"]);
    assert!(replace(&environment, stale)
        .await
        .unwrap_err()
        .contains("Index changed"));
    assert_eq!(
        std::fs::read(fixture.root.join("with spaces.txt")).unwrap(),
        b"later"
    );
    assert!(!environment.app_data.join("linux-recovery-v1").exists());
}

#[tokio::test]
async fn recovery_undo_preserves_a_later_edit() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit("with spaces.txt", b"index");
    fixture.write("with spaces.txt", b"previous");
    let environment = environment(&fixture);
    let recovery = replace(&environment, write(&fixture, b"index").await)
        .await
        .unwrap();
    fixture.write("with spaces.txt", b"later");
    assert!(super::super::Service::default()
        .undo(&environment, &recovery.id)
        .await
        .is_err());
    assert_eq!(
        std::fs::read(fixture.root.join("with spaces.txt")).unwrap(),
        b"later"
    );
}

#[tokio::test]
async fn discarded_working_deletion_restores_index_and_undo_restores_missing_file() {
    let _serial = crate::test_support::serial().await;
    let fixture = Fixture::new();
    fixture.commit("with spaces.txt", b"index\r\nlast");
    std::fs::remove_file(fixture.root.join("with spaces.txt")).unwrap();
    let environment = environment(&fixture);
    let recovery = replace(&environment, write(&fixture, b"index\r\nlast").await)
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(fixture.root.join("with spaces.txt")).unwrap(),
        b"index\r\nlast"
    );
    super::super::Service::default()
        .undo(&environment, &recovery.id)
        .await
        .unwrap();
    assert!(!fixture.root.join("with spaces.txt").exists());
    assert_eq!(fixture.git(&["show", ":with spaces.txt"]), b"index\r\nlast");
}
