use super::*;
#[test]
fn final_mkdir_slot_revalidates_root_metadata_prefix_namespace_lock_and_exact_revision() {
    for mode in [
        "root",
        "metadata",
        "prefix",
        "lock",
        "namespace",
        "creatingRevision",
        "revocation",
    ] {
        let fixture = Fixture::new();
        std::fs::create_dir(fixture.path.join("repo/prefix")).unwrap();
        let journal = fixture.journal();
        let mut authority = Authority::default();
        let revoked = authority.revoked.clone();
        let path = fixture.path.clone();
        parents::PARENT_JOURNAL_HOOK.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move |phase, id| {
                if phase == "parentBeforeMkdir" {
                    if mode == "creatingRevision" {
                        let state = path
                            .join("data/linux-recovery-v1")
                            .join(id)
                            .join("state-0002.json");
                        let mut revision =
                            parent_decode::<ParentRevision>(&std::fs::read(&state).unwrap())
                                .unwrap();
                        if let ParentState::Creating { next, .. } = &mut revision.state {
                            *next = 1;
                        }
                        std::fs::write(&state, parent_encode(&revision).unwrap()).unwrap();
                    } else if mode == "revocation" {
                        revoked.store(true, Ordering::Release);
                    } else {
                        super::substitutions::substitute(&path, mode);
                    }
                }
                Ok(())
            }))
        });
        let result = journal.replace_with_parents(
            &fixture.plan("prefix/new/target"),
            &Snapshot::Missing,
            b"after",
            &mut authority,
        );
        parents::PARENT_JOURNAL_HOOK.with(|slot| *slot.borrow_mut() = None);
        if mode == "root" {
            std::fs::remove_dir(fixture.path.join("repo")).unwrap();
            std::fs::rename(fixture.path.join("saved-root"), fixture.path.join("repo")).unwrap();
        }
        if mode == "metadata" {
            std::fs::remove_dir(fixture.path.join("repo/.git")).unwrap();
            std::fs::rename(
                fixture.path.join("saved-metadata"),
                fixture.path.join("repo/.git"),
            )
            .unwrap();
        }
        let error = result.unwrap_err();
        assert!(error.parent_record.is_some(), "{mode}");
        assert!(error.created.is_empty(), "{mode}");
        assert!(
            !error.error.applied && error.error.record.is_none(),
            "{mode}"
        );
        assert!(!fixture.path.join("repo/prefix/new").exists(), "{mode}");
        assert!(!fixture.path.join("saved-prefix/new").exists(), "{mode}");
        std::fs::write(fixture.path.join("mkdir-substitution-proof.json"), serde_json::to_vec(&serde_json::json!({"case":mode,"phase":"parentBeforeMkdir","parentRecord":error.parent_record,"created":0,"fileApplied":false,"nextComponentAbsent":true})).unwrap()).unwrap();
    }
}
#[test]
fn exact_linking_revision_and_file_identity_cannot_be_replaced_by_an_arbitrary_binding() {
    let fixture = Fixture::new();
    let journal = fixture.journal();
    let path = fixture.path.clone();
    parents::PARENT_JOURNAL_HOOK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |phase, id| {
            if phase == "parentLinking" {
                let directory = path.join("data/linux-recovery-v1").join(id);
                let states = std::fs::read_dir(&directory)
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .filter(|path| {
                        path.file_name()
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .starts_with("state-")
                    })
                    .collect::<std::collections::BTreeSet<_>>();
                let latest = states.last().unwrap();
                let mut revision =
                    parent_decode::<ParentRevision>(&std::fs::read(latest).unwrap()).unwrap();
                if let ParentState::Linking { file_record, .. } = &mut revision.state {
                    *file_record = format!("r-{}", "f".repeat(32));
                }
                std::fs::write(latest, parent_encode(&revision).unwrap()).unwrap();
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
    assert_eq!(error.created.len(), 1);
    assert!(error.error.record.is_none() && !error.error.applied);
    assert!(!fixture.path.join("repo/first/target").exists());
    let loaded = journal
        .load_parent(error.parent_record.as_ref().unwrap())
        .unwrap();
    let current = parents::binding::ParentBinding {
        parent_record: loaded.intent.id.clone(),
        revision_fingerprint: loaded.revision.fingerprint().unwrap(),
        file_record: format!("r-{}", "f".repeat(32)),
    };
    journal.verify_parent_binding(&current).unwrap();
    let mut changed = current.clone();
    changed.revision_fingerprint = "a".repeat(64);
    assert!(journal.verify_parent_binding(&changed).is_err());
    changed = current;
    changed.file_record = format!("r-{}", "e".repeat(32));
    assert!(journal.verify_parent_binding(&changed).is_err());
}

#[test]
fn native_post_mkdir_boundaries_keep_uncertain_identity_out_of_the_durable_prefix() {
    for phase in ["mkdir", "reopened", "parentSync"] {
        let fixture = Fixture::new();
        let journal = fixture.journal();
        crate::linux_guard::mutation::set_parent_hook(Some(Box::new(move |at| {
            if at == phase {
                Err(crate::linux_guard::Error::conflict(
                    "Owned post-mkdir fault",
                ))
            } else {
                Ok(())
            }
        })));
        let result = journal.replace_with_parents(
            &fixture.plan("first/next/target"),
            &Snapshot::Missing,
            b"after",
            &mut Authority::default(),
        );
        crate::linux_guard::mutation::set_parent_hook(None);
        let error = result.unwrap_err();
        assert!(error.created.is_empty());
        assert_eq!(error.uncertain, Some(0));
        assert!(!error.error.applied && error.error.record.is_none());
        assert!(fixture.path.join("repo/first").is_dir());
        assert!(!fixture.path.join("repo/first/next").exists());
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
    }
}

#[test]
fn stale_missing_preview_refuses_before_any_parent_record_allocation() {
    let fixture = Fixture::new();
    let journal = fixture.journal();
    let plan = fixture.plan("first/target");
    std::fs::create_dir(fixture.path.join("repo/first")).unwrap();
    std::fs::set_permissions(
        fixture.path.join("repo/first"),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let identity = fixture
        .root()
        .preview_parents("first/target")
        .unwrap()
        .existing;
    let error = journal
        .replace_with_parents(
            &plan,
            &Snapshot::Missing,
            b"after",
            &mut Authority::default(),
        )
        .unwrap_err();
    assert!(error.parent_record.is_none());
    assert_eq!(journal.directory.names(3).unwrap(), ["lock"]);
    assert_eq!(
        fixture
            .root()
            .preview_parents("first/target")
            .unwrap()
            .existing,
        identity
    );
    assert!(!fixture.path.join("repo/first/target").exists());
}
