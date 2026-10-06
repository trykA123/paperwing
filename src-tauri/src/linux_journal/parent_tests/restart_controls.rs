use super::crash_controls::OwnedChild;
use super::recovery_controls::{artifacts, Authority};
use super::*;
use crate::linux_guard::mutation::Snapshot;
use parents::{ParentFileReference, ParentRecoveryStage};

#[test]
fn restart_changed_root_metadata_and_proven_prefix_classify_conflict_without_target_work() {
    for mode in ["root", "metadata", "prefix"] {
        let fixture = Fixture::new();
        let mut journal = fixture.journal();
        journal.fault = Some("parentReady");
        let error = journal
            .replace_with_parents(
                &fixture.plan("first/next/target"),
                &Snapshot::Missing,
                b"after",
                &mut Authority,
            )
            .unwrap_err();
        let id = error.parent_record.unwrap();
        assert_eq!(error.created.len(), 2);
        drop(journal);
        let relative = match mode {
            "root" => "repo",
            "metadata" => "repo/.git",
            "prefix" => "repo/first",
            _ => unreachable!(),
        };
        std::fs::rename(
            fixture.path.join(relative),
            fixture.path.join("saved-object"),
        )
        .unwrap();
        std::fs::create_dir(fixture.path.join(relative)).unwrap();
        std::fs::set_permissions(
            fixture.path.join(relative),
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        let before = artifacts(&fixture.path.join("repo"));
        let saved = artifacts(&fixture.path.join("saved-object"));
        let journal = Journal::open_existing(&fixture.path.join("data"))
            .unwrap()
            .unwrap();
        journal.reconcile().unwrap();
        let row = journal.list_parents().unwrap().pop().unwrap();
        assert_eq!(row.id, id);
        assert_eq!(row.stage, ParentRecoveryStage::Conflict);
        assert_eq!(row.created, 2);
        assert_eq!(row.uncertain, None);
        assert_eq!(artifacts(&fixture.path.join("repo")), before);
        assert_eq!(artifacts(&fixture.path.join("saved-object")), saved);
        assert!(journal.cleanup_parent(&id, true).is_err());
        std::fs::write(fixture.path.join("changed-prefix-restart-proof.json"), serde_json::to_vec_pretty(&serde_json::json!({"case":mode,"row":row,"targetBefore":before,"targetAfter":artifacts(&fixture.path.join("repo")),"savedBefore":saved,"savedAfter":artifacts(&fixture.path.join("saved-object"))})).unwrap()).unwrap();
        drop(journal);
        std::fs::remove_dir(fixture.path.join(relative)).unwrap();
        std::fs::rename(
            fixture.path.join("saved-object"),
            fixture.path.join(relative),
        )
        .unwrap();
    }
}
fn rewrite_intent(directory: &Path, intent: &Intent, sequence: u32) {
    intent.validate().unwrap();
    let encoded = encode(intent).unwrap();
    std::fs::write(directory.join("intent.json"), &encoded).unwrap();
    let mut previous = None;
    for number in 1..=sequence {
        let path = directory.join(state::name(number));
        let mut revision: Revision = decode(&std::fs::read(&path).unwrap()).unwrap();
        revision.intent = hash(&encoded);
        revision.previous = previous;
        previous = Some(revision.fingerprint().unwrap());
        std::fs::write(path, encode(&revision).unwrap()).unwrap();
    }
}
#[test]
fn restart_linking_requires_exact_terminal_file_intent_and_complete_ancestor_binding() {
    for mode in [
        "matching", "absent", "pending", "corrupt", "foreign", "root", "path", "ancestor",
    ] {
        let fixture = Fixture::new();
        let mut child = OwnedChild::spawn(&fixture.path, "fileapplied", 1, false);
        let pid = child.kill_after_ready("fileapplied");
        drop(child);
        let journal = Journal::open_existing(&fixture.path.join("data"))
            .unwrap()
            .unwrap();
        let parent = journal.list_parents().unwrap().pop().unwrap();
        assert_eq!(parent.stage, ParentRecoveryStage::Linking);
        let id = parent.id;
        let file_id = parent.file_record.unwrap();
        let loaded = journal.load(&file_id).unwrap();
        let directory = journal.directory.path().join(&file_id);
        let intent_file = directory.join("intent.json");
        match mode {
            "matching" => (),
            "absent" => {
                std::fs::rename(&directory, fixture.path.join("saved-file-record")).unwrap()
            }
            "pending" => {
                std::fs::remove_file(directory.join(state::name(loaded.revision.sequence))).unwrap()
            }
            "corrupt" => std::fs::write(&intent_file, b"{").unwrap(),
            "foreign" => {
                let mut envelope: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&intent_file).unwrap()).unwrap();
                envelope["version"] = 2.into();
                std::fs::write(&intent_file, serde_json::to_vec(&envelope).unwrap()).unwrap();
            }
            "root" | "path" | "ancestor" => {
                let mut intent = loaded.intent;
                if mode == "root" {
                    std::fs::create_dir(fixture.path.join("other-root")).unwrap();
                    intent.root = Root::open(&fixture.path.join("other-root"), &[])
                        .unwrap()
                        .value()
                        .unwrap();
                } else if mode == "path" {
                    intent.path = "first/next/other".into();
                } else {
                    intent.ancestors[0].identity = Identity::maximum_width();
                }
                rewrite_intent(&directory, &intent, loaded.revision.sequence);
                assert!(journal.load(&file_id).is_ok());
            }
            _ => unreachable!(),
        }
        let targets = artifacts(&fixture.path.join("repo"));
        let intent_before = if intent_file.exists() {
            Some(artifacts(&intent_file))
        } else {
            None
        };
        journal.reconcile_parents().unwrap();
        let row = journal.list_parents().unwrap().pop().unwrap();
        let expected = match mode {
            "matching" => ParentRecoveryStage::Linked,
            "absent" => ParentRecoveryStage::Retained,
            _ => ParentRecoveryStage::Conflict,
        };
        assert_eq!(row.stage, expected);
        assert_eq!(row.created, 2);
        assert_eq!(row.file_record.as_deref(), Some(file_id.as_str()));
        let reference = match mode {
            "matching" => ParentFileReference::MatchingTerminal,
            "absent" => ParentFileReference::Missing,
            "root" | "path" | "ancestor" => ParentFileReference::Mismatched,
            _ => ParentFileReference::Unavailable,
        };
        assert_eq!(row.file_reference, reference);
        assert_eq!(artifacts(&fixture.path.join("repo")), targets);
        if let Some(before) = &intent_before {
            assert_eq!(artifacts(&intent_file), *before);
        }
        if row.stage == ParentRecoveryStage::Conflict {
            assert!(journal.cleanup_parent(&id, true).is_err());
        }
        std::fs::write(fixture.path.join("linking-restart-proof.json"), serde_json::to_vec_pretty(&serde_json::json!({"case":mode,"pid":pid,"reaped":true,"row":row,"targetsBefore":targets,"targetsAfter":artifacts(&fixture.path.join("repo")),"intentBefore":intent_before,"intentAfter":if intent_file.exists() { Some(artifacts(&intent_file)) } else { None }})).unwrap()).unwrap();
    }
}
