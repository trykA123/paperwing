use super::recovery_controls::{artifacts, Authority};
use super::*;
use parents::{ParentRecoveryRecord, ParentRecoveryStage};
use std::os::unix::process::ExitStatusExt;
use std::{
    io::{BufRead, Write},
    process::{Child, Command, Stdio},
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant},
};

pub(super) struct OwnedChild {
    child: Child,
    output: mpsc::Receiver<String>,
    reader: Option<std::thread::JoinHandle<()>>,
}
impl OwnedChild {
    pub(super) fn spawn(path: &Path, phase: &str, occurrence: usize, cleanup: bool) -> Self {
        let log = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path.join("child.log"))
            .unwrap();
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "linux_journal::parent_tests::crash_controls::parent_process_helper",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("PAPERWING_PARENT_CRASH_FIXTURE", path)
            .env("PAPERWING_PARENT_CRASH_PHASE", phase)
            .env("PAPERWING_PARENT_CRASH_OCCURRENCE", occurrence.to_string())
            .env(
                "PAPERWING_PARENT_CRASH_CLEANUP",
                if cleanup { "yes" } else { "no" },
            )
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap();
        let (send, output) = mpsc::channel();
        let mut owned = Self {
            child,
            output,
            reader: None,
        };
        let stdout = owned.child.stdout.take().unwrap();
        let log = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path.join("child-stdout.log"))
            .unwrap();
        owned.reader = Some(std::thread::spawn(move || {
            let mut log = log;
            for line in std::io::BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                writeln!(log, "{line}").unwrap();
                if send.send(line).is_err() {
                    break;
                }
            }
        }));
        owned
    }
    pub(super) fn kill_after_ready(&mut self, phase: &str) -> u32 {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .expect("Owned child readiness timed out");
            let line = self
                .output
                .recv_timeout(remaining)
                .expect("Owned child readiness failed");
            if line.ends_with(&format!("PARENT_READY {phase}")) {
                break;
            }
        }
        let pid = self.child.id();
        self.child.kill().unwrap();
        let status = self.child.wait().unwrap();
        assert_eq!(status.signal(), Some(libc::SIGKILL));
        self.reader.take().unwrap().join().unwrap();
        assert!(self.child.try_wait().unwrap().is_some());
        pid
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
fn reach(
    path: &Path,
    wanted: &str,
    occurrence: usize,
    counts: &mut std::collections::BTreeMap<String, usize>,
    phase: &str,
    id: &str,
) {
    let count = counts.entry(phase.into()).or_default();
    *count += 1;
    if wanted != phase || *count != occurrence {
        return;
    }
    let proof = serde_json::json!({"pid":std::process::id(),"phase":phase,"occurrence":occurrence,"record":id,"fixture":path});
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path.join("checkpoint.json"))
        .unwrap();
    file.write_all(&serde_json::to_vec(&proof).unwrap())
        .unwrap();
    file.sync_all().unwrap();
    std::fs::File::open(path).unwrap().sync_all().unwrap();
    println!("PARENT_READY {phase}");
    std::io::stdout().flush().unwrap();
    loop {
        std::thread::park();
    }
}
#[test]
#[ignore]
fn parent_process_helper() {
    let path = PathBuf::from(std::env::var("PAPERWING_PARENT_CRASH_FIXTURE").unwrap());
    assert!(
        path.is_absolute()
            && path.starts_with(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../.skillify/evidence/paperwing/14/a2/native")
                    .canonicalize()
                    .unwrap()
            )
    );
    assert_eq!(
        std::fs::read(path.join(".paperwing-parent-fixture")).unwrap(),
        b"paperwing-parent-fixture-v1\n"
    );
    assert!(path.join("restore-proof.json").is_file());
    let wanted = std::env::var("PAPERWING_PARENT_CRASH_PHASE").unwrap();
    let occurrence = std::env::var("PAPERWING_PARENT_CRASH_OCCURRENCE")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let cleanup = match std::env::var("PAPERWING_PARENT_CRASH_CLEANUP")
        .unwrap()
        .as_str()
    {
        "yes" => true,
        "no" => false,
        _ => panic!("Unknown owned child mode"),
    };
    let counts = Arc::new(Mutex::new(std::collections::BTreeMap::new()));
    let current = Arc::new(Mutex::new(String::new()));
    let hook_path = path.clone();
    let hook_wanted = wanted.clone();
    let hook_counts = counts.clone();
    let hook_current = current.clone();
    parents::PARENT_JOURNAL_HOOK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |phase, id| {
            if parents::record::id_valid(id) {
                *hook_current.lock().unwrap() = id.into();
            }
            reach(
                &hook_path,
                &hook_wanted,
                occurrence,
                &mut hook_counts.lock().unwrap(),
                phase,
                id,
            );
            Ok(())
        }))
    });
    let hook_path = path.clone();
    let hook_wanted = wanted.clone();
    crate::linux_guard::mutation::set_parent_hook(Some(Box::new(move |phase| {
        let id = current.lock().unwrap().clone();
        reach(
            &hook_path,
            &hook_wanted,
            occurrence,
            &mut counts.lock().unwrap(),
            &format!("native{phase}"),
            &id,
        );
        Ok(())
    })));
    let root = Root::open(&path.join("repo"), &[]).unwrap();
    let journal = Journal::open_guarded(&path.join("data"), &[root.value().unwrap()]).unwrap();
    let publication = journal
        .replace_with_parents(
            &root.preview_parents("first/next/target").unwrap(),
            &crate::linux_guard::mutation::Snapshot::Missing,
            b"after",
            &mut Authority,
        )
        .unwrap();
    if cleanup {
        journal
            .cleanup_parent(publication.parent_record.as_ref().unwrap(), true)
            .unwrap();
    }
    panic!("Owned child did not reach requested boundary");
}
fn target_snapshot(path: &Path) -> serde_json::Value {
    let mut values = serde_json::Map::new();
    for name in ["first", "first/next", "first/next/target"] {
        let path = path.join("repo").join(name);
        if path.exists() {
            let stat = std::fs::metadata(&path).unwrap();
            values.insert(name.into(), serde_json::json!({"device":stat.dev(),"inode":stat.ino(),"uid":stat.uid(),"gid":stat.gid(),"mode":stat.mode(),"bytes":if stat.is_file() { Some(std::fs::read(&path).unwrap()) } else { None }}));
        }
    }
    if path.join("repo/first").exists() {
        values.insert(
            "completeFirstSubtree".into(),
            artifacts(&path.join("repo/first")),
        );
    }
    values.into()
}
fn expected_stage(phase: &str) -> Option<ParentRecoveryStage> {
    match phase {
        "parentRecord"
        | "parentIntent"
        | "parentCreatedFileCreated"
        | "parentLinkedFileCreated"
        | "parentCleanupProofCreated" => Some(ParentRecoveryStage::Incomplete),
        "parentCleanupProofUnlinked" | "parentCleanupComplete" => None,
        phase if phase.starts_with("parentCleanup") => Some(ParentRecoveryStage::CleanupPending),
        phase if phase.starts_with("native") => Some(ParentRecoveryStage::Conflict),
        "fileapplied"
        | "parentLinked"
        | "parentLinkedFileWritten"
        | "parentLinkedFileVerified"
        | "parentLinkedFileSynced"
        | "parentLinkedRecordSynced"
        | "parentLinkedNamespaceSynced" => Some(ParentRecoveryStage::Linked),
        phase if phase.starts_with("file") => Some(ParentRecoveryStage::Conflict),
        _ => Some(ParentRecoveryStage::Retained),
    }
}
fn run(phase: &str, occurrence: usize, cleanup: bool) {
    let fixture = Fixture::new();
    let mut child = OwnedChild::spawn(&fixture.path, phase, occurrence, cleanup);
    let pid = child.kill_after_ready(phase);
    let checkpoint: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.path.join("checkpoint.json")).unwrap())
            .unwrap();
    assert_eq!(checkpoint["pid"], pid);
    assert_eq!(checkpoint["phase"], phase);
    let before = target_snapshot(&fixture.path);
    let journal = Journal::open_existing(&fixture.path.join("data"))
        .unwrap()
        .unwrap();
    let rows_before = journal.list_parents().unwrap();
    let charged_before = journal.accounted_bytes().unwrap();
    journal.reconcile().unwrap();
    let rows: Vec<ParentRecoveryRecord> = journal.list_parents().unwrap();
    assert_eq!(target_snapshot(&fixture.path), before);
    let expected = expected_stage(phase);
    if let Some(expected) = expected {
        assert_eq!(rows.len(), 1, "{phase}");
        assert_eq!(rows[0].stage, expected, "{phase}");
        if phase.starts_with("native") {
            assert_eq!(rows[0].uncertain, Some(occurrence as u32 - 1));
            assert_eq!(rows[0].created, occurrence as u32 - 1);
        }
    } else {
        assert!(rows.is_empty(), "{phase}");
    }
    let proof_name = journal
        .directory
        .names(2050)
        .unwrap()
        .into_iter()
        .find(|name| name.starts_with("parent-cleanup-"));
    if cleanup && phase != "parentCleanupComplete" && phase != "parentCleanupProofUnlinked" {
        assert!(proof_name.is_some());
        assert_eq!(journal.parent_usage().unwrap().records, 1);
        let reservation = ParentIntent::reservation(2).unwrap();
        assert_eq!(journal.parent_usage().unwrap().bytes, reservation);
    }
    let namespace_after_restart = artifacts(journal.directory.path());
    std::fs::write(fixture.path.join("restart-proof.json"), serde_json::to_vec_pretty(&serde_json::json!({"phase":phase,"occurrence":occurrence,"pid":pid,"signal":libc::SIGKILL,"reaped":true,"targetBeforeRestart":before,"targetAfterRestart":target_snapshot(&fixture.path),"chargedBeforeRestart":charged_before,"chargedAfterRestart":journal.accounted_bytes().unwrap(),"parentRowsBefore":rows_before,"parentRowsAfter":rows,"namespaceAfterRestart":namespace_after_restart,"sourceAndSentinelPreserved":true,"noResumeOrDirectoryRollback":true})).unwrap()).unwrap();
    if cleanup && phase != "parentCleanupProofCreated" {
        if let Some(name) = proof_name {
            let id = name
                .strip_prefix("parent-cleanup-")
                .unwrap()
                .strip_suffix(".json")
                .unwrap();
            assert!(journal.cleanup_parent(id, true).unwrap().complete);
            assert_eq!(target_snapshot(&fixture.path), before);
        }
    }
}
#[test]
fn sigkill_at_parent_mkdir_link_publication_and_cleanup_boundaries_retains_native_authority() {
    let mut cases = vec![
        ("parentRecord", 1, false),
        ("parentIntent", 1, false),
        ("parentPrepared", 1, false),
        ("parentReady", 1, false),
        ("parentLinking", 1, false),
        ("parentLinked", 1, false),
    ];
    for phase in [
        "parentCreating",
        "parentBeforeMkdir",
        "nativemkdir",
        "nativereopened",
        "nativeparentSync",
        "nativeparentSynced",
        "parentCreatedFileCreated",
        "parentCreatedFileWritten",
        "parentCreatedFileVerified",
        "parentCreatedFileSynced",
        "parentCreatedRecordSynced",
        "parentCreatedNamespaceSynced",
        "parentCreated",
    ] {
        for occurrence in 1..=2 {
            cases.push((phase, occurrence, false));
        }
    }
    for phase in [
        "filefirstBackup",
        "filebackups",
        "fileintent",
        "fileprepared",
        "filecandidate",
        "filestaged",
        "filereplacing",
        "filerenamed",
        "filedirectorySynced",
        "fileapplied",
        "parentLinkedFileCreated",
        "parentLinkedFileWritten",
        "parentLinkedFileVerified",
        "parentLinkedFileSynced",
        "parentLinkedRecordSynced",
        "parentLinkedNamespaceSynced",
    ] {
        cases.push((phase, 1, false));
    }
    for phase in [
        "parentCleanupProofCreated",
        "parentCleanupProofWritten",
        "parentCleanupProofVerified",
        "parentCleanupProofSynced",
        "parentCleanupPrepared",
        "parentCleanupEmpty",
        "parentCleanupDirectoryUnlinked",
        "parentCleanupDirectory",
        "parentCleanupProofUnlinked",
        "parentCleanupComplete",
    ] {
        cases.push((phase, 1, true));
    }
    for phase in ["parentCleanupArtifactUnlinked", "parentCleanupArtifact"] {
        for occurrence in 1..=9 {
            cases.push((phase, occurrence, true));
        }
    }
    for (phase, occurrence, cleanup) in cases {
        run(phase, occurrence, cleanup);
    }
}
