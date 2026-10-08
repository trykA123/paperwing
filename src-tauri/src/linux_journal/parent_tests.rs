use super::*;
mod compat_fixture;
mod crash_controls;
mod publication_controls;
mod recovery_controls;
mod restart_controls;
use crate::linux_guard::{
    mutation::{AncestorValue, ParentPlan},
    Identity, Root,
};
use parents::{
    record::{decode as parent_decode, encode as parent_encode, ParentIntent, LIMIT},
    state::{ParentRevision, ParentState, RetainReason},
};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Fixture {
    _budget: crate::test_support::Shared,
    path: PathBuf,
    before: serde_json::Value,
}
fn snapshot(path: &Path) -> serde_json::Value {
    let stat = std::fs::metadata(path).unwrap();
    serde_json::json!({"device":stat.dev(),"inode":stat.ino(),"uid":stat.uid(),"gid":stat.gid(),"mode":stat.mode(),"bytes":std::fs::read(path).unwrap()})
}
impl Fixture {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.skillify/evidence/skein/14/a2/native/journal-parents")
            .join(format!(
                "fixture-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(
            path.join(".skein-parent-fixture"),
            b"skein-parent-fixture-v1\n",
        )
        .unwrap();
        for name in ["repo", "repo/.git", "data"] {
            std::fs::create_dir(path.join(name)).unwrap();
        }
        std::fs::write(path.join("repo/file"), b"before").unwrap();
        std::fs::write(path.join("outside"), b"outside-sentinel").unwrap();
        let before = serde_json::json!({"source":snapshot(&path.join("repo/file")),"sentinel":snapshot(&path.join("outside"))});
        std::fs::write(path.join("restore-backup"), b"before").unwrap();
        std::fs::write(path.join("repo/file"), b"restore-drill").unwrap();
        std::fs::write(
            path.join("repo/file"),
            std::fs::read(path.join("restore-backup")).unwrap(),
        )
        .unwrap();
        assert_eq!(snapshot(&path.join("repo/file")), before["source"]);
        std::fs::write(
            path.join("before.json"),
            serde_json::to_vec(&before).unwrap(),
        )
        .unwrap();
        std::fs::write(
            path.join("restore-proof.json"),
            b"{\"exactSourceRestore\":true,\"sentinelPreserved\":true}\n",
        )
        .unwrap();
        Self {
            _budget: crate::test_support::Shared::new(),
            path: path.canonicalize().unwrap(),
            before,
        }
    }
    fn root(&self) -> Root {
        Root::open(&self.path.join("repo"), &[]).unwrap()
    }
    fn journal(&self) -> Journal {
        Journal::open_guarded(&self.path.join("data"), &[self.root().value().unwrap()]).unwrap()
    }
    fn plan(&self, path: &str) -> ParentPlan {
        self.root().preview_parents(path).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let after = serde_json::json!({"source":snapshot(&self.path.join("repo/file")),"sentinel":snapshot(&self.path.join("outside"))});
        assert_eq!(after, self.before);
        std::fs::write(
            self.path.join("after.json"),
            serde_json::to_vec(&after).unwrap(),
        )
        .unwrap();
    }
}
fn intent(plan: ParentPlan) -> ParentIntent {
    ParentIntent {
        id: format!("p-{}", "a".repeat(32)),
        created_at: 1,
        directory_mode: 448,
        reserved_bytes: ParentIntent::reservation(plan.missing.len()).unwrap(),
        plan,
    }
}
#[test]
fn complete_parent_encoding_support_accepts_short_depth_and_refuses_long_escaped_prefixes() {
    let fixture = Fixture::new();
    let short = (0..63)
        .map(|_| "p")
        .chain(std::iter::once("file"))
        .collect::<Vec<_>>()
        .join("/");
    let accepted = intent(fixture.plan(&short));
    accepted.validate().unwrap();
    let bytes = parent_encode(&accepted).unwrap();
    assert!(bytes.len() <= LIMIT);
    assert_eq!(parent_decode::<ParentIntent>(&bytes).unwrap(), accepted);
    let long = (0..63)
        .map(|_| "a".repeat(62))
        .chain(std::iter::once("x".into()))
        .collect::<Vec<_>>()
        .join("/");
    assert_eq!(long.len(), 3970);
    let unsupported = intent(fixture.plan(&long));
    assert!(unsupported.validate().is_err());
    assert!(!fixture.path.join("repo").join("a".repeat(62)).exists());
    let escaped = (0..63)
        .map(|_| "\\\"".repeat(15))
        .chain(std::iter::once("file".into()))
        .collect::<Vec<_>>()
        .join("/");
    let unsupported = intent(fixture.plan(&escaped));
    assert!(unsupported.validate().is_err());
    assert!(!fixture.path.join("data/linux-recovery-v1").exists());
    std::fs::write(fixture.path.join("encoding-proof.json"),serde_json::to_vec(&serde_json::json!({"short63Supported":true,"shortIntentBytes":bytes.len(),"longDestinationBytes":long.len(),"longUnsupported":true,"escapedUnsupported":true,"targetAndStorageUnchanged":true})).unwrap()).unwrap();
}
#[test]
fn parent_admission_charges_full_reservation_before_any_target_creation_and_after_restart() {
    let fixture = Fixture::new();
    let plan = fixture.plan("missing/file");
    let mut journal = fixture.journal();
    let reservation = ParentIntent::reservation(1).unwrap();
    journal.quota = reservation - 1;
    assert!(journal.prepare_parent(&plan).is_err());
    assert_eq!(journal.directory.names(3).unwrap(), ["lock"]);
    assert!(!fixture.path.join("repo/missing").exists());
    journal.quota = STORAGE_LIMIT;
    journal.records = 0;
    assert!(journal.prepare_parent(&plan).is_err());
    assert_eq!(journal.directory.names(3).unwrap(), ["lock"]);
    journal.records = RECORD_LIMIT;
    let loaded = journal.prepare_parent(&plan).unwrap();
    assert_eq!(journal.accounted_bytes().unwrap(), reservation);
    assert!(!fixture.path.join("repo/missing").exists());
    let id = loaded.intent.id.clone();
    drop(journal);
    let journal = Journal::open_existing(&fixture.path.join("data"))
        .unwrap()
        .unwrap();
    assert_eq!(journal.accounted_bytes().unwrap(), reservation);
    assert!(journal.load_parent(&id).is_ok());
    std::fs::remove_file(
        fixture
            .path
            .join("data/linux-recovery-v1")
            .join(&id)
            .join("state-0001.json"),
    )
    .unwrap();
    assert!(journal.load_parent(&id).is_err());
    assert_eq!(journal.accounted_bytes().unwrap(), reservation);
    assert!(journal
        .reserve(STORAGE_LIMIT - reservation + 1, false)
        .is_err());
}
#[test]
fn parent_states_require_exact_contiguous_prefixes_and_strict_transitions() {
    let fixture = Fixture::new();
    let intent = intent(fixture.plan("a/b/file"));
    intent.validate().unwrap();
    let prepared = ParentState::Prepared {
        created: Vec::new(),
    };
    let creating = ParentState::Creating {
        created: Vec::new(),
        next: 0,
    };
    parents::state::transition(&prepared, &creating).unwrap();
    let first = AncestorValue {
        relative: "a".into(),
        identity: Identity::maximum_width(),
    };
    let created = ParentState::Created {
        created: vec![first.clone()],
    };
    created.validate(&intent).unwrap();
    parents::state::transition(&creating, &created).unwrap();
    assert!(parents::state::transition(&prepared, &created).is_err());
    let bad = ParentState::Creating {
        created: vec![first.clone()],
        next: 0,
    };
    assert!(bad.validate(&intent).is_err());
    let retained = ParentState::Retained {
        created: vec![first],
        file_record: None,
        reason: RetainReason::Cancelled,
    };
    parents::state::transition(&created, &retained).unwrap();
    assert!(parents::state::transition(&retained, &creating).is_err());
    let mut unknown = serde_json::to_value(&prepared).unwrap();
    unknown["extra"] = true.into();
    assert!(serde_json::from_value::<ParentState>(unknown).is_err());
    let revision = ParentRevision {
        sequence: intent.revisions() + 1,
        previous: Some("f".repeat(64)),
        intent: "f".repeat(64),
        state: retained,
    };
    assert!(revision.validate(&intent).is_err());
}

#[test]
fn unsupported_parent_encoding_refuses_record_allocation_through_actual_admission() {
    let fixture = Fixture::new();
    let journal = fixture.journal();
    let long = (0..63)
        .map(|_| "a".repeat(62))
        .chain(std::iter::once("x".into()))
        .collect::<Vec<_>>()
        .join("/");
    let plan = fixture.plan(&long);
    let error = match journal.prepare_parent(&plan) {
        Ok(_) => panic!("Unsupported parent encoding admitted"),
        Err(error) => error,
    };
    assert_eq!(error.error.message, "Parent record encoding is unsupported");
    assert!(error.parent_record.is_none());
    assert_eq!(journal.directory.names(3).unwrap(), ["lock"]);
    assert!(!fixture.path.join("repo").join("a".repeat(62)).exists());
}

#[test]
fn parent_revision_capacity_is_prepaid_and_terminal_append_needs_no_extra_reservation() {
    let fixture = Fixture::new();
    let mut journal = fixture.journal();
    let plan = fixture.plan("missing/file");
    let reservation = ParentIntent::reservation(1).unwrap();
    journal.quota = reservation;
    let mut loaded = journal.prepare_parent(&plan).unwrap();
    assert_eq!(journal.accounted_bytes().unwrap(), reservation);
    journal
        .append_parent(
            &mut loaded,
            ParentState::Retained {
                created: Vec::new(),
                file_record: None,
                reason: RetainReason::OperationStopped,
            },
        )
        .unwrap();
    assert_eq!(loaded.revision.sequence, 2);
    assert_eq!(journal.accounted_bytes().unwrap(), reservation);
    assert!(journal.prepare_parent(&fixture.plan("other/file")).is_err());
    assert!(!fixture.path.join("repo/missing").exists());
    assert!(!fixture.path.join("repo/other").exists());
}

#[test]
fn malformed_foreign_and_broken_parent_chains_remain_and_block_new_admission() {
    for mode in [
        "tornState",
        "gap",
        "badPrevious",
        "unknownStage",
        "wrongReservation",
        "unknownField",
        "futureVersion",
    ] {
        let fixture = Fixture::new();
        let journal = fixture.journal();
        let loaded = journal
            .prepare_parent(&fixture.plan("missing/file"))
            .unwrap();
        let directory = fixture
            .path
            .join("data/linux-recovery-v1")
            .join(&loaded.intent.id);
        let state = directory.join("state-0001.json");
        match mode {
            "tornState" => std::fs::write(&state, b"{").unwrap(),
            "gap" => std::fs::rename(&state, directory.join("state-0002.json")).unwrap(),
            "badPrevious" => {
                let mut revision = loaded.revision.clone();
                revision.previous = Some("f".repeat(64));
                std::fs::write(&state, parent_encode(&revision).unwrap()).unwrap();
            }
            "unknownStage" => {
                let mut payload = serde_json::to_value(&loaded.revision).unwrap();
                payload["state"]["stage"] = "unknown".into();
                std::fs::write(&state, record::encode(&payload).unwrap()).unwrap();
            }
            "wrongReservation" => {
                let mut intent = loaded.intent.clone();
                intent.reserved_bytes += 1;
                std::fs::write(
                    directory.join("intent.json"),
                    parent_encode(&intent).unwrap(),
                )
                .unwrap();
            }
            "unknownField" => {
                let mut payload = serde_json::to_value(&loaded.intent).unwrap();
                payload["extra"] = true.into();
                std::fs::write(
                    directory.join("intent.json"),
                    record::encode(&payload).unwrap(),
                )
                .unwrap();
            }
            "futureVersion" => {
                let mut envelope =
                    serde_json::from_slice::<serde_json::Value>(&loaded.intent_bytes).unwrap();
                envelope["version"] = 2.into();
                std::fs::write(
                    directory.join("intent.json"),
                    serde_json::to_vec(&envelope).unwrap(),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        assert!(journal.load_parent(&loaded.intent.id).is_err());
        if ["tornState", "gap", "badPrevious", "unknownStage"].contains(&mode) {
            assert_eq!(
                journal.accounted_bytes().unwrap(),
                loaded.intent.reserved_bytes
            );
        }
        let names = journal.directory.names(4).unwrap();
        assert!(journal.prepare_parent(&fixture.plan("next/file")).is_err());
        assert_eq!(journal.directory.names(4).unwrap(), names);
        assert!(directory.exists());
        assert!(!fixture.path.join("repo/missing").exists());
        assert!(!fixture.path.join("repo/next").exists());
        std::fs::write(fixture.path.join("classification-proof.json"),serde_json::to_vec(&serde_json::json!({"case":mode,"recordRetained":true,"newAdmissionBlocked":true,"targetUnchanged":true})).unwrap()).unwrap();
    }
}

#[test]
fn exact_encoded_boundary_and_maximum_metadata_refuse_without_parent_mutation() {
    let fixture = Fixture::new();
    std::fs::create_dir(fixture.path.join("metadata")).unwrap();
    let short = (0..63)
        .map(|_| "p")
        .chain(std::iter::once("file"))
        .collect::<Vec<_>>()
        .join("/");
    let candidate = |length: usize| {
        let path = fixture
            .path
            .join("metadata/missing")
            .join((0..length).map(|_| "\"").collect::<String>());
        let root = Root::open(&fixture.path.join("repo"), &[path]).unwrap();
        intent(root.preview_parents(&short).unwrap())
    };
    let mut low = 0usize;
    let mut high = 3000usize;
    assert!(parents::support::maximum_size(&candidate(low)).unwrap() <= LIMIT);
    assert!(parents::support::maximum_size(&candidate(high)).unwrap() > LIMIT);
    while low + 1 < high {
        let mid = (low + high) / 2;
        if parents::support::maximum_size(&candidate(mid)).unwrap() <= LIMIT {
            low = mid;
        } else {
            high = mid;
        }
    }
    let mut boundary = candidate(low);
    let base = parents::support::maximum_size(&boundary).unwrap();
    let extra = LIMIT - base;
    boundary.created_at = 10u128.pow(extra as u32);
    assert_eq!(parents::support::maximum_size(&boundary).unwrap(), LIMIT);
    boundary.validate().unwrap();
    boundary.created_at = boundary.created_at.checked_mul(10).unwrap();
    assert_eq!(
        parents::support::maximum_size(&boundary).unwrap(),
        LIMIT + 1
    );
    assert!(boundary.validate().is_err());
    assert!(!fixture.path.join("repo/p").exists());
    assert!(!fixture.path.join("data/linux-recovery-v1").exists());
    std::fs::write(fixture.path.join("boundary-proof.json"),serde_json::to_vec(&serde_json::json!({"maximumEnvelopeAtBoundary":LIMIT,"unsupportedEnvelope":LIMIT+1,"actualCapturedMetadata":true,"escapingMeasuredByTypedEncoder":true,"targetAndStorageUnchanged":true})).unwrap()).unwrap();
}

#[test]
fn maximum_parent_artifact_name_set_fits_the_native_directory_allowance() {
    let fixture = Fixture::new();
    let journal = fixture.journal();
    let destination = (0..63)
        .map(|_| "p")
        .chain(std::iter::once("file"))
        .collect::<Vec<_>>()
        .join("/");
    let loaded = journal.prepare_parent(&fixture.plan(&destination)).unwrap();
    for sequence in 2..=loaded.intent.revisions() {
        loaded
            .directory
            .write_new(&state::name(sequence), b"")
            .unwrap();
    }
    let directory = fixture
        .path
        .join("data/linux-recovery-v1")
        .join(&loaded.intent.id);
    let stat = std::fs::metadata(&directory).unwrap();
    let fs = rustix::fs::statfs(&directory).unwrap();
    let names = loaded.directory.names(200).unwrap();
    let expected = std::iter::once("intent.json".to_string())
        .chain((1..=loaded.intent.revisions()).map(state::name))
        .collect::<Vec<_>>();
    assert_eq!(names, expected);
    assert_eq!(fs.f_type, libc::EXT4_SUPER_MAGIC);
    let proof = serde_json::json!({"record":loaded.intent.id,"missingParents":63,"revisions":loaded.intent.revisions(),"artifactNames":names,"artifactCount":names.len(),"filesystem":"ext4","filesystemBlockBytes":fs.f_bsize,"directoryNativeBytes":stat.size(),"directoryBlocks512":stat.blocks(),"directoryAllocatedBytes":stat.blocks()*512,"directoryAllowanceBytes":4096,"reservedBytes":loaded.intent.reserved_bytes,"layoutOnlyEmptyExtraStates":true,"incompleteRecordRetained":true,"targetUnchanged":!fixture.path.join("repo/p").exists()});
    std::fs::write(
        fixture.path.join("directory-layout-proof.json"),
        serde_json::to_vec_pretty(&proof).unwrap(),
    )
    .unwrap();
    assert!(journal.load_parent(&loaded.intent.id).is_err());
    assert!(!fixture.path.join("repo/p").exists());
    assert!(
        stat.size() <= 4096 && stat.blocks() * 512 <= 4096,
        "Native parent directory allowance disproved: {}",
        proof
    );
}

#[test]
fn maximum_captured_metadata_support_refuses_before_parent_record_creation() {
    let fixture = Fixture::new();
    std::fs::create_dir(fixture.path.join("metadata")).unwrap();
    let metadata = (0..6)
        .map(|index| {
            let base = fixture.path.join(format!("metadata/missing{index}"));
            let remaining = 4095 - base.as_os_str().len() - 1;
            base.join("\"".repeat(remaining))
        })
        .collect::<Vec<_>>();
    let root = Root::open(&fixture.path.join("repo"), &metadata).unwrap();
    let value = root.value().unwrap();
    assert_eq!(value.protected.len(), 8);
    assert!(value
        .inputs
        .iter()
        .all(|path| path.as_os_str().len() == 4095));
    let journal = Journal::open_guarded(&fixture.path.join("data"), &[value]).unwrap();
    let plan = root.preview_parents("missing/file").unwrap();
    let measured = parents::support::maximum_size(&intent(plan.clone())).unwrap();
    assert!(measured > LIMIT);
    let error = match journal.prepare_parent(&plan) {
        Ok(_) => panic!("Unsupported maximum metadata admitted"),
        Err(error) => error,
    };
    assert_eq!(error.error.message, "Parent record encoding is unsupported");
    assert!(error.parent_record.is_none());
    assert_eq!(journal.directory.names(3).unwrap(), ["lock"]);
    assert!(!fixture.path.join("repo/missing").exists());
    std::fs::write(fixture.path.join("maximum-metadata-proof.json"), serde_json::to_vec(&serde_json::json!({"actualProtectedLocations":8,"externalInputCount":6,"eachExternalInputBytes":4095,"kernelPathLimitIncludesNul":true,"maximumEncodedBytes":measured,"unsupportedBeforeRecordCreation":true,"targetUnchanged":true})).unwrap()).unwrap();
}

#[test]
fn foreign_parent_path_and_paired_proof_are_both_accounted_and_retained() {
    let fixture = Fixture::new();
    let journal = fixture.journal();
    let id = format!("p-{}", "a".repeat(32));
    let proof_name = format!("parent-cleanup-{id}.json");
    journal.directory.write_new(&id, b"foreign-record").unwrap();
    journal
        .directory
        .write_new(&proof_name, b"foreign-proof")
        .unwrap();
    let record = journal
        .directory
        .file(&id, false)
        .unwrap()
        .proof(LIMIT)
        .unwrap();
    let proof = journal
        .directory
        .file(&proof_name, false)
        .unwrap()
        .proof(LIMIT)
        .unwrap();
    assert_eq!(
        journal.accounted_bytes().unwrap(),
        record.length + proof.length
    );
    assert!(journal
        .prepare_parent(&fixture.plan("missing/file"))
        .is_err());
    assert_eq!(
        journal
            .directory
            .file(&id, false)
            .unwrap()
            .proof(LIMIT)
            .unwrap(),
        record
    );
    assert_eq!(
        journal
            .directory
            .file(&proof_name, false)
            .unwrap()
            .proof(LIMIT)
            .unwrap(),
        proof
    );
    assert!(!fixture.path.join("repo/missing").exists());
}

#[test]
fn file_and_parent_records_share_the_exact_count_and_storage_limits() {
    let fixture = Fixture::new();
    let mut journal = fixture.journal();
    assert_eq!(STORAGE_LIMIT, 512 * 1024 * 1024);
    assert_eq!(RECORD_LIMIT, 1024);
    journal.records = 2;
    let root = fixture.root();
    let file = journal
        .replace(
            &root,
            "ordinary",
            &crate::linux_guard::mutation::Snapshot::Missing,
            b"ordinary",
        )
        .unwrap();
    let file_bytes = journal.accounted_bytes().unwrap();
    let mut parent = journal
        .prepare_parent(&fixture.plan("missing/file"))
        .unwrap();
    journal
        .append_parent(
            &mut parent,
            ParentState::Retained {
                created: Vec::new(),
                file_record: None,
                reason: RetainReason::Cancelled,
            },
        )
        .unwrap();
    let combined = journal.accounted_bytes().unwrap();
    assert_eq!(journal.parent_usage().unwrap().records, 1);
    assert!(combined >= file_bytes + parent.intent.reserved_bytes);
    assert!(journal
        .prepare_parent(&fixture.plan("another/file"))
        .is_err());
    assert!(journal
        .replace(
            &root,
            "other",
            &crate::linux_guard::mutation::Snapshot::Missing,
            b"other"
        )
        .is_err());
    assert_eq!(journal.export(&file.record, false).unwrap(), b"ordinary");
    assert_eq!(journal.accounted_bytes().unwrap(), combined);
    assert!(!fixture.path.join("repo/missing").exists());
    assert!(!fixture.path.join("repo/another").exists());
    assert!(!fixture.path.join("repo/other").exists());
}

#[test]
#[ignore]
fn frozen13_fixture_producer() {
    compat_fixture::produce();
}
