use super::*;
use crate::linux_guard::mutation::{OperationAuthority, ParentCreation, Snapshot};
use parents::{ParentFileReference, ParentRecoveryStage};
pub(super) struct Authority;
impl OperationAuthority for Authority {
    fn refresh(&mut self) -> Result<(), crate::linux_guard::Error> {
        Ok(())
    }
    fn check(&self) -> Result<(), crate::linux_guard::Error> {
        Ok(())
    }
}
fn target(path: &Path) -> serde_json::Value {
    let mut result = serde_json::Map::new();
    for name in ["first", "first/next", "first/target", "first/next/target"] {
        let path = path.join("repo").join(name);
        if let Ok(stat) = std::fs::metadata(&path) {
            result.insert(name.into(), serde_json::json!({"inode":stat.ino(),"device":stat.dev(),"uid":stat.uid(),"gid":stat.gid(),"mode":stat.mode(),"bytes":if stat.is_file() { Some(std::fs::read(path).unwrap()) } else { None }}));
        }
    }
    result.into()
}
pub(super) fn artifacts(path: &Path) -> serde_json::Value {
    let stat = std::fs::symlink_metadata(path).unwrap();
    let mut value = serde_json::json!({"device":stat.dev(),"inode":stat.ino(),"uid":stat.uid(),"gid":stat.gid(),"mode":stat.mode()});
    if stat.is_file() {
        value["bytes"] = serde_json::to_value(std::fs::read(path).unwrap()).unwrap();
    }
    if stat.is_symlink() {
        value["link"] = std::fs::read_link(path).unwrap().to_str().unwrap().into();
    }
    if stat.is_dir() {
        let entries = std::fs::read_dir(path)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<std::collections::BTreeSet<_>>();
        let children = entries
            .into_iter()
            .map(|child| {
                (
                    child.file_name().unwrap().to_str().unwrap().to_string(),
                    artifacts(&child),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        value["children"] = children.into();
    }
    value
}
#[test]
fn restart_creating_observes_absence_or_unproved_existence_without_resuming() {
    for applied in [false, true] {
        let fixture = Fixture::new();
        let journal = fixture.journal();
        let plan = fixture.plan("first/next/target");
        let mut loaded = journal.prepare_parent(&plan).unwrap();
        journal
            .append_parent(
                &mut loaded,
                ParentState::Creating {
                    created: Vec::new(),
                    next: 0,
                },
            )
            .unwrap();
        if applied {
            let mutation = fixture.root().create_parent_component(
                ParentCreation {
                    plan: &plan,
                    created: &[],
                },
                &mut Authority,
                || Ok(()),
            );
            assert!(mutation.error.is_none() && mutation.applied);
        }
        let id = loaded.intent.id.clone();
        let before = target(&fixture.path);
        drop(journal);
        let journal = Journal::open_existing(&fixture.path.join("data"))
            .unwrap()
            .unwrap();
        journal.reconcile().unwrap();
        let row = journal.list_parents().unwrap().pop().unwrap();
        assert_eq!(row.id, id);
        assert_eq!(row.created, 0);
        assert_eq!(target(&fixture.path), before);
        assert_eq!(
            journal.accounted_bytes().unwrap(),
            loaded.intent.reserved_bytes
        );
        if applied {
            assert_eq!(row.stage, ParentRecoveryStage::Conflict);
            assert_eq!(row.uncertain, Some(0));
            assert!(journal.cleanup_parent(&id, true).is_err());
            assert!(journal.acknowledge_parent(&id, false).is_err());
            let resolved = journal.acknowledge_parent(&id, true).unwrap();
            assert_eq!(resolved.stage, ParentRecoveryStage::Resolved);
            assert_eq!(resolved.created, 0);
            assert_eq!(resolved.uncertain, Some(0));
            assert!(journal.cleanup_parent(&id, true).unwrap().complete);
            assert_eq!(target(&fixture.path), before);
        } else {
            assert_eq!(row.stage, ParentRecoveryStage::Retained);
            assert!(matches!(
                journal.load_parent(&id).unwrap().revision.state,
                ParentState::Retained {
                    reason: RetainReason::Interrupted,
                    ..
                }
            ));
        }
        assert!(!fixture.path.join("repo/first/next").exists());
    }
}
#[test]
fn interrupted_prepared_created_ready_and_absent_linking_are_observation_only() {
    for phase in [
        "parentPrepared",
        "parentCreated",
        "parentReady",
        "parentLinking",
    ] {
        let fixture = Fixture::new();
        let mut journal = fixture.journal();
        journal.fault = Some(phase);
        let error = journal
            .replace_with_parents(
                &fixture.plan("first/target"),
                &Snapshot::Missing,
                b"after",
                &mut Authority,
            )
            .unwrap_err();
        let id = error.parent_record.unwrap();
        let before = target(&fixture.path);
        drop(journal);
        let journal = Journal::open_existing(&fixture.path.join("data"))
            .unwrap()
            .unwrap();
        journal.reconcile().unwrap();
        let row = journal.list_parents().unwrap().pop().unwrap();
        assert_eq!(row.stage, ParentRecoveryStage::Retained);
        assert_eq!(row.created, u32::from(phase != "parentPrepared"));
        assert_eq!(target(&fixture.path), before);
        assert!(!fixture.path.join("repo/first/target").exists());
        if phase == "parentLinking" {
            assert_eq!(row.file_reference, ParentFileReference::Missing);
        }
        assert!(journal.cleanup_parent(&id, true).unwrap().complete);
        assert_eq!(target(&fixture.path), before);
    }
}
#[test]
fn historical_linked_parent_survives_file_undo_cleanup_and_missing_source() {
    let fixture = Fixture::new();
    let journal = fixture.journal();
    let publication = journal
        .replace_with_parents(
            &fixture.plan("first/target"),
            &Snapshot::Missing,
            b"after",
            &mut Authority,
        )
        .unwrap();
    let id = publication.parent_record.unwrap();
    let original = journal
        .load_parent(&id)
        .unwrap()
        .revision
        .fingerprint()
        .unwrap();
    assert_eq!(journal.list().unwrap().len(), 1);
    journal.undo(&publication.file.record).unwrap();
    assert!(!fixture.path.join("repo/first/target").exists());
    assert!(fixture.path.join("repo/first").exists());
    let row = journal.list_parents().unwrap().pop().unwrap();
    assert_eq!(row.stage, ParentRecoveryStage::Linked);
    assert_eq!(row.file_reference, ParentFileReference::MatchingTerminal);
    assert!(
        journal
            .cleanup(&publication.file.record, true)
            .unwrap()
            .complete
    );
    let row = journal.list_parents().unwrap().pop().unwrap();
    assert_eq!(row.stage, ParentRecoveryStage::Linked);
    assert_eq!(row.file_reference, ParentFileReference::Missing);
    assert!(row.warning.is_some());
    assert_eq!(
        journal
            .load_parent(&id)
            .unwrap()
            .revision
            .fingerprint()
            .unwrap(),
        original
    );
    journal
        .replace(&fixture.root(), "probe", &Snapshot::Missing, b"probe")
        .unwrap();
    drop(journal);
    std::fs::rename(fixture.path.join("repo"), fixture.path.join("saved-root")).unwrap();
    let before = artifacts(&fixture.path.join("saved-root/first"));
    let journal = Journal::open_existing(&fixture.path.join("data"))
        .unwrap()
        .unwrap();
    assert_eq!(
        journal.list_parents().unwrap()[0].stage,
        ParentRecoveryStage::Linked
    );
    let result = journal.cleanup_parent(&id, true).unwrap();
    assert!(result.complete && result.warning.is_none());
    assert_eq!(artifacts(&fixture.path.join("saved-root/first")), before);
    std::fs::rename(fixture.path.join("saved-root"), fixture.path.join("repo")).unwrap();
}
#[test]
fn terminal_parent_cleanup_uses_prepaid_capacity_and_counts_private_record_files() {
    let fixture = Fixture::new();
    let mut journal = fixture.journal();
    let mut loaded = journal
        .prepare_parent(&fixture.plan("first/target"))
        .unwrap();
    journal
        .append_parent(
            &mut loaded,
            ParentState::Retained {
                created: Vec::new(),
                file_record: None,
                reason: RetainReason::Cancelled,
            },
        )
        .unwrap();
    journal.quota = loaded.intent.reserved_bytes;
    assert_eq!(journal.accounted_bytes().unwrap(), journal.quota);
    assert!(journal.cleanup_parent(&loaded.intent.id, false).is_err());
    let result = journal.cleanup_parent(&loaded.intent.id, true).unwrap();
    assert!(result.complete);
    assert_eq!(result.removed, 3);
    assert_eq!(journal.directory.names(3).unwrap(), ["lock"]);
    assert_eq!(journal.accounted_bytes().unwrap(), 0);
    assert!(!fixture.path.join("repo/first").exists());
}
#[test]
fn outside_parent_cleanup_proof_keeps_full_charge_until_proof_last_removal() {
    for phase in [
        "parentCleanupPrepared",
        "parentCleanupArtifact",
        "parentCleanupDirectory",
        "parentCleanupProofUnlinked",
    ] {
        let fixture = Fixture::new();
        let mut journal = fixture.journal();
        let publication = journal
            .replace_with_parents(
                &fixture.plan("first/target"),
                &Snapshot::Missing,
                b"after",
                &mut Authority,
            )
            .unwrap();
        let id = publication.parent_record.unwrap();
        let reserve = journal.load_parent(&id).unwrap().intent.reserved_bytes;
        let file = journal.directory.path().join(&publication.file.record);
        let file_before = artifacts(&file);
        let before = target(&fixture.path);
        let total = journal.accounted_bytes().unwrap();
        journal.fault = Some(phase);
        let result = journal.cleanup_parent(&id, true);
        if phase == "parentCleanupPrepared" {
            assert!(result.is_err());
        } else {
            let result = result.unwrap();
            assert!(!result.complete && result.warning.is_some());
        }
        assert_eq!(target(&fixture.path), before);
        assert_eq!(artifacts(&file), file_before);
        journal.fault = None;
        if phase != "parentCleanupProofUnlinked" {
            assert_eq!(journal.accounted_bytes().unwrap(), total);
            assert_eq!(journal.parent_usage().unwrap().records, 1);
            assert_eq!(
                journal.list_parents().unwrap()[0].stage,
                ParentRecoveryStage::CleanupPending
            );
        } else {
            assert_eq!(journal.accounted_bytes().unwrap(), total - reserve);
            assert!(journal.list_parents().unwrap().is_empty());
        }
        let result = journal.cleanup_parent(&id, true).unwrap();
        assert!(result.complete);
        assert_eq!(target(&fixture.path), before);
        assert_eq!(artifacts(&file), file_before);
        std::fs::write(fixture.path.join("cleanup-capacity-proof.json"), serde_json::to_vec(&serde_json::json!({"phase":phase,"record":id,"reservedBytes":reserve,"fullChargeUntilProofRemoval":true,"fileRecordUnchanged":true,"targetsUnchanged":true,"completedOnExplicitResume":true})).unwrap()).unwrap();
    }
}
#[test]
fn changed_or_unknown_proof_backed_parent_artifacts_are_retained_before_any_unlink() {
    for mode in [
        "bytes", "identity", "metadata", "link", "hardlink", "unknown",
    ] {
        let fixture = Fixture::new();
        let mut journal = fixture.journal();
        let published = journal
            .replace_with_parents(
                &fixture.plan("first/target"),
                &Snapshot::Missing,
                b"after",
                &mut Authority,
            )
            .unwrap();
        let id = published.parent_record.unwrap();
        journal.fault = Some("parentCleanupPrepared");
        assert!(journal.cleanup_parent(&id, true).is_err());
        journal.fault = None;
        let directory = journal.directory.path().join(&id);
        let artifact = directory.join("state-0001.json");
        match mode {
            "bytes" => std::fs::write(&artifact, b"changed").unwrap(),
            "identity" => {
                std::fs::rename(&artifact, fixture.path.join("saved-state")).unwrap();
                std::fs::write(&artifact, b"changed").unwrap();
                std::fs::set_permissions(&artifact, std::fs::Permissions::from_mode(0o600))
                    .unwrap();
            }
            "metadata" => {
                std::fs::set_permissions(&artifact, std::fs::Permissions::from_mode(0o644)).unwrap()
            }
            "link" => {
                std::fs::rename(&artifact, fixture.path.join("saved-state")).unwrap();
                std::os::unix::fs::symlink(fixture.path.join("outside"), &artifact).unwrap();
            }
            "hardlink" => std::fs::hard_link(&artifact, fixture.path.join("state-link")).unwrap(),
            "unknown" => {
                std::fs::write(directory.join("unknown"), b"unknown").unwrap();
                std::fs::set_permissions(
                    directory.join("unknown"),
                    std::fs::Permissions::from_mode(0o600),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let before = artifacts(&directory);
        let proof = journal
            .directory
            .path()
            .join(format!("parent-cleanup-{id}.json"));
        let proof_before = artifacts(&proof);
        let result = journal.cleanup_parent(&id, true);
        if let Ok(result) = result {
            assert!(!result.complete && result.removed == 0);
        }
        assert_eq!(artifacts(&directory), before);
        assert_eq!(artifacts(&proof), proof_before);
        assert_eq!(
            std::fs::read(fixture.path.join("repo/first/target")).unwrap(),
            b"after"
        );
    }
}
#[test]
fn final_cleanup_slot_revalidates_outside_proof_and_flock_before_unlink() {
    for mode in ["proof", "lock"] {
        let fixture = Fixture::new();
        let mut journal = fixture.journal();
        let published = journal
            .replace_with_parents(
                &fixture.plan("first/target"),
                &Snapshot::Missing,
                b"after",
                &mut Authority,
            )
            .unwrap();
        let id = published.parent_record.unwrap();
        journal.fault = Some("parentCleanupPrepared");
        assert!(journal.cleanup_parent(&id, true).is_err());
        journal.fault = None;
        let directory = journal.directory.path().join(&id);
        let before = artifacts(&directory);
        let namespace = journal.directory.path().to_path_buf();
        let proof_name = format!("parent-cleanup-{id}.json");
        let fired = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let used = fired.clone();
        crate::linux_guard::storage::DIFF_STORAGE_HOOK.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move |phase| {
                if phase == "unlink-file" && !used.swap(true, Ordering::AcqRel) {
                    if mode == "proof" {
                        std::fs::write(namespace.join(&proof_name), b"changed-proof").unwrap();
                    } else {
                        std::fs::rename(namespace.join("lock"), namespace.join("saved-lock"))
                            .unwrap();
                        std::fs::write(namespace.join("lock"), b"").unwrap();
                        std::fs::set_permissions(
                            namespace.join("lock"),
                            std::fs::Permissions::from_mode(0o600),
                        )
                        .unwrap();
                    }
                }
                Ok(())
            }))
        });
        let result = journal.cleanup_parent(&id, true);
        crate::linux_guard::storage::DIFF_STORAGE_HOOK.with(|slot| *slot.borrow_mut() = None);
        assert!(fired.load(Ordering::Acquire));
        let result = result.unwrap();
        assert!(!result.complete && result.removed == 0);
        assert_eq!(artifacts(&directory), before);
        assert_eq!(
            std::fs::read(fixture.path.join("repo/first/target")).unwrap(),
            b"after"
        );
    }
}
#[test]
fn separate_parent_rows_keep_foreign_and_incomplete_values_empty() {
    let fixture = Fixture::new();
    let journal = fixture.journal();
    let foreign = "p-invalid";
    let broken = format!("p-{}", "a".repeat(32));
    journal.directory.create_child(foreign).unwrap();
    journal
        .directory
        .create_child(&broken)
        .unwrap()
        .write_new("intent.json", b"{")
        .unwrap();
    let rows = journal.list_parents().unwrap();
    assert_eq!(rows.len(), 2);
    for row in rows {
        assert!(row.root.is_none() && row.destination.is_none());
        assert_eq!(row.created, 0);
        assert!(row.file_record.is_none());
        assert!(row.warning.is_some());
        assert!(matches!(
            row.stage,
            ParentRecoveryStage::Foreign | ParentRecoveryStage::Incomplete
        ));
        assert!(journal.acknowledge_parent(&row.id, true).is_err());
        assert!(journal.cleanup_parent(&row.id, true).is_err());
    }
    assert!(journal.list().unwrap().is_empty());
}

#[test]
fn final_directory_cleanup_slot_retains_substituted_or_unknown_native_artifacts() {
    for mode in ["directory", "unknown", "namespace"] {
        let fixture = Fixture::new();
        let mut journal = fixture.journal();
        let publication = journal
            .replace_with_parents(
                &fixture.plan("first/target"),
                &Snapshot::Missing,
                b"after",
                &mut Authority,
            )
            .unwrap();
        let id = publication.parent_record.unwrap();
        journal.fault = Some("parentCleanupEmpty");
        assert!(!journal.cleanup_parent(&id, true).unwrap().complete);
        journal.fault = None;
        let namespace = journal.directory.path().to_path_buf();
        let record = id.clone();
        let saved = fixture.path.join("saved-namespace");
        let fired = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let used = fired.clone();
        crate::linux_guard::storage::DIFF_STORAGE_HOOK.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move |phase| {
                if phase != "remove-directory" || used.swap(true, Ordering::AcqRel) {
                    return Ok(());
                }
                match mode {
                    "directory" => {
                        std::fs::rename(namespace.join(&record), namespace.join("saved-parent"))
                            .unwrap();
                        std::fs::create_dir(namespace.join(&record)).unwrap();
                        std::fs::set_permissions(
                            namespace.join(&record),
                            std::fs::Permissions::from_mode(0o700),
                        )
                        .unwrap();
                    }
                    "unknown" => {
                        std::fs::write(namespace.join(&record).join("unknown"), b"retained")
                            .unwrap();
                    }
                    "namespace" => {
                        std::fs::rename(&namespace, &saved).unwrap();
                        std::fs::create_dir(&namespace).unwrap();
                        std::fs::set_permissions(
                            &namespace,
                            std::fs::Permissions::from_mode(0o700),
                        )
                        .unwrap();
                    }
                    _ => unreachable!(),
                }
                Ok(())
            }));
        });
        let result = journal.cleanup_parent(&id, true);
        crate::linux_guard::storage::DIFF_STORAGE_HOOK.with(|slot| *slot.borrow_mut() = None);
        assert!(fired.load(Ordering::Acquire));
        assert!(!result.unwrap().complete);
        let actual = if mode == "namespace" {
            fixture.path.join("saved-namespace")
        } else {
            journal.directory.path().to_path_buf()
        };
        assert!(actual.join(&id).is_dir());
        assert!(actual.join(format!("parent-cleanup-{id}.json")).is_file());
        if mode == "unknown" {
            assert_eq!(
                std::fs::read(actual.join(&id).join("unknown")).unwrap(),
                b"retained"
            );
        }
        assert_eq!(
            std::fs::read(fixture.path.join("repo/first/target")).unwrap(),
            b"after"
        );
    }
}
