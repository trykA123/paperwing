use super::*;
use crate::linux_guard::mutation::{OperationAuthority, Snapshot};
use std::sync::{atomic::AtomicBool, Arc};
mod mkdir_binding;
mod substitutions;

type RefreshAction = Box<dyn FnMut(usize) -> Result<(), crate::linux_guard::Error>>;
#[derive(Default)]
struct Authority {
    refreshes: usize,
    revoked: Arc<AtomicBool>,
    action: Option<RefreshAction>,
}
impl OperationAuthority for Authority {
    fn refresh(&mut self) -> Result<(), crate::linux_guard::Error> {
        self.refreshes += 1;
        if let Some(action) = self.action.as_mut() {
            action(self.refreshes)?;
        }
        Ok(())
    }
    fn check(&self) -> Result<(), crate::linux_guard::Error> {
        if self.revoked.load(Ordering::Acquire) {
            return Err(crate::linux_guard::Error::conflict(
                "Owned authority revoked",
            ));
        }
        Ok(())
    }
}
#[test]
fn guarded_parent_route_refuses_legacy_constructor_even_with_zero_missing_parents() {
    for destination in ["new/file", "other"] {
        let fixture = Fixture::new();
        let journal = Journal::open(&fixture.path.join("data")).unwrap();
        let error = journal
            .replace_with_parents(
                &fixture.plan(destination),
                &Snapshot::Missing,
                b"after",
                &mut Authority::default(),
            )
            .unwrap_err();
        assert_eq!(
            error.error.message,
            "Guarded parent constructor authority is missing"
        );
        assert!(error.error.record.is_none() && error.parent_record.is_none());
        assert_eq!(journal.directory.names(3).unwrap(), ["lock"]);
        assert!(!fixture.path.join("repo/new").exists());
        assert!(!fixture.path.join("repo/other").exists());
    }
}
#[test]
fn authorized_zero_parent_create_and_replace_preserve_file_record_shape() {
    for replace in [false, true] {
        let fixture = Fixture::new();
        let journal = fixture.journal();
        let root = fixture.root();
        if replace {
            std::fs::write(fixture.path.join("repo/target"), b"target-before").unwrap();
        }
        let expected = root.parent("target", false).unwrap().snapshot().unwrap();
        let mut authority = Authority::default();
        let published = journal
            .replace_with_parents(&fixture.plan("target"), &expected, b"after", &mut authority)
            .unwrap();
        assert!(published.parent_record.is_none() && published.created.is_empty());
        assert_eq!(authority.refreshes, 3);
        let file = journal.load(&published.file.record).unwrap();
        assert_eq!(file.intent.before.is_some(), replace);
        assert!(matches!(file.revision.state, State::Applied { .. }));
        assert_eq!(
            std::fs::read(fixture.path.join("repo/target")).unwrap(),
            b"after"
        );
        assert_eq!(journal.directory.names(4).unwrap().len(), 2);
    }
}
#[test]
fn durable_parent_creation_links_exact_file_and_sixty_three_real_identities() {
    for count in [2usize, 63] {
        let fixture = Fixture::new();
        let journal = fixture.journal();
        let destination = (0..count)
            .map(|_| "p")
            .chain(std::iter::once("target"))
            .collect::<Vec<_>>()
            .join("/");
        let plan = fixture.plan(&destination);
        let published = journal
            .replace_with_parents(
                &plan,
                &Snapshot::Missing,
                b"after",
                &mut Authority::default(),
            )
            .unwrap();
        let parent = journal
            .load_parent(published.parent_record.as_ref().unwrap())
            .unwrap();
        assert_eq!(published.created.len(), count);
        assert_eq!(parent.revision.sequence, 2 * count as u32 + 4);
        assert_eq!(parent.revision.state.created(), published.created);
        let file = journal.load(&published.file.record).unwrap();
        assert!(
            matches!(&parent.revision.state, ParentState::Linked { file_record, file_intent, .. } if file_record == &file.intent.id && file_intent == &file.intent_hash)
        );
        assert_eq!(file.intent.ancestors, published.created);
        assert_eq!(
            std::fs::read(fixture.path.join("repo").join(destination)).unwrap(),
            b"after"
        );
        for created in &published.created {
            let path = fixture.path.join("repo").join(&created.relative);
            let native = std::fs::metadata(&path).unwrap();
            assert_eq!(
                Root::open(&path, &[]).unwrap().value().unwrap().identity,
                created.identity
            );
            assert_eq!(native.mode() & 0o777, 0o700);
        }
        assert!(parent.directory.native_size().unwrap() <= 4096);
        std::fs::write(fixture.path.join("valid-chain-proof.json"), serde_json::to_vec(&serde_json::json!({"missingParents":count,"validRevisionCount":parent.revision.sequence,"allocatedRevisionLimit":parent.intent.revisions(),"allCreatedIdentitiesNative":true,"terminalFileMatched":true,"fileIntentSha256":file.intent_hash,"directoryNativeBytes":parent.directory.native_size().unwrap()})).unwrap()).unwrap();
    }
}
#[test]
fn caller_revocation_retains_only_the_durably_created_prefix() {
    for deny_refresh in [2usize, 3] {
        let fixture = Fixture::new();
        let journal = fixture.journal();
        let mut authority = Authority::default();
        let revoked = authority.revoked.clone();
        authority.action = Some(Box::new(move |refresh| {
            if refresh == deny_refresh {
                revoked.store(true, Ordering::Release);
            }
            Ok(())
        }));
        let error = journal
            .replace_with_parents(
                &fixture.plan("first/next/target"),
                &Snapshot::Missing,
                b"after",
                &mut authority,
            )
            .unwrap_err();
        assert!(!error.error.applied && error.error.record.is_none());
        assert!(error.uncertain.is_none());
        assert_eq!(error.created.len(), deny_refresh - 2);
        let parent = journal
            .load_parent(error.parent_record.as_ref().unwrap())
            .unwrap();
        assert_eq!(parent.revision.state.created(), error.created);
        assert!(matches!(
            parent.revision.state,
            ParentState::Retained {
                reason: RetainReason::Cancelled,
                ..
            }
        ));
        assert_eq!(fixture.path.join("repo/first").exists(), deny_refresh == 3);
        assert!(!fixture.path.join("repo/first/next").exists());
    }
}
#[test]
fn unrelated_unknown_record_blocks_the_one_parent_bound_publication() {
    let fixture = Fixture::new();
    let journal = fixture.journal();
    let namespace = fixture.path.join("data/linux-recovery-v1");
    parents::PARENT_JOURNAL_HOOK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |phase, _| {
            if phase == "parentLinking" {
                std::fs::write(namespace.join("foreign"), b"foreign").unwrap();
                std::fs::set_permissions(
                    namespace.join("foreign"),
                    std::fs::Permissions::from_mode(0o600),
                )
                .unwrap();
            }
            Ok(())
        }))
    });
    let result = journal.replace_with_parents(
        &fixture.plan("first/target"),
        &Snapshot::Missing,
        b"after",
        &mut Authority::default(),
    );
    parents::PARENT_JOURNAL_HOOK.with(|slot| *slot.borrow_mut() = None);
    let error = result.unwrap_err();
    assert!(!error.error.applied && error.error.record.is_none());
    assert_eq!(error.created.len(), 1);
    assert!(!fixture.path.join("repo/first/target").exists());
    assert_eq!(
        std::fs::read(journal.directory.path().join("foreign")).unwrap(),
        b"foreign"
    );
}

#[test]
fn post_mkdir_created_revision_failures_report_uncertain_without_claiming_identity() {
    for phase in [
        "parentCreatedFileCreated",
        "parentCreatedFileWritten",
        "parentCreatedFileVerified",
        "parentCreatedRecordSynced",
        "parentCreatedNamespaceSynced",
    ] {
        let fixture = Fixture::new();
        let mut journal = fixture.journal();
        journal.fault = Some(phase);
        let error = journal
            .replace_with_parents(
                &fixture.plan("first/next/target"),
                &Snapshot::Missing,
                b"after",
                &mut Authority::default(),
            )
            .unwrap_err();
        assert!(error.created.is_empty());
        assert_eq!(error.uncertain, Some(0));
        assert!(error.parent_record.is_some());
        assert!(!error.error.applied && error.error.record.is_none());
        assert!(fixture.path.join("repo/first").is_dir());
        assert!(!fixture.path.join("repo/first/next").exists());
    }
}

#[test]
fn post_file_success_linked_failures_preserve_file_applied_outcome_and_durable_prefix() {
    for phase in [
        "parentLinkedFileCreated",
        "parentLinkedFileWritten",
        "parentLinkedFileVerified",
        "parentLinkedRecordSynced",
        "parentLinkedNamespaceSynced",
        "parentLinked",
    ] {
        let fixture = Fixture::new();
        let mut journal = fixture.journal();
        journal.fault = Some(phase);
        let error = journal
            .replace_with_parents(
                &fixture.plan("first/target"),
                &Snapshot::Missing,
                b"after",
                &mut Authority::default(),
            )
            .unwrap_err();
        assert!(error.error.applied);
        let file_id = error.error.record.as_ref().unwrap();
        assert!(record::id_valid(file_id));
        assert!(error.parent_record.is_some());
        assert_eq!(error.created.len(), 1);
        assert!(error.uncertain.is_none());
        assert_eq!(journal.export(file_id, false).unwrap(), b"after");
        assert!(matches!(
            journal.load(file_id).unwrap().revision.state,
            State::Applied { .. }
        ));
        assert_eq!(
            std::fs::read(fixture.path.join("repo/first/target")).unwrap(),
            b"after"
        );
    }
}

#[test]
fn post_rename_file_faults_preserve_independent_record_and_applied_outcome() {
    for phase in ["renamed", "directorySynced", "applied"] {
        let fixture = Fixture::new();
        let mut journal = fixture.journal();
        journal.fault = Some(phase);
        let error = journal
            .replace_with_parents(
                &fixture.plan("first/target"),
                &Snapshot::Missing,
                b"after",
                &mut Authority::default(),
            )
            .unwrap_err();
        assert!(error.error.applied && error.error.record.is_some());
        assert!(error.parent_record.is_some());
        assert_eq!(error.created.len(), 1);
        assert_eq!(
            std::fs::read(fixture.path.join("repo/first/target")).unwrap(),
            b"after"
        );
        assert!(matches!(
            journal
                .load_parent(error.parent_record.as_ref().unwrap())
                .unwrap()
                .revision
                .state,
            ParentState::Conflict { .. }
        ));
    }
}

#[test]
fn phase3_late_ready_authority_failure_keeps_the_parent_receipt() {
    let fixture = Fixture::new();
    let journal = fixture.journal();
    let path = fixture.path.clone();
    parents::PARENT_JOURNAL_HOOK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |phase, _| {
            if phase == "parentReady" {
                std::fs::rename(path.join("repo"), path.join("saved-root")).unwrap();
                std::fs::create_dir(path.join("repo")).unwrap();
            }
            Ok(())
        }))
    });
    let result = journal.replace_with_parents(
        &fixture.plan("first/target"),
        &Snapshot::Missing,
        b"after",
        &mut Authority::default(),
    );
    parents::PARENT_JOURNAL_HOOK.with(|slot| *slot.borrow_mut() = None);
    std::fs::remove_dir(fixture.path.join("repo")).unwrap();
    std::fs::rename(fixture.path.join("saved-root"), fixture.path.join("repo")).unwrap();
    let error = result.unwrap_err();
    assert!(error.parent_record.is_some());
    assert_eq!(error.created.len(), 1);
    assert!(!error.error.applied && error.error.record.is_none());
    assert!(!fixture.path.join("repo/first/target").exists());
}

#[test]
fn phase3_external_mkdir_winner_is_unproved_conflict_and_never_adopted() {
    let fixture = Fixture::new();
    let journal = fixture.journal();
    let path = fixture.path.clone();
    parents::PARENT_JOURNAL_HOOK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |phase, _| {
            if phase == "parentCreating" {
                std::fs::create_dir(path.join("repo/first")).unwrap();
                std::fs::set_permissions(
                    path.join("repo/first"),
                    std::fs::Permissions::from_mode(0o700),
                )
                .unwrap();
            }
            Ok(())
        }))
    });
    let result = journal.replace_with_parents(
        &fixture.plan("first/next/target"),
        &Snapshot::Missing,
        b"after",
        &mut Authority::default(),
    );
    parents::PARENT_JOURNAL_HOOK.with(|slot| *slot.borrow_mut() = None);
    let error = result.unwrap_err();
    assert!(error.created.is_empty());
    assert_eq!(error.uncertain, Some(0));
    assert!(matches!(
        journal
            .load_parent(error.parent_record.as_ref().unwrap())
            .unwrap()
            .revision
            .state,
        ParentState::Conflict {
            uncertain: Some(0),
            ..
        }
    ));
    assert!(!fixture.path.join("repo/first/next").exists());
}

#[test]
fn file_admission_failure_after_parent_success_preserves_directories_and_reason() {
    let fixture = Fixture::new();
    let mut journal = fixture.journal();
    journal.quota = ParentIntent::reservation(1).unwrap();
    let error = journal
        .replace_with_parents(
            &fixture.plan("first/target"),
            &Snapshot::Missing,
            b"after",
            &mut Authority::default(),
        )
        .unwrap_err();
    assert_eq!(error.created.len(), 1);
    assert!(error.error.record.is_none() && !error.error.applied);
    let parent = journal
        .load_parent(error.parent_record.as_ref().unwrap())
        .unwrap();
    assert!(matches!(
        parent.revision.state,
        ParentState::Retained {
            reason: RetainReason::FileAdmission,
            ..
        }
    ));
    assert_eq!(
        journal.accounted_bytes().unwrap(),
        parent.intent.reserved_bytes
    );
    assert!(fixture.path.join("repo/first").is_dir());
    assert!(!fixture.path.join("repo/first/target").exists());
}
