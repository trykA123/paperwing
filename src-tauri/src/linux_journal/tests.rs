use super::*;
#[test]
fn tagged_envelopes_are_checksummed_and_foreign_formats_remain_foreign() {
    let value = record::Content::of(b"backup");
    let encoded = encode(&value).unwrap();
    assert_eq!(decode::<record::Content>(&encoded).unwrap(), value);
    let mut data: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    data["payload"]["length"] = 1.into();
    assert!(matches!(
        decode::<record::Content>(&serde_json::to_vec(&data).unwrap()),
        Err(ReadError::Incomplete)
    ));
    data["version"] = 2.into();
    assert!(matches!(
        decode::<record::Content>(&serde_json::to_vec(&data).unwrap()),
        Err(ReadError::Foreign)
    ));
    assert!(matches!(
        decode::<record::Content>(br#"{"id":"1-2","rootVolume":9,"rootIndex":42}"#),
        Err(ReadError::Foreign)
    ));
    assert!(matches!(
        decode::<record::Content>(b"{\"platform\":"),
        Err(ReadError::Incomplete)
    ));
}
#[test]
fn encoded_metadata_bound_accommodates_supported_decimal_attribute_bytes() {
    let mut security = crate::linux_guard::metadata::Security::new_file();
    for n in 0..4 {
        security
            .attributes
            .insert(format!("user.attribute-{n}"), vec![255; 65536]);
    }
    security.validate().unwrap();
    let encoded = encode(&security).unwrap();
    assert!(encoded.len() > 1024 * 1024);
    assert!(encoded.len() < JSON_LIMIT);
    assert_eq!(
        decode::<crate::linux_guard::metadata::Security>(&encoded).unwrap(),
        security
    );
}

pub(super) fn persist_checkpoint(phase: &str, id: &str) {
    let path = std::path::PathBuf::from(crate::env_names::var_os("SKEIN_JOURNAL_FIXTURE").unwrap());
    assert_eq!(
        std::fs::read(path.join(".skein-journal-fixture")).unwrap(),
        b"skein-journal-fixture-v1\n"
    );
    let file = std::fs::File::create(path.join("checkpoint")).unwrap();
    use std::io::Write;
    (&file)
        .write_all(format!("{phase} {id}\n").as_bytes())
        .unwrap();
    file.sync_all().unwrap();
}

use crate::linux_guard::{mutation::Snapshot, Root};
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Fixture {
    path: std::path::PathBuf,
    _budget: crate::test_support::Shared,
}
impl Fixture {
    fn new() -> Self {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.skillify/evidence/skein/13/repair-1/native")
            .join(format!(
                "fixture-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(
            path.join(".skein-journal-fixture"),
            b"skein-journal-fixture-v1\n",
        )
        .unwrap();
        for directory in ["repo", "repo/.git", "data"] {
            std::fs::create_dir(path.join(directory)).unwrap();
        }
        std::fs::write(path.join("repo/file"), b"before").unwrap();
        std::fs::write(path.join("outside"), b"outside-sentinel").unwrap();
        let root = Root::open(&path.join("repo"), &[]).unwrap();
        let restored_expected = root.parent("file", false).unwrap().snapshot().unwrap();
        let bytes = std::fs::read(path.join("repo/file")).unwrap();
        std::fs::write(path.join("restore-backup"), &bytes).unwrap();
        std::fs::write(path.join("repo/file"), b"restore-drill").unwrap();
        std::fs::write(
            path.join("repo/file"),
            std::fs::read(path.join("restore-backup")).unwrap(),
        )
        .unwrap();
        assert_eq!(std::fs::read(path.join("repo/file")).unwrap(), bytes);
        assert_eq!(
            root.parent("file", false).unwrap().snapshot().unwrap(),
            restored_expected
        );
        std::fs::write(path.join("restore-proof.json"),serde_json::to_vec(&serde_json::json!({"before":hash(&bytes),"restored":hash(&std::fs::read(path.join("repo/file")).unwrap()),"sentinel":hash(b"outside-sentinel"),"identityAndMetadataRestored":true})).unwrap()).unwrap();
        Self { path: path.canonicalize().unwrap(), _budget: crate::test_support::Shared::new() }
    }
    fn root(&self) -> Root {
        Root::open(&self.path.join("repo"), &[]).unwrap()
    }
    fn journal(&self) -> Journal {
        Journal::open(&self.path.join("data")).unwrap()
    }
    fn write(&self, journal: &Journal, path: &str, bytes: &[u8]) -> publication::Published {
        let root = self.root();
        let expected = root.parent(path, false).unwrap().snapshot().unwrap();
        journal.replace(&root, path, &expected, bytes).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(
            std::fs::read(self.path.join("outside")).unwrap(),
            b"outside-sentinel"
        );
    }
}

#[test]
fn native_publication_replacement_creation_and_same_bytes_bind_distinct_identities() {
    let fixture = Fixture::new();
    let journal = fixture.journal();
    let root = fixture.root();
    let before = root.parent("file", false).unwrap().snapshot().unwrap();
    let result = journal.replace(&root, "file", &before, b"before").unwrap();
    let after = root.parent("file", false).unwrap().snapshot().unwrap();
    assert!(
        matches!((&before,&after),(Snapshot::Regular{identity:a,..},Snapshot::Regular{identity:b,..}) if a!=b && b==&result.identity)
    );
    assert_eq!(journal.list().unwrap()[0].stage, "applied");
    assert_eq!(journal.export(&result.record, true).unwrap(), b"before");
    assert_eq!(
        journal.load(&result.record).unwrap().intent.after_security,
        result.security
    );
    let result = fixture.write(&journal, "created", b"created");
    assert!(
        !journal
            .list()
            .unwrap()
            .iter()
            .find(|r| r.id == result.record)
            .unwrap()
            .existed
    );
    assert_eq!(
        std::fs::read(fixture.path.join("repo/created")).unwrap(),
        b"created"
    );
    assert!(journal
        .export("r-00000000000000000000000000000000", true)
        .is_err());
    assert!(!fixture
        .path
        .join("data/linux-recovery-v1/r-00000000000000000000000000000000")
        .exists());
}

#[test]
fn checkpoints_retain_backups_and_post_native_publication_errors_report_applied() {
    for phase in [
        "firstBackup",
        "backups",
        "intent",
        "prepared",
        "candidate",
        "staged",
        "replacing",
        "renamed",
        "directorySynced",
        "applied",
    ] {
        let fixture = Fixture::new();
        let mut journal = fixture.journal();
        journal.fault = Some(phase);
        let root = fixture.root();
        let expected = root.parent("file", false).unwrap().snapshot().unwrap();
        let error = journal
            .replace(&root, "file", &expected, b"after")
            .unwrap_err();
        let id = error.record.unwrap();
        let applied = ["renamed", "directorySynced", "applied"].contains(&phase);
        assert_eq!(error.applied, applied, "{phase}");
        assert_eq!(
            std::fs::read(fixture.path.join("repo/file")).unwrap(),
            if applied {
                b"after".as_slice()
            } else {
                b"before".as_slice()
            },
            "{phase}"
        );
        assert_eq!(
            journal
                .record_dir(&id)
                .unwrap()
                .file("before", false)
                .unwrap()
                .read(64)
                .unwrap(),
            b"before"
        );
        journal.fault = None;
        drop(journal);
        let journal = fixture.journal();
        journal.reconcile().unwrap();
        let record = journal
            .list()
            .unwrap()
            .into_iter()
            .find(|r| r.id == id)
            .unwrap();
        assert_eq!(
            record.stage,
            if ["firstBackup", "backups", "intent"].contains(&phase) {
                "incomplete"
            } else if applied {
                "applied"
            } else {
                "notApplied"
            },
            "{phase}"
        );
    }
}

#[test]
fn unsafe_pending_torn_foreign_and_full_stores_refuse_new_allocation() {
    for phase in ["prepared", "staged", "replacing"] {
        let fixture = Fixture::new();
        let mut journal = fixture.journal();
        journal.fault = Some(phase);
        let root = fixture.root();
        let expected = root.parent("file", false).unwrap().snapshot().unwrap();
        assert!(journal.replace(&root, "file", &expected, b"after").is_err());
        drop(journal);
        let journal = fixture.journal();
        let names = journal.directory.names(32).unwrap();
        assert!(journal.replace(&root, "file", &expected, b"new").is_err());
        assert_eq!(names, journal.directory.names(32).unwrap());
        assert_eq!(
            std::fs::read(fixture.path.join("repo/file")).unwrap(),
            b"before"
        );
    }
    let fixture = Fixture::new();
    let mut journal = fixture.journal();
    let valid = fixture.write(&journal, "file", b"after");
    let directory = journal
        .directory
        .create_child("r-00000000000000000000000000000000")
        .unwrap();
    directory
        .write_new("intent.json", br#"{"platform":"windows","version":2}"#)
        .unwrap();
    let root = fixture.root();
    let expected = root.parent("file", false).unwrap().snapshot().unwrap();
    let names = journal.directory.names(32).unwrap();
    assert!(journal.replace(&root, "file", &expected, b"new").is_err());
    assert_eq!(names, journal.directory.names(32).unwrap());
    assert_eq!(journal.export(&valid.record, true).unwrap(), b"before");
    let loaded = journal.load(&valid.record).unwrap();
    loaded.directory.write_new("state-0005.json", b"{").unwrap();
    assert_eq!(
        journal
            .list()
            .unwrap()
            .iter()
            .find(|r| r.id == valid.record)
            .unwrap()
            .stage,
        "incomplete"
    );
    assert!(journal.replace(&root, "file", &expected, b"new").is_err());
    journal.quota = 0;
    assert!(journal.reserve(1, false).is_err());
    let fixture = Fixture::new();
    let mut journal = fixture.journal();
    journal.records = 0;
    let root = fixture.root();
    let before = root.parent("file", false).unwrap().snapshot().unwrap();
    assert!(journal.replace(&root, "file", &before, b"new").is_err());
    assert_eq!(journal.directory.names(8).unwrap(), vec!["lock"]);
}

#[test]
fn reader_keeps_valid_records_accessible_beside_unknown_corrupt_and_linked_records() {
    let fixture = Fixture::new();
    let journal = fixture.journal();
    let valid = fixture.write(&journal, "file", b"after");
    let foreign = journal
        .directory
        .create_child("r-00000000000000000000000000000000")
        .unwrap();
    foreign
        .write_new(
            "intent.json",
            br#"{"id":"windows-original","rootVolume":3,"rootIndex":9}"#,
        )
        .unwrap();
    let original = foreign
        .file("intent.json", false)
        .unwrap()
        .read(1024)
        .unwrap();
    std::os::unix::fs::symlink(
        fixture.path.join("outside"),
        journal
            .directory
            .path()
            .join("r-11111111111111111111111111111111"),
    )
    .unwrap();
    let list = journal.list().unwrap();
    assert_eq!(list.len(), 3);
    assert_eq!(
        list.iter().find(|r| r.id == valid.record).unwrap().stage,
        "applied"
    );
    assert_eq!(journal.export(&valid.record, false).unwrap(), b"after");
    assert_eq!(
        foreign
            .file("intent.json", false)
            .unwrap()
            .read(1024)
            .unwrap(),
        original
    );
}

#[test]
fn native_undo_restores_bytes_mode_acl_and_user_attributes_with_new_writer_disabled() {
    let fixture = Fixture::new();
    let path = fixture.path.join("repo/file");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    rustix::fs::setxattr(
        &path,
        "user.skein",
        b"retained",
        rustix::fs::XattrFlags::empty(),
    )
    .unwrap();
    let mut acl = 2u32.to_le_bytes().to_vec();
    for (tag, permission, id) in [
        (1u16, 6u16, u32::MAX),
        (2, 4, unsafe { libc::geteuid() } + 1),
        (4, 0, u32::MAX),
        (16, 4, u32::MAX),
        (32, 0, u32::MAX),
    ] {
        acl.extend_from_slice(&tag.to_le_bytes());
        acl.extend_from_slice(&permission.to_le_bytes());
        acl.extend_from_slice(&id.to_le_bytes());
    }
    rustix::fs::setxattr(
        &path,
        "system.posix_acl_access",
        &acl,
        rustix::fs::XattrFlags::empty(),
    )
    .unwrap();
    let mut journal = fixture.journal();
    let before = fixture
        .root()
        .parent("file", false)
        .unwrap()
        .snapshot()
        .unwrap();
    let result = fixture.write(&journal, "file", b"after");
    journal.writer = false;
    assert!(journal
        .replace(
            &fixture.root(),
            "file",
            &fixture
                .root()
                .parent("file", false)
                .unwrap()
                .snapshot()
                .unwrap(),
            b"blocked"
        )
        .is_err());
    assert_eq!(journal.export(&result.record, true).unwrap(), b"before");
    journal.undo(&result.record).unwrap();
    let restored = fixture
        .root()
        .parent("file", false)
        .unwrap()
        .snapshot()
        .unwrap();
    assert!(
        matches!((&before,&restored),(Snapshot::Regular{bytes:a,security:s,..},Snapshot::Regular{bytes:b,security:t,..}) if a==b&&s==t)
    );
    let identity = match restored {
        Snapshot::Regular { identity, .. } => identity,
        _ => unreachable!(),
    };
    journal.undo(&result.record).unwrap();
    assert!(
        matches!(fixture.root().parent("file",false).unwrap().snapshot().unwrap(),Snapshot::Regular{identity:current,..} if current==identity)
    );
    let original = journal.load(&result.record).unwrap();
    let State::Undone {
        reverse: Some(reverse),
        ..
    } = original.revision.state
    else {
        panic!()
    };
    assert_eq!(journal.export(&reverse, true).unwrap(), b"after");
    assert_eq!(journal.export(&reverse, false).unwrap(), b"before");
}

#[test]
fn created_file_undo_and_post_unlink_failures_reconcile_without_recreating_bytes() {
    for phase in ["undoing", "unlinked", "unlinkSynced", "undone"] {
        let fixture = Fixture::new();
        let mut journal = fixture.journal();
        let result = fixture.write(&journal, "created", b"new");
        journal.fault = Some(phase);
        let error = journal.undo(&result.record).unwrap_err();
        assert_eq!(error.applied, phase != "undoing");
        assert_eq!(fixture.path.join("repo/created").exists(), phase == "undoing");
        drop(journal);
        let journal = fixture.journal();
        journal.reconcile().unwrap();
        journal.undo(&result.record).unwrap();
        assert!(!fixture.path.join("repo/created").exists());
        assert_eq!(journal.list().unwrap()[0].stage, "undone");
    }
}

#[test]
fn later_edits_equal_bytes_identity_changes_and_metadata_changes_are_preserved() {
    for edit in ["bytes", "identity", "mode", "attribute", "missing"] {
        let fixture = Fixture::new();
        let journal = fixture.journal();
        let result = fixture.write(&journal, "file", b"after");
        let path = fixture.path.join("repo/file");
        match edit {
            "bytes" => std::fs::write(&path, b"later").unwrap(),
            "identity" => {
                std::fs::rename(&path, fixture.path.join("repo/saved-after")).unwrap();
                std::fs::write(&path, b"after").unwrap();
            }
            "mode" => {
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap()
            }
            "attribute" => rustix::fs::setxattr(
                &path,
                "user.later",
                b"later",
                rustix::fs::XattrFlags::empty(),
            )
            .unwrap(),
            _ => std::fs::remove_file(&path).unwrap(),
        }
        let before = fixture
            .root()
            .parent("file", false)
            .unwrap()
            .snapshot()
            .unwrap();
        assert!(journal.undo(&result.record).is_err(), "{edit}");
        assert_eq!(
            fixture
                .root()
                .parent("file", false)
                .unwrap()
                .snapshot()
                .unwrap(),
            before
        );
        assert_eq!(journal.list().unwrap()[0].stage, "conflict");
        assert!(journal.resolve(&result.record, false).is_err());
        journal.resolve(&result.record, true).unwrap();
        assert_eq!(journal.list().unwrap()[0].stage, "resolved");
        assert_eq!(journal.export(&result.record, true).unwrap(), b"before");
        assert!(journal.cleanup(&result.record, false).is_err());
        assert!(journal.cleanup(&result.record, true).unwrap().complete);
        assert_eq!(
            fixture
                .root()
                .parent("file", false)
                .unwrap()
                .snapshot()
                .unwrap(),
            before
        );
    }
}

#[test]
fn fresh_guard_undo_refuses_changed_roots_pointers_and_destination_ancestors() {
    for edit in ["root", "pointer", "ancestor"] {
        let fixture = Fixture::new();
        std::fs::create_dir(fixture.path.join("repo/nested")).unwrap();
        std::fs::write(fixture.path.join("repo/nested/file"), b"before").unwrap();
        let journal = fixture.journal();
        let result = fixture.write(&journal, "nested/file", b"after");
        match edit {
            "root" => {
                std::fs::rename(fixture.path.join("repo"), fixture.path.join("moved")).unwrap();
                std::fs::create_dir(fixture.path.join("repo")).unwrap();
            }
            "pointer" => {
                std::fs::write(fixture.path.join("repo/.git/commondir"), b"../later").unwrap()
            }
            _ => {
                std::fs::rename(fixture.path.join("repo/nested"), fixture.path.join("old-nested"))
                    .unwrap();
                std::fs::create_dir(fixture.path.join("repo/nested")).unwrap();
                std::fs::write(fixture.path.join("repo/nested/file"), b"later").unwrap();
            }
        }
        assert!(journal.undo(&result.record).is_err(), "{edit}");
        assert_eq!(journal.export(&result.record, true).unwrap(), b"before");
        let path = match edit {
            "root" => fixture.path.join("moved/nested/file"),
            "ancestor" => fixture.path.join("old-nested/file"),
            _ => fixture.path.join("repo/nested/file"),
        };
        assert_eq!(std::fs::read(path).unwrap(), b"after");
    }
}

#[test]
fn unrelated_pending_torn_and_foreign_records_do_not_block_bound_reverse_undo() {
    for blocker in ["pending", "torn", "foreign"] {
        let fixture = Fixture::new();
        let mut journal = fixture.journal();
        let result = fixture.write(&journal, "file", b"after");
        let id = if blocker == "foreign" {
            let id = "r-00000000000000000000000000000000";
            journal
                .directory
                .create_child(id)
                .unwrap()
                .write_new("intent.json", br#"{"platform":"windows","version":2}"#)
                .unwrap();
            id.to_string()
        } else {
            journal.fault = Some("prepared");
            let root = fixture.root();
            let expected = root.parent("other", false).unwrap().snapshot().unwrap();
            let error = journal
                .replace(&root, "other", &expected, b"other")
                .unwrap_err();
            let id = error.record.unwrap();
            journal.fault = None;
            if blocker == "torn" {
                journal
                    .record_dir(&id)
                    .unwrap()
                    .write_new("state-0002.json", b"{")
                    .unwrap();
            }
            id
        };
        let directory = journal.record_dir(&id).unwrap();
        let fingerprint: Vec<_> = directory
            .entries(32)
            .unwrap()
            .into_iter()
            .map(|entry| {
                (
                    entry.name.clone(),
                    hash(
                        &directory
                            .file(&entry.name, false)
                            .unwrap()
                            .read(JSON_LIMIT)
                            .unwrap(),
                    ),
                )
            })
            .collect();
        assert_eq!(journal.export(&result.record, true).unwrap(), b"before");
        journal.undo(&result.record).unwrap();
        assert_eq!(
            std::fs::read(fixture.path.join("repo/file")).unwrap(),
            b"before"
        );
        let after: Vec<_> = directory
            .entries(32)
            .unwrap()
            .into_iter()
            .map(|entry| {
                (
                    entry.name.clone(),
                    hash(
                        &directory
                            .file(&entry.name, false)
                            .unwrap()
                            .read(JSON_LIMIT)
                            .unwrap(),
                    ),
                )
            })
            .collect();
        assert_eq!(after, fingerprint, "{blocker}");
    }
}

#[test]
fn cleanup_manifest_resumes_after_each_deletion_boundary_and_returns_partial_counts() {
    for phase in [
        "cleanupPrepared",
        "cleanupStage",
        "cleanupArtifact",
        "cleanupEmpty",
        "cleanupDirectory",
        "cleanupManifest",
    ] {
        let fixture = Fixture::new();
        let mut journal = fixture.journal();
        let result = fixture.write(&journal, "created", b"created");
        journal.undo(&result.record).unwrap();
        let before = journal.reserve(0, false);
        assert!(before.is_ok());
        journal.fault = Some(phase);
        let first = journal.cleanup(&result.record, true);
        if phase == "cleanupPrepared" {
            assert!(first.is_err());
        } else {
            let first = first.unwrap();
            assert!(!first.complete);
            if phase == "cleanupArtifact" {
                assert_eq!(first.removed, 1);
            }
        }
        drop(journal);
        let journal = fixture.journal();
        let resumed = journal.cleanup(&result.record, true).unwrap();
        assert!(resumed.complete, "{phase}");
        assert!(!journal.directory.path().join(&result.record).exists());
        assert!(!journal
            .directory
            .path()
            .join(format!("cleanup-{}.json", result.record))
            .exists());
        assert_eq!(journal.directory.names(16).unwrap(), vec!["lock"]);
        assert!(!fixture.path.join("repo/created").exists());
        assert_eq!(journal.cleanup(&result.record, true).unwrap().removed, 0);
    }
}

#[test]
fn cleanup_refuses_unknown_entries_substituted_artifacts_and_changed_bytes() {
    for change in ["unknown", "identity", "bytes", "link"] {
        let fixture = Fixture::new();
        let mut journal = fixture.journal();
        let result = fixture.write(&journal, "created", b"created");
        journal.undo(&result.record).unwrap();
        journal.fault = Some("cleanupPrepared");
        assert!(journal.cleanup(&result.record, true).is_err());
        journal.fault = None;
        let path = journal.directory.path().join(&result.record);
        match change {
            "unknown" => {
                let file = journal
                    .record_dir(&result.record)
                    .unwrap()
                    .file("unknown", true)
                    .unwrap();
                file.write(b"retain").unwrap();
            }
            "identity" => {
                std::fs::rename(path.join("before"), path.join("old-before")).unwrap();
                std::fs::write(path.join("before"), b"").unwrap();
                std::fs::set_permissions(
                    path.join("before"),
                    std::fs::Permissions::from_mode(0o600),
                )
                .unwrap();
            }
            "bytes" => std::fs::write(path.join("after"), b"changed").unwrap(),
            _ => std::fs::hard_link(path.join("after"), fixture.path.join("retained-alias")).unwrap(),
        }
        let names = std::fs::read_dir(&path).unwrap().count();
        let result = journal.cleanup(&result.record, true).unwrap();
        assert!(!result.complete);
        assert_eq!(result.removed, 0);
        assert_eq!(std::fs::read_dir(&path).unwrap().count(), names);
    }
}

fn validated_child_fixture() -> Fixture {
    let path = std::path::PathBuf::from(crate::env_names::var_os("SKEIN_JOURNAL_FIXTURE").unwrap());
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../.skillify/evidence/skein/13/repair-1/native")
        .canonicalize()
        .unwrap();
    assert!(path.is_absolute());
    assert_eq!(path.canonicalize().unwrap(), path);
    assert!(path.starts_with(&base));
    assert_eq!(
        std::fs::read(path.join(".skein-journal-fixture")).unwrap(),
        b"skein-journal-fixture-v1\n"
    );
    assert_eq!(
        std::fs::read(path.join("outside")).unwrap(),
        b"outside-sentinel"
    );
    assert_eq!(
        std::fs::read(path.join("restore-backup")).unwrap(),
        b"before"
    );
    Fixture { path, _budget: crate::test_support::Shared::new() }
}
#[test]
#[ignore]
fn native_child() {
    let fixture = validated_child_fixture();
    let mode = crate::env_names::var("SKEIN_JOURNAL_CHILD_MODE").unwrap();
    if mode == "park" {
        std::fs::write(
            fixture.path.join("child-ready.json"),
            serde_json::to_vec(&serde_json::json!({"pid":std::process::id()})).unwrap(),
        )
        .unwrap();
        loop {
            std::thread::park();
        }
    }
    if mode == "earlyExit" {
        return;
    }
    if mode == "pidfdProbe" {
        let error = match OwnedChild::spawn_result(&fixture, "park", None, None) {
            Err(error) => error,
            Ok(_) => panic!("Forced pidfd acquisition unexpectedly succeeded"),
        };
        assert_eq!(error.raw_os_error(), Some(libc::EMFILE));
        return;
    }
    if mode == "earlyExitProbe" {
        let mut child = OwnedChild::spawn(&fixture, "earlyExit", None, None);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            child.checkpoint(&fixture, "absent")
        }));
        assert!(result.is_err());
        assert!(child.reaped);
        drop(child);
        return;
    }
    if mode == "lock" {
        assert!(fixture.journal_result().is_err());
        return;
    }
    if mode == "io" {
        native_io_child(&fixture);
        return;
    }
    if mode == "restoreControl" {
        let leaf = crate::env_names::var("SKEIN_CONTROL_STORE").unwrap();
        assert!(["data", "bind-data", "nested-bind-data"].contains(&leaf.as_str()));
        let original = Journal::open(&fixture.path.join("repo").join(&leaf)).unwrap();
        let id = crate::env_names::var("SKEIN_JOURNAL_RECORD").unwrap();
        let loaded = original.load(&id).unwrap();
        assert!(matches!(loaded.revision.state, State::Applied { .. }));
        let backup = original.export(&id, true).unwrap();
        assert_eq!(backup, b"before");
        let security = loaded.intent.before.as_ref().unwrap().security.clone();
        let root = fixture.root();
        let parent = root.parent("file", false).unwrap();
        let expected = parent.snapshot().unwrap();
        assert!(
            matches!(&expected,Snapshot::Regular{bytes,security,..} if loaded.intent.after.matches(bytes) && security==&loaded.intent.after_security)
        );
        let journal = fixture.journal();
        let restored = journal
            .publish_record(publication::Write {
                root: &root,
                parent: &parent,
                path: "file",
                expected: &expected,
                after: &backup,
                security: &security,
                id: None,
                reverse: None,
                binding: None,
            })
            .unwrap();
        let actual = parent.snapshot().unwrap();
        assert!(
            matches!(&actual,Snapshot::Regular{bytes,security:actual,..} if bytes==&backup && actual==&security)
        );
        std::fs::write(fixture.path.join("negative-control-restored.json"),serde_json::to_vec_pretty(&serde_json::json!({"originalRecord":id,"restoreRecord":restored.record,"backupHash":hash(&backup),"restoredHash":hash(actual.bytes().unwrap()),"mode":security.mode,"uidInFixtureNamespace":security.uid,"gidInFixtureNamespace":security.gid,"metadataRestored":true,"sentinelHash":hash(b"outside-sentinel")})).unwrap()).unwrap();
        return;
    }
    if mode == "nestedBind" {
        let journal = Journal::open(&fixture.path.join("namespace-alias")).unwrap();
        let root = fixture.root();
        let expected = root.parent("file", false).unwrap().snapshot().unwrap();
        assert!(journal.replace(&root, "file", &expected, b"after").is_err());
        assert_eq!(journal.directory.names(8).unwrap(), vec!["lock"]);
        assert_eq!(
            root.parent("file", false).unwrap().snapshot().unwrap(),
            expected
        );
        std::fs::write(
            fixture.path.join("nested-bind-proof.json"),
            br#"{"recordAllocated":false,"targetChanged":false,"nestedBindRefused":true}"#,
        )
        .unwrap();
        return;
    }
    if mode == "insideBind" {
        let app_data = fixture.path.join("repo/bind-data");
        std::fs::create_dir(&app_data).unwrap();
        let journal = Journal::open(&app_data).unwrap();
        let root = Root::open(&fixture.path.join("namespace-alias"), &[]).unwrap();
        let expected = root.parent("file", false).unwrap().snapshot().unwrap();
        assert!(journal.replace(&root, "file", &expected, b"after").is_err());
        assert_eq!(journal.directory.names(8).unwrap(), vec!["lock"]);
        assert_eq!(
            root.parent("file", false).unwrap().snapshot().unwrap(),
            expected
        );
        std::fs::write(
            fixture.path.join("bind-outside-proof.json"),
            br#"{"recordAllocated":false,"targetChanged":false,"physicalAliasRefused":true}"#,
        )
        .unwrap();
        return;
    }
    let journal = fixture.journal();
    match mode.as_str() {
        "replace" => {
            let root = fixture.root();
            let expected = root.parent("file", false).unwrap().snapshot().unwrap();
            journal.replace(&root, "file", &expected, b"after").unwrap();
        }
        "undo" => journal
            .undo(&crate::env_names::var("SKEIN_JOURNAL_RECORD").unwrap())
            .unwrap(),
        "cleanup" => {
            assert!(
                journal
                    .cleanup(&crate::env_names::var("SKEIN_JOURNAL_RECORD").unwrap(), true)
                    .unwrap()
                    .complete
            );
        }
        _ => panic!("Invalid native child mode"),
    }
}
impl Fixture {
    fn journal_result(&self) -> Result<Journal, Error> {
        Journal::open(&self.path.join("data"))
    }
}
struct OwnedChild {
    child: std::process::Child,
    pidfd: Option<std::os::fd::OwnedFd>,
    reaped: bool,
    status: Option<std::process::ExitStatus>,
}
impl OwnedChild {
    fn spawn(fixture: &Fixture, mode: &str, phase: Option<&str>, id: Option<&str>) -> Self {
        Self::spawn_result(fixture, mode, phase, id).unwrap()
    }
    fn spawn_result(
        fixture: &Fixture,
        mode: &str,
        phase: Option<&str>,
        id: Option<&str>,
    ) -> std::io::Result<Self> {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "linux_journal::tests::native_child",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("SKEIN_JOURNAL_FIXTURE", &fixture.path)
            .env("SKEIN_JOURNAL_CHILD_MODE", mode)
            .env_remove("SKEIN_JOURNAL_KILL_PHASE")
            .env_remove("SKEIN_JOURNAL_RECORD")
            .stdout(std::fs::File::create(fixture.path.join("child-stdout.log"))?)
            .stderr(std::fs::File::create(fixture.path.join("child-stderr.log"))?);
        if let Some(phase) = phase {
            command.env("SKEIN_JOURNAL_KILL_PHASE", phase);
        }
        if let Some(id) = id {
            command.env("SKEIN_JOURNAL_RECORD", id);
        }
        let mut owner = Self {
            child: command.spawn()?,
            pidfd: None,
            reaped: false,
            status: None,
        };
        let pid = owner.child.id();
        std::fs::write(
            fixture.path.join("spawned-child.json"),
            serde_json::to_vec(
                &serde_json::json!({"pid":pid,"parent":std::process::id(),"mode":mode}),
            )
            .map_err(std::io::Error::other)?,
        )?;
        let forced =
            mode == "park" && crate::env_names::var("SKEIN_JOURNAL_PIDFD_FAIL").as_deref() == Ok("1");
        let fd = if forced {
            -1
        } else {
            unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) }
        };
        if fd < 0 {
            let error = if forced {
                std::io::Error::from_raw_os_error(libc::EMFILE)
            } else {
                std::io::Error::last_os_error()
            };
            owner.stop()?;
            return Err(error);
        }
        use std::os::fd::FromRawFd;
        owner.pidfd = Some(unsafe { std::os::fd::OwnedFd::from_raw_fd(fd as i32) });
        std::fs::write(fixture.path.join("owned-child.json"),serde_json::to_vec(&serde_json::json!({"pid":pid,"pidfdOwned":true,"mode":mode,"phase":phase,"fixture":fixture.path})).map_err(std::io::Error::other)?)?;
        Ok(owner)
    }
    fn poll(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        if self.reaped {
            return Ok(self.status);
        }
        let status = self.child.try_wait()?;
        if let Some(status) = status {
            self.reaped = true;
            self.status = Some(status);
        }
        Ok(status)
    }
    fn wait_bounded(&mut self) -> std::process::ExitStatus {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            if let Some(status) = self.poll().unwrap() {
                return status;
            }
            assert!(std::time::Instant::now() < end, "Native child timeout");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    fn stop(&mut self) -> std::io::Result<()> {
        if self.poll()?.is_some() {
            return Ok(());
        }
        if let Some(pidfd) = &self.pidfd {
            use std::os::fd::AsRawFd;
            if unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    pidfd.as_raw_fd(),
                    libc::SIGKILL,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                )
            } < 0
            {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(error);
                }
            }
        } else {
            self.child.kill()?;
        }
        let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if self.poll()?.is_some() {
                return Ok(());
            }
            if std::time::Instant::now() >= end {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "Owned child did not reap after termination",
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    fn kill(&mut self) {
        assert!(!self.reaped);
        self.stop().unwrap();
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(self.status.unwrap().signal(), Some(libc::SIGKILL));
    }
    fn checkpoint(&mut self, fixture: &Fixture, phase: &str) -> String {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            if let Ok(bytes) = std::fs::read_to_string(fixture.path.join("checkpoint")) {
                let mut values = bytes.split_whitespace();
                if values.next() == Some(phase) {
                    let id = values.next().unwrap().to_string();
                    assert!(record::id_valid(&id));
                    return id;
                }
            }
            if let Some(status) = self.poll().unwrap() {
                panic!("Child exited before checkpoint {phase}: {status}");
            }
            assert!(
                std::time::Instant::now() < end,
                "Missing native checkpoint {phase}"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[test]
fn second_process_flock_refuses_competitors_and_releases_after_owner_closes() {
    let fixture = Fixture::new();
    let journal = fixture.journal();
    let mut child = OwnedChild::spawn(&fixture, "lock", None, None);
    assert!(child.wait_bounded().success());
    drop(child);
    drop(journal);
    fixture.journal();
}

#[test]
fn actual_sigkill_publication_restart_matrix_retains_identity_proofs_stages_and_backups() {
    let mut matrix = Vec::new();
    for phase in [
        "firstBackup",
        "backups",
        "intent",
        "prepared",
        "candidate",
        "staged",
        "replacing",
        "renamed",
        "directorySynced",
        "applied",
    ] {
        let fixture = Fixture::new();
        std::fs::set_permissions(
            fixture.path.join("repo/file"),
            std::fs::Permissions::from_mode(0o640),
        )
        .unwrap();
        rustix::fs::setxattr(
            fixture.path.join("repo/file"),
            "user.kill-fixture",
            b"metadata",
            rustix::fs::XattrFlags::empty(),
        )
        .unwrap();
        let before = fixture
            .root()
            .parent("file", false)
            .unwrap()
            .snapshot()
            .unwrap();
        let mut child = OwnedChild::spawn(&fixture, "replace", Some(phase), None);
        let id = child.checkpoint(&fixture, phase);
        child.kill();
        drop(child);
        let mut journal = fixture.journal();
        let directory = journal.record_dir(&id).unwrap();
        assert_eq!(
            directory.file("before", false).unwrap().read(64).unwrap(),
            b"before"
        );
        let names = journal.directory.names(32).unwrap();
        let parent = fixture.root().parent("file", false).unwrap();
        let current = parent.snapshot().unwrap();
        let applied = ["renamed", "directorySynced", "applied"].contains(&phase);
        assert_eq!(
            current.bytes().unwrap(),
            if applied {
                b"after".as_slice()
            } else {
                b"before".as_slice()
            }
        );
        if ["candidate", "staged", "replacing", "renamed"].contains(&phase) {
            assert_eq!(parent.stage_artifacts(32).unwrap().len(), 1, "{phase}");
        }
        if ["prepared", "candidate", "staged", "replacing"].contains(&phase) {
            assert!(journal
                .replace(&fixture.root(), "file", &current, b"blocked")
                .is_err());
            assert_eq!(journal.directory.names(32).unwrap(), names);
        }
        journal.reconcile().unwrap();
        let stage = journal
            .list()
            .unwrap()
            .into_iter()
            .find(|r| r.id == id)
            .unwrap()
            .stage;
        if applied {
            journal.writer = false;
            assert_eq!(journal.export(&id, true).unwrap(), b"before");
            journal.undo(&id).unwrap();
        }
        let restored = parent.snapshot().unwrap();
        assert!(
            matches!((&before,&restored),(Snapshot::Regular{bytes:a,security:s,..},Snapshot::Regular{bytes:b,security:t,..}) if a==b&&s==t)
        );
        if ["staged", "replacing"].contains(&phase) {
            assert!(journal.cleanup(&id, true).unwrap().complete);
            assert!(parent.stage_artifacts(32).unwrap().is_empty());
        }
        if phase == "candidate" {
            assert!(journal.cleanup(&id, true).is_err());
            assert!(journal
                .replace(&fixture.root(), "file", &restored, b"blocked")
                .is_err());
        }
        matrix.push(serde_json::json!({"phase":phase,"fixture":fixture.path,"record":id,"classified":stage,"beforeHash":hash(b"before"),"restoredHash":hash(restored.bytes().unwrap()),"sentinelHash":hash(b"outside-sentinel"),"processReaped":true,"powerLoss":false}));
    }
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../.skillify/evidence/skein/13/repair-1");
    std::fs::write(
        base.join("sigkill-publication.json"),
        serde_json::to_vec_pretty(&matrix).unwrap(),
    )
    .unwrap();
}

#[test]
fn actual_sigkill_reverse_and_created_undo_resume_with_forward_reader_available() {
    let mut matrix = Vec::new();
    for (created, phase) in [
        (false, "undoing"),
        (false, "firstBackup"),
        (false, "prepared"),
        (false, "staged"),
        (false, "replacing"),
        (false, "renamed"),
        (false, "directorySynced"),
        (false, "applied"),
        (false, "reverseApplied"),
        (false, "undone"),
        (true, "undoing"),
        (true, "unlinked"),
        (true, "unlinkSynced"),
        (true, "undone"),
    ] {
        let fixture = Fixture::new();
        let journal = fixture.journal();
        let path = if created { "created" } else { "file" };
        let result = fixture.write(&journal, path, b"after");
        drop(journal);
        let mut child = OwnedChild::spawn(&fixture, "undo", Some(phase), Some(&result.record));
        child.checkpoint(&fixture, phase);
        child.kill();
        drop(child);
        let mut journal = fixture.journal();
        journal.writer = false;
        journal.reconcile().unwrap();
        let state = journal.load(&result.record).unwrap().revision.state;
        let incomplete = !created && phase == "firstBackup";
        if incomplete {
            assert!(journal.undo(&result.record).is_err());
            assert_eq!(
                std::fs::read(fixture.path.join("repo/file")).unwrap(),
                b"after"
            );
            assert_eq!(journal.export(&result.record, true).unwrap(), b"before");
        } else {
            journal.undo(&result.record).unwrap();
            if created {
                assert!(!fixture.path.join("repo/created").exists());
            } else {
                assert_eq!(
                    std::fs::read(fixture.path.join("repo/file")).unwrap(),
                    b"before"
                );
            }
        }
        matrix.push(serde_json::json!({"phase":phase,"created":created,"fixture":fixture.path,"record":result.record,"restartState":state.name(),"incompleteRetained":incomplete,"processReaped":true}));
    }
    std::fs::write(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.skillify/evidence/skein/13/repair-1/sigkill-undo.json"),
        serde_json::to_vec_pretty(&matrix).unwrap(),
    )
    .unwrap();
}

#[test]
fn actual_sigkill_cleanup_resumes_through_empty_directory_and_final_manifest_deletion() {
    let mut matrix = Vec::new();
    for phase in [
        "cleanupPrepared",
        "cleanupStage",
        "cleanupArtifact",
        "cleanupEmpty",
        "cleanupDirectory",
        "cleanupManifest",
    ] {
        let fixture = Fixture::new();
        let journal = fixture.journal();
        let result = fixture.write(&journal, "created", b"created");
        journal.undo(&result.record).unwrap();
        drop(journal);
        let mut child = OwnedChild::spawn(&fixture, "cleanup", Some(phase), Some(&result.record));
        child.checkpoint(&fixture, phase);
        child.kill();
        drop(child);
        let mut journal = fixture.journal();
        journal.records = 1;
        if phase != "cleanupManifest" {
            assert_eq!(journal.list().unwrap()[0].id, result.record);
        }
        assert!(journal.cleanup(&result.record, true).unwrap().complete);
        assert!(journal.reserve(1, true).is_ok());
        assert_eq!(journal.directory.names(32).unwrap(), vec!["lock"]);
        assert!(!fixture.path.join("repo/created").exists());
        matrix.push(serde_json::json!({"phase":phase,"fixture":fixture.path,"record":result.record,"complete":true,"processReaped":true}));
    }
    std::fs::write(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.skillify/evidence/skein/13/repair-1/sigkill-cleanup.json"),
        serde_json::to_vec_pretty(&matrix).unwrap(),
    )
    .unwrap();
}
fn native_io_child(fixture: &Fixture) {
    let mount = fixture.path.join("namespace-mount");
    let directory =
        crate::linux_guard::storage::PrivateDir::open(&mount, "linux-recovery-v1").unwrap();
    let phase = crate::env_names::var("SKEIN_JOURNAL_IO_PHASE").unwrap();
    if phase == "readonly" {
        assert_eq!(
            directory.file("lock", false).unwrap_err().code,
            Some(libc::EROFS)
        );
        assert!(!mount.join("readonly-new").exists());
        std::fs::write(
            fixture.path.join("io-readonly.json"),
            br#"{"EROFS":true,"sourcePreserved":true,"sentinelPreserved":true}"#,
        )
        .unwrap();
        return;
    }
    let lock = directory.lock().unwrap();
    let journal = Journal {
        directory,
        _lock: lock,
        guarded: None,
        fault: None,
        quota: STORAGE_LIMIT,
        records: RECORD_LIMIT,
        writer: true,
    };
    if phase == "full" {
        assert!(Journal::open(&mount).is_err());
        let root = fixture.root();
        let before = root.parent("file", false).unwrap().snapshot().unwrap();
        let error = journal
            .replace(&root, "file", &before, &vec![b'x'; 128 * 1024])
            .unwrap_err();
        assert_eq!(error.code, Some(libc::ENOSPC));
        assert!(error.record.is_some());
        assert!(!error.applied);
        assert_eq!(
            root.parent("file", false).unwrap().snapshot().unwrap(),
            before
        );
        let source = fixture.path.join("cross-device-source");
        std::fs::write(&source, b"retained-source").unwrap();
        let error = rustix::fs::renameat_with(
            rustix::fs::CWD,
            &source,
            rustix::fs::CWD,
            mount.join("cross-device-target"),
            rustix::fs::RenameFlags::NOREPLACE,
        )
        .unwrap_err();
        assert_eq!(error, rustix::io::Errno::XDEV);
        assert_eq!(std::fs::read(&source).unwrap(), b"retained-source");
        std::fs::write(fixture.path.join("io-full.json"),serde_json::to_vec(&serde_json::json!({"ENOSPC":true,"EXDEV":true,"sourceHash":hash(before.bytes().unwrap()),"sentinelHash":hash(b"outside-sentinel"),"applied":false})).unwrap()).unwrap();
    } else {
        panic!("Invalid writable native I/O phase");
    }
}

#[test]
fn sequential_saves_require_owned_reverse_provenance_for_older_undo() {
    let fixture = Fixture::new();
    let journal = fixture.journal();
    let first = fixture.write(&journal, "file", b"B");
    let second = fixture.write(&journal, "file", b"C");
    journal.undo(&second.record).unwrap();
    assert_eq!(std::fs::read(fixture.path.join("repo/file")).unwrap(), b"B");
    journal.undo(&first.record).unwrap();
    assert_eq!(
        std::fs::read(fixture.path.join("repo/file")).unwrap(),
        b"before"
    );
    journal.undo(&first.record).unwrap();
    assert_eq!(
        std::fs::read(fixture.path.join("repo/file")).unwrap(),
        b"before"
    );
}

#[test]
fn trusted_stack_transition_preserves_external_equal_bytes_and_created_predecessors() {
    for external in [false, true] {
        let fixture = Fixture::new();
        let journal = fixture.journal();
        let first = fixture.write(&journal, "created", b"created");
        let second = fixture.write(&journal, "created", b"changed");
        journal.undo(&second.record).unwrap();
        if external {
            std::fs::rename(
                fixture.path.join("repo/created"),
                fixture.path.join("saved-created"),
            )
            .unwrap();
            std::fs::write(fixture.path.join("repo/created"), b"created").unwrap();
            assert!(journal.undo(&first.record).is_err());
            assert_eq!(
                std::fs::read(fixture.path.join("repo/created")).unwrap(),
                b"created"
            );
        } else {
            journal.undo(&first.record).unwrap();
            assert!(!fixture.path.join("repo/created").exists());
            journal.undo(&first.record).unwrap();
        }
    }
    let fixture = Fixture::new();
    let journal = fixture.journal();
    let first = fixture.write(&journal, "file", b"B");
    let second = fixture.write(&journal, "file", b"C");
    journal.undo(&second.record).unwrap();
    std::fs::rename(fixture.path.join("repo/file"), fixture.path.join("saved-B")).unwrap();
    std::fs::write(fixture.path.join("repo/file"), b"B").unwrap();
    let before = fixture
        .root()
        .parent("file", false)
        .unwrap()
        .snapshot()
        .unwrap();
    assert!(journal.undo(&first.record).is_err());
    assert_eq!(
        fixture
            .root()
            .parent("file", false)
            .unwrap()
            .snapshot()
            .unwrap(),
        before
    );
}

fn clone_record(
    journal: &Journal,
    source: &Loaded,
    id: &str,
    reverse_of: Option<&str>,
    reverse_id: Option<&str>,
    file_identity: Option<&crate::linux_guard::Identity>,
) {
    let directory = journal.directory.create_child(id).unwrap();
    let mut intent = source.intent.clone();
    intent.id = id.into();
    intent.reverse_of = reverse_of.map(str::to_string);
    let encoded = encode(&intent).unwrap();
    let intent_hash = hash(&encoded);
    directory.write_new("before", &source.before).unwrap();
    directory.write_new("after", &source.after).unwrap();
    directory.write_new("intent.json", &encoded).unwrap();
    let mut previous = None;
    for sequence in 1..=source.revision.sequence {
        let bytes = source
            .directory
            .file(&state::name(sequence), false)
            .unwrap()
            .read(JSON_LIMIT)
            .unwrap();
        let mut revision: Revision = decode(&bytes).unwrap();
        revision.intent = intent_hash.clone();
        revision.previous = previous;
        if let Some(id) = reverse_id {
            if let State::Undoing { reverse, .. } | State::Undone { reverse, .. } =
                &mut revision.state
            {
                *reverse = Some(id.into());
            }
        }
        if let Some(identity) = file_identity {
            match &mut revision.state {
                State::Staged { proof } | State::Replacing { proof } => {
                    proof.file = identity.clone()
                }
                State::Applied {
                    proof,
                    identity: actual,
                } => {
                    proof.file = identity.clone();
                    *actual = identity.clone();
                }
                _ => (),
            }
        }
        previous = Some(revision.fingerprint().unwrap());
        directory
            .write_new(&state::name(sequence), &encode(&revision).unwrap())
            .unwrap();
    }
}

#[test]
fn missing_cleaned_corrupt_competing_and_cyclic_stack_provenance_refuse_older_undo() {
    for defect in ["missing", "cleaned", "corrupt", "competing", "cyclic"] {
        let fixture = Fixture::new();
        let journal = fixture.journal();
        let first = fixture.write(&journal, "file", b"B");
        let second = fixture.write(&journal, "file", b"C");
        journal.undo(&second.record).unwrap();
        let original = journal.load(&second.record).unwrap();
        let State::Undone {
            reverse: Some(reverse),
            ..
        } = &original.revision.state
        else {
            panic!()
        };
        let reversed = journal.load(reverse).unwrap();
        match defect {
            "missing" => std::fs::rename(
                journal.directory.path().join(reverse),
                fixture.path.join("retained-reverse"),
            )
            .unwrap(),
            "cleaned" => {
                assert!(journal.cleanup(&second.record, true).unwrap().complete);
            }
            "corrupt" => std::fs::write(
                reversed
                    .directory
                    .path()
                    .join(state::name(reversed.revision.sequence)),
                b"{",
            )
            .unwrap(),
            "competing" | "cyclic" => {
                let original_id = "r-11111111111111111111111111111111";
                let reverse_id = "r-22222222222222222222222222222222";
                let identity = original.intent.before.as_ref().unwrap().identity.clone();
                clone_record(
                    &journal,
                    &original,
                    original_id,
                    None,
                    Some(reverse_id),
                    None,
                );
                clone_record(
                    &journal,
                    &reversed,
                    reverse_id,
                    Some(original_id),
                    None,
                    (defect == "cyclic").then_some(&identity),
                );
                if defect == "cyclic" {
                    std::fs::rename(
                        original.directory.path(),
                        fixture.path.join("retained-original"),
                    )
                    .unwrap();
                    std::fs::rename(
                        reversed.directory.path(),
                        fixture.path.join("retained-reverse"),
                    )
                    .unwrap();
                }
            }
            _ => unreachable!(),
        }
        let before = fixture
            .root()
            .parent("file", false)
            .unwrap()
            .snapshot()
            .unwrap();
        assert!(journal.undo(&first.record).is_err(), "{defect}");
        assert_eq!(
            fixture
                .root()
                .parent("file", false)
                .unwrap()
                .snapshot()
                .unwrap(),
            before,
            "{defect}"
        );
        assert_eq!(journal.export(&first.record, true).unwrap(), b"before");
    }
}

#[test]
fn actual_sigkill_stacked_undo_resumes_only_with_exact_persisted_provenance() {
    let mut matrix = Vec::new();
    for (created, phase) in [
        (false, "undoing"),
        (false, "prepared"),
        (false, "staged"),
        (false, "replacing"),
        (false, "renamed"),
        (false, "applied"),
        (false, "reverseApplied"),
        (false, "undone"),
        (true, "undoing"),
        (true, "unlinked"),
        (true, "undone"),
    ] {
        let fixture = Fixture::new();
        let journal = fixture.journal();
        let path = if created { "created" } else { "file" };
        let first = fixture.write(&journal, path, b"B");
        let second = fixture.write(&journal, path, b"C");
        journal.undo(&second.record).unwrap();
        drop(journal);
        let mut child = OwnedChild::spawn(&fixture, "undo", Some(phase), Some(&first.record));
        child.checkpoint(&fixture, phase);
        child.kill();
        drop(child);
        let mut journal = fixture.journal();
        journal.writer = false;
        journal.reconcile().unwrap();
        journal.undo(&first.record).unwrap();
        if created {
            assert!(!fixture.path.join("repo/created").exists());
        } else {
            assert_eq!(
                std::fs::read(fixture.path.join("repo/file")).unwrap(),
                b"before"
            );
        }
        matrix.push(serde_json::json!({"created":created,"phase":phase,"record":first.record,"fixture":fixture.path,"complete":true,"processReaped":true}));
    }
    std::fs::write(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.skillify/evidence/skein/13/repair-1/sigkill-stack.json"),
        serde_json::to_vec_pretty(&matrix).unwrap(),
    )
    .unwrap();
}

#[test]
fn isolated_native_journal_full_readonly_and_cross_device_failures_preserve_source() {
    let fixture = Fixture::new();
    std::fs::create_dir(fixture.path.join("namespace-mount")).unwrap();
    let namespace = std::fs::read_link("/proc/self/ns/mnt").unwrap();
    let script = r#"import os,pathlib,subprocess,sys
root=pathlib.Path(sys.argv[1]);exe=sys.argv[2];original=sys.argv[3]
assert root.is_absolute() and root.resolve()==root
assert (root/'.skein-journal-fixture').read_bytes()==b'skein-journal-fixture-v1\n'
assert os.readlink('/proc/self/ns/mnt')!=original
mount=root/'namespace-mount'
subprocess.run(['mount','-t','tmpfs','-o','size=64k,mode=700','tmpfs',str(mount)],check=True,timeout=10)
env=os.environ.copy();env['SKEIN_JOURNAL_FIXTURE']=str(root);env['SKEIN_JOURNAL_CHILD_MODE']='io';env['SKEIN_JOURNAL_IO_PHASE']='full'
subprocess.run([exe,'--exact','linux_journal::tests::native_child','--ignored','--nocapture','--test-threads=1'],env=env,check=True,timeout=20)
subprocess.run(['mount','-o','remount,ro','-t','tmpfs','tmpfs',str(mount)],check=True,timeout=10)
env['SKEIN_JOURNAL_IO_PHASE']='readonly'
subprocess.run([exe,'--exact','linux_journal::tests::native_child','--ignored','--nocapture','--test-threads=1'],env=env,check=True,timeout=20)
assert (root/'repo/file').read_bytes()==b'before'
assert (root/'outside').read_bytes()==b'outside-sentinel'

"#;
    let output = std::process::Command::new("unshare")
        .args(["-Urnm", "python3", "-c", script])
        .arg(&fixture.path)
        .arg(std::env::current_exe().unwrap())
        .arg(&namespace)
        .output()
        .unwrap();
    std::fs::write(fixture.path.join("io-stdout.log"), &output.stdout).unwrap();
    std::fs::write(fixture.path.join("io-stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_link("/proc/self/ns/mnt").unwrap(), namespace);
    assert!(fixture.path.join("io-full.json").exists());
    assert!(fixture.path.join("io-readonly.json").exists());
}

#[test]
fn verified_backup_export_survives_torn_state_but_refuses_corrupt_bytes_and_unknown_intent() {
    let fixture = Fixture::new();
    let journal = fixture.journal();
    let result = fixture.write(&journal, "file", b"after");
    let loaded = journal.load(&result.record).unwrap();
    std::fs::write(loaded.directory.path().join("state-0004.json"), b"{").unwrap();
    assert_eq!(journal.export(&result.record, true).unwrap(), b"before");
    assert_eq!(journal.export(&result.record, false).unwrap(), b"after");
    assert!(journal.undo(&result.record).is_err());
    std::fs::write(loaded.directory.path().join("before"), b"corrupt").unwrap();
    assert!(journal.export(&result.record, true).is_err());
    assert_eq!(journal.export(&result.record, false).unwrap(), b"after");
    let bytes = loaded
        .directory
        .file("intent.json", false)
        .unwrap()
        .read(JSON_LIMIT)
        .unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    value["payload"]["unknown"] = true.into();
    std::fs::write(
        loaded.directory.path().join("intent.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    assert_eq!(journal.list().unwrap()[0].stage, "foreign");
    assert!(journal.export(&result.record, false).is_err());
}

#[test]
fn both_security_snapshots_fit_the_encoded_intent_bound() {
    let fixture = Fixture::new();
    let journal = fixture.journal();
    let result = fixture.write(&journal, "file", b"after");
    let mut intent = journal.load(&result.record).unwrap().intent;
    let mut security = crate::linux_guard::metadata::Security::new_file();
    for n in 0..4 {
        security
            .attributes
            .insert(format!("user.attribute-{n}"), vec![255; 65536]);
    }
    intent.before.as_mut().unwrap().security = security.clone();
    intent.after_security = security;
    intent.validate().unwrap();
    let encoded = encode(&intent).unwrap();
    assert!(encoded.len() > 2 * 1024 * 1024);
    assert!(encoded.len() < JSON_LIMIT);
    assert_eq!(decode::<record::Intent>(&encoded).unwrap(), intent);
}

#[test]
fn maximum_state_chain_is_readable_and_refuses_an_additional_revision_without_target_mutation() {
    let fixture = Fixture::new();
    let journal = fixture.journal();
    let result = fixture.write(&journal, "file", b"after");
    let loaded = journal.load(&result.record).unwrap();
    let proof = loaded.revision.state.proof().unwrap().clone();
    let mut previous = loaded.revision.fingerprint().unwrap();
    for sequence in 5..=STATE_LIMIT {
        let revision = Revision {
            sequence,
            previous: Some(previous),
            intent: loaded.intent_hash.clone(),
            state: State::Undoing {
                proof: proof.clone(),
                identity: proof.file.clone(),
                provenance: Vec::new(),
                reverse: Some("r-00000000000000000000000000000000".into()),
            },
        };
        previous = revision.fingerprint().unwrap();
        loaded
            .directory
            .write_new(&state::name(sequence), &encode(&revision).unwrap())
            .unwrap();
    }
    let mut loaded = journal.load(&result.record).unwrap();
    assert_eq!(loaded.revision.sequence, STATE_LIMIT);
    let before = fixture
        .root()
        .parent("file", false)
        .unwrap()
        .snapshot()
        .unwrap();
    let names = loaded.directory.names(ARTIFACT_LIMIT).unwrap();
    let state = loaded.revision.state.clone();
    assert!(journal.append(&mut loaded, state).is_err());
    assert_eq!(names, loaded.directory.names(ARTIFACT_LIMIT).unwrap());
    assert_eq!(
        fixture
            .root()
            .parent("file", false)
            .unwrap()
            .snapshot()
            .unwrap(),
        before
    );
}

#[test]
fn confirmed_cleanup_reclaims_only_physically_removed_artifact_capacity() {
    let fixture = Fixture::new();
    let mut journal = fixture.journal();
    journal.records = 1;
    let result = fixture.write(&journal, "created", b"created");
    journal.undo(&result.record).unwrap();
    let before = journal.accounted_bytes().unwrap();
    assert!(before > 0);
    assert!(journal.reserve(1, true).is_err());
    journal.fault = Some("cleanupArtifact");
    let partial = journal.cleanup(&result.record, true).unwrap();
    assert!(!partial.complete);
    assert_eq!(partial.removed, 1);
    let partial_bytes = journal.accounted_bytes().unwrap();
    assert!(partial_bytes > 0);
    assert!(journal.reserve(1, true).is_err());
    journal.fault = None;
    assert!(journal.cleanup(&result.record, true).unwrap().complete);
    assert_eq!(journal.accounted_bytes().unwrap(), 0);
    assert!(journal.reserve(1, true).is_ok());
    fixture.write(&journal, "next", b"next");
    assert_eq!(std::fs::read(fixture.path.join("repo/next")).unwrap(), b"next");
}

#[test]
fn nested_pending_reverse_requires_explicit_linked_recovery_without_recursive_loading() {
    let fixture = Fixture::new();
    let mut journal = fixture.journal();
    let result = fixture.write(&journal, "file", b"after");
    journal.fault = Some("reverseApplied");
    assert!(journal.undo(&result.record).unwrap_err().applied);
    journal.fault = None;
    let original = journal.load(&result.record).unwrap();
    let State::Undoing {
        reverse: Some(reverse),
        ..
    } = original.revision.state
    else {
        panic!()
    };
    journal.fault = Some("undoing");
    assert!(journal.undo(&reverse).is_err());
    journal.fault = None;
    let before = fixture
        .root()
        .parent("file", false)
        .unwrap()
        .snapshot()
        .unwrap();
    let mut original = journal.load(&result.record).unwrap();
    let fingerprint = original.revision.fingerprint().unwrap();
    assert!(journal.reconcile_loaded(&mut original).is_err());
    assert_eq!(
        journal
            .load(&result.record)
            .unwrap()
            .revision
            .fingerprint()
            .unwrap(),
        fingerprint
    );
    assert_eq!(
        fixture
            .root()
            .parent("file", false)
            .unwrap()
            .snapshot()
            .unwrap(),
        before
    );
    assert_eq!(journal.export(&result.record, true).unwrap(), b"before");
    assert_eq!(journal.export(&reverse, false).unwrap(), b"before");
    journal.undo(&reverse).unwrap();
    let mut original = journal.load(&result.record).unwrap();
    journal.reconcile_loaded(&mut original).unwrap();
    assert_eq!(original.revision.state.name(), "conflict");
    assert_eq!(
        std::fs::read(fixture.path.join("repo/file")).unwrap(),
        b"after"
    );
}

#[test]
fn persistent_inventory_streams_deep_parent_handles_below_the_live_handle_bound() {
    let fixture = Fixture::new();
    let journal = fixture.journal();
    for index in 0..16 {
        let mut relative = format!("parent-{index}");
        for _ in 0..30 {
            relative.push_str("/nested");
        }
        std::fs::create_dir_all(fixture.path.join("repo").join(&relative)).unwrap();
        relative.push_str("/file");
        fixture.write(&journal, &relative, b"new");
    }
    assert_eq!(journal.list().unwrap().len(), 16);
    journal.reserve(1, false).unwrap();
}

#[test]
fn recovery_store_inside_a_differently_spelled_root_refuses_before_record_allocation() {
    let fixture = Fixture::new();
    let app_data = fixture.path.join("repo/data");
    std::fs::create_dir(&app_data).unwrap();
    let journal = Journal::open(&app_data).unwrap();
    let root = Root::open(&fixture.path.join("repo/../repo"), &[]).unwrap();
    let expected = root.parent("file", false).unwrap().snapshot().unwrap();
    assert!(journal.replace(&root, "file", &expected, b"after").is_err());
    assert_eq!(journal.directory.names(8).unwrap(), vec!["lock"]);
    assert_eq!(
        root.parent("file", false).unwrap().snapshot().unwrap(),
        expected
    );
}

#[test]
fn private_namespace_bind_alias_cannot_place_recovery_inside_the_physical_root() {
    let fixture = Fixture::new();
    std::fs::create_dir(fixture.path.join("namespace-alias")).unwrap();
    let namespace = std::fs::read_link("/proc/self/ns/mnt").unwrap();
    let script = r#"import os,pathlib,subprocess,sys
root=pathlib.Path(sys.argv[1]);exe=sys.argv[2];original=sys.argv[3]
assert root.is_absolute() and root.resolve()==root
assert (root/'.skein-journal-fixture').read_bytes()==b'skein-journal-fixture-v1\n'
assert os.readlink('/proc/self/ns/mnt')!=original
subprocess.run(['mount','--bind',str(root/'repo'),str(root/'namespace-alias')],check=True,timeout=10)
env=os.environ.copy();env['SKEIN_JOURNAL_FIXTURE']=str(root);env['SKEIN_JOURNAL_CHILD_MODE']='insideBind'
subprocess.run([exe,'--exact','linux_journal::tests::native_child','--ignored','--nocapture','--test-threads=1'],env=env,check=True,timeout=20)
assert (root/'repo/file').read_bytes()==b'before'
assert (root/'outside').read_bytes()==b'outside-sentinel'
"#;
    let output = std::process::Command::new("unshare")
        .args(["-Urnm", "python3", "-c", script])
        .arg(&fixture.path)
        .arg(std::env::current_exe().unwrap())
        .arg(&namespace)
        .output()
        .unwrap();
    std::fs::write(fixture.path.join("bind-stdout.log"), &output.stdout).unwrap();
    std::fs::write(fixture.path.join("bind-stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_link("/proc/self/ns/mnt").unwrap(), namespace);
    assert!(fixture.path.join("bind-outside-proof.json").exists());
}

#[test]
fn private_namespace_nested_bind_alias_cannot_hide_recovery_root_ancestry() {
    let fixture = Fixture::new();
    std::fs::create_dir(fixture.path.join("repo/nested-bind-data")).unwrap();
    std::fs::create_dir(fixture.path.join("namespace-alias")).unwrap();
    let namespace = std::fs::read_link("/proc/self/ns/mnt").unwrap();
    let script = r#"import os,pathlib,subprocess,sys
root=pathlib.Path(sys.argv[1]);exe=sys.argv[2];original=sys.argv[3]
assert root.is_absolute() and root.resolve()==root
assert (root/'.skein-journal-fixture').read_bytes()==b'skein-journal-fixture-v1\n'
assert os.readlink('/proc/self/ns/mnt')!=original
subprocess.run(['mount','--bind',str(root/'repo/nested-bind-data'),str(root/'namespace-alias')],check=True,timeout=10)
env=os.environ.copy();env['SKEIN_JOURNAL_FIXTURE']=str(root);env['SKEIN_JOURNAL_CHILD_MODE']='nestedBind'
subprocess.run([exe,'--exact','linux_journal::tests::native_child','--ignored','--nocapture','--test-threads=1'],env=env,check=True,timeout=20)
assert (root/'repo/file').read_bytes()==b'before'
assert (root/'outside').read_bytes()==b'outside-sentinel'
"#;
    let output = std::process::Command::new("unshare")
        .args(["-Urnm", "python3", "-c", script])
        .arg(&fixture.path)
        .arg(std::env::current_exe().unwrap())
        .arg(&namespace)
        .output()
        .unwrap();
    std::fs::write(fixture.path.join("nested-bind-stdout.log"), &output.stdout).unwrap();
    std::fs::write(fixture.path.join("nested-bind-stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_link("/proc/self/ns/mnt").unwrap(), namespace);
    assert!(fixture.path.join("nested-bind-proof.json").exists());
}

fn recovery_state_bytes(journal: &Journal, id: &str) -> Vec<(String, Vec<u8>)> {
    let directory = journal.record_dir(id).unwrap();
    directory
        .names(ARTIFACT_LIMIT)
        .unwrap()
        .into_iter()
        .map(|name| {
            let bytes = directory
                .file(&name, false)
                .unwrap()
                .read(crate::linux_guard::FILE_LIMIT)
                .unwrap();
            (name, bytes)
        })
        .collect()
}

#[test]
fn resumed_created_undo_refuses_replaced_lock_before_target_or_state_mutation() {
    let fixture = Fixture::new();
    let mut journal = fixture.journal();
    let result = fixture.write(&journal, "created", b"new");
    journal.fault = Some("undoing");
    assert!(!journal.undo(&result.record).unwrap_err().applied);
    journal.fault = None;
    let parent = fixture.root().parent("created", false).unwrap();
    let expected = parent.snapshot().unwrap();
    let states = recovery_state_bytes(&journal, &result.record);
    std::fs::rename(
        fixture.path.join("data/linux-recovery-v1/lock"),
        fixture.path.join("retained-lock"),
    )
    .unwrap();
    let replacement = fixture.journal();
    let error = journal.undo(&result.record).unwrap_err();
    let current = parent.snapshot().unwrap();
    let current_states = recovery_state_bytes(&replacement, &result.record);
    std::fs::write(fixture.path.join("undo-entry-authority.json"), serde_json::to_vec_pretty(&serde_json::json!({"applied":error.applied,"targetUnchanged":current==expected,"statesUnchanged":current_states==states,"record":result.record})).unwrap()).unwrap();
    assert!(!error.applied);
    assert_eq!(current, expected);
    assert_eq!(current_states, states);
}

#[test]
fn final_created_undo_refuses_replaced_lock_after_target_validation() {
    let fixture = Fixture::new();
    let mut journal = fixture.journal();
    let result = fixture.write(&journal, "created", b"new");
    journal.fault = Some("undoing");
    assert!(!journal.undo(&result.record).unwrap_err().applied);
    journal.fault = None;
    let parent = fixture.root().parent("created", false).unwrap();
    let expected = parent.snapshot().unwrap();
    let states = recovery_state_bytes(&journal, &result.record);
    let mut replacement = None;
    let error = journal
        .undo_inner(&result.record, || {
            assert_eq!(parent.snapshot().unwrap(), expected);
            std::fs::rename(
                fixture.path.join("data/linux-recovery-v1/lock"),
                fixture.path.join("retained-lock"),
            )
            .unwrap();
            replacement = Some(fixture.journal());
            Ok(())
        })
        .unwrap_err();
    let current = parent.snapshot().unwrap();
    let current_states = recovery_state_bytes(replacement.as_ref().unwrap(), &result.record);
    std::fs::write(fixture.path.join("undo-final-authority.json"), serde_json::to_vec_pretty(&serde_json::json!({"applied":error.applied,"targetUnchanged":current==expected,"statesUnchanged":current_states==states,"record":result.record})).unwrap()).unwrap();
    assert!(!error.applied);
    assert_eq!(current, expected);
    assert_eq!(current_states, states);
}

fn externally_owned_child_fault_probe(mode: &str) {
    let fixture = Fixture::new();
    let script = r#"import ctypes,json,os,pathlib,resource,signal,subprocess,sys,time
fixture=pathlib.Path(sys.argv[1]);exe=sys.argv[2];mode=sys.argv[3]
assert fixture.is_absolute() and fixture.resolve()==fixture
assert (fixture/'.skein-journal-fixture').read_bytes()==b'skein-journal-fixture-v1\n'
assert '/repair-1/native/' in str(fixture)
resource.setrlimit(resource.RLIMIT_CORE,(0,0))
assert ctypes.CDLL(None,use_errno=True).prctl(36,1,0,0,0)==0
env=os.environ.copy();env['SKEIN_JOURNAL_FIXTURE']=str(fixture);env['SKEIN_JOURNAL_CHILD_MODE']=mode
env.pop('SKEIN_JOURNAL_KILL_PHASE',None)
if mode=='pidfdProbe':env['SKEIN_JOURNAL_PIDFD_FAIL']='1'
else:env.pop('SKEIN_JOURNAL_PIDFD_FAIL',None)
with (fixture/'probe-stdout.log').open('wb') as stdout,(fixture/'probe-stderr.log').open('wb') as stderr:
 helper=subprocess.Popen([exe,'--exact','linux_journal::tests::native_child','--ignored','--nocapture','--test-threads=1'],env=env,stdout=stdout,stderr=stderr)
 helper_fd=None
 try:
  try:helper_fd=os.pidfd_open(helper.pid)
  except OSError:
   helper.kill();code=helper.wait(timeout=5)
  else:
   try:code=helper.wait(timeout=20)
   except subprocess.TimeoutExpired:
    signal.pidfd_send_signal(helper_fd,signal.SIGKILL);code=helper.wait(timeout=5)
 finally:
  if helper.poll() is None:
   helper.kill();helper.wait(timeout=5)
  if helper_fd is not None:os.close(helper_fd)
owned=json.loads((fixture/'spawned-child.json').read_text());assert owned['parent']==helper.pid
pid=owned['pid'];external_cleanup=False;adopted_status=None
try:
 child_fd=os.pidfd_open(pid)
except ProcessLookupError:child_fd=None
if child_fd is not None:
 try:
  status=pathlib.Path('/proc')/str(pid)/'status'
  parent=int(next(line.split()[1] for line in status.read_text().splitlines() if line.startswith('PPid:')))
  assert parent==os.getpid()
  external_cleanup=True;signal.pidfd_send_signal(child_fd,signal.SIGKILL)
  end=time.monotonic()+5
  while True:
   reaped,adopted_status=os.waitpid(pid,os.WNOHANG)
   if reaped==pid:break
   assert time.monotonic()<end;time.sleep(.01)
 finally:os.close(child_fd)
try:
 remaining=os.waitpid(-1,os.WNOHANG)
except ChildProcessError:remaining=None
assert remaining is None
assert not (pathlib.Path('/proc')/str(pid)).exists()
assert (fixture/'outside').read_bytes()==b'outside-sentinel'
proof={'mode':mode,'helperPid':helper.pid,'helperExit':code,'childPid':pid,'externallyOwned':True,'externalCleanupRequired':external_cleanup,'adoptedWaitStatus':adopted_status,'allChildrenReaped':True,'sentinelPreserved':True}
(fixture/'child-fault-proof.json').write_text(json.dumps(proof,indent=2)+'\n')
print(json.dumps(proof),flush=True)
assert code==0 and not external_cleanup
"#;
    let output = std::process::Command::new("python3")
        .args(["-c", script])
        .arg(&fixture.path)
        .arg(std::env::current_exe().unwrap())
        .arg(mode)
        .output()
        .unwrap();
    std::fs::write(fixture.path.join("external-owner-stdout.log"), &output.stdout).unwrap();
    std::fs::write(fixture.path.join("external-owner-stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn failed_pidfd_acquisition_keeps_immediate_cleanup_ownership() {
    externally_owned_child_fault_probe("pidfdProbe");
}
#[test]
fn exit_before_checkpoint_records_reaping_without_a_panicking_drop() {
    externally_owned_child_fault_probe("earlyExitProbe");
}
