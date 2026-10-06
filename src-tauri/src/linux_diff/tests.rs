use super::*;
use crate::linux_guard::Root;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::sync::atomic::AtomicU64;

static NEXT: AtomicU64 = AtomicU64::new(1);
struct Fixture(PathBuf, #[allow(dead_code)] crate::test_support::Shared);
impl Fixture {
    fn new() -> Self {
        let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.skillify/evidence/paperwing/14/resume/native");
        std::fs::create_dir_all(&parent).unwrap();
        let path = parent.join(format!(
            "fixture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(
            path.join(".paperwing-diff-fixture"),
            b"paperwing-diff-fixture-v1\n",
        )
        .unwrap();
        for name in [
            "left",
            "right",
            "left/.git",
            "right/.git",
            "metadata",
            "data",
        ] {
            std::fs::create_dir(path.join(name)).unwrap();
        }
        std::fs::write(path.join("left/file"), b"source-left").unwrap();
        std::fs::write(path.join("right/file"), b"source-right").unwrap();
        std::fs::write(path.join("outside"), b"outside-sentinel").unwrap();
        let fixture = Self(path.canonicalize().unwrap(), crate::test_support::Shared::new());
        let before = fixture.snapshot();
        let bytes = std::fs::read(fixture.0.join("left/file")).unwrap();
        std::fs::write(fixture.0.join("restore-backup"), &bytes).unwrap();
        std::fs::write(fixture.0.join("left/file"), b"restore-drill").unwrap();
        std::fs::write(fixture.0.join("left/file"), &bytes).unwrap();
        assert_eq!(fixture.snapshot(), before);
        std::fs::write(
            fixture.0.join("restore-proof.json"),
            serde_json::to_vec(&before).unwrap(),
        )
        .unwrap();
        fixture
    }
    fn roots(&self) -> Vec<RootValue> {
        ["left", "right"]
            .into_iter()
            .map(|name| {
                Root::open(&self.0.join(name), &[self.0.join("metadata")])
                    .unwrap()
                    .value()
                    .unwrap()
            })
            .collect()
    }
    fn storage(&self) -> Storage {
        Storage::new(self.0.join("data")).unwrap()
    }
    fn snapshot(&self) -> serde_json::Value {
        let mut entries = Vec::new();
        fn walk(path: &Path, root: &Path, entries: &mut Vec<serde_json::Value>) {
            let metadata = std::fs::symlink_metadata(path).unwrap();
            entries.push(serde_json::json!({"path":path.strip_prefix(root).unwrap(),"inode":metadata.ino(),"device":metadata.dev(),"mode":metadata.mode(),"uid":metadata.uid(),"gid":metadata.gid(),"bytes":if metadata.is_file(){Some(std::fs::read(path).unwrap())}else{None}}));
            if metadata.is_dir() {
                let mut paths: Vec<_> = std::fs::read_dir(path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .collect();
                paths.sort();
                for child in paths {
                    walk(&child, root, entries);
                }
            }
        }
        for name in ["left", "right", "metadata", "outside"] {
            walk(&self.0.join(name), &self.0, &mut entries);
        }
        serde_json::json!(entries)
    }
    fn unchanged(&self, before: &serde_json::Value) {
        assert_eq!(&self.snapshot(), before);
    }
}

#[test]
fn configuration_does_not_create_storage_and_missing_suffix_is_private() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let parent = fixture.0.join("missing/app/data");
    let storage = Storage::new(parent.clone()).unwrap();
    assert!(!parent.exists());
    let directory = storage.initialize(&fixture.roots()).unwrap();
    assert_eq!(directory.path(), parent.join("linux-diff-v1"));
    for path in [
        fixture.0.join("missing"),
        fixture.0.join("missing/app"),
        parent,
        directory.path().to_path_buf(),
    ] {
        assert_eq!(std::fs::metadata(path).unwrap().mode() & 0o777, 0o700);
    }
    assert_eq!(
        std::fs::metadata(fixture.0.join("data")).unwrap().mode() & 0o777,
        0o755
    );
    fixture.unchanged(&before);
}

#[test]
fn missing_or_stale_authority_and_direct_source_or_metadata_overlap_refuse_before_creation() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    for name in ["left/new/data", "right/new/data", "metadata/new/data"] {
        let parent = fixture.0.join(name);
        assert!(Storage::new(parent.clone())
            .unwrap()
            .initialize(&fixture.roots())
            .is_err());
        assert!(!parent.exists());
        fixture.unchanged(&before);
    }
    let absent = fixture.0.join("absent-meta");
    let roots = vec![
        Root::open(&fixture.0.join("left"), std::slice::from_ref(&absent))
            .unwrap()
            .value()
            .unwrap(),
    ];
    assert!(Storage::new(absent.join("data"))
        .unwrap()
        .initialize(&roots)
        .is_err());
    assert!(!absent.exists());
    assert!(fixture.storage().initialize(&[]).is_err());
    assert!(!fixture.0.join("data/linux-diff-v1").exists());
    let roots = fixture.roots();
    std::fs::rename(fixture.0.join("left"), fixture.0.join("saved-left")).unwrap();
    std::fs::create_dir(fixture.0.join("left")).unwrap();
    assert!(fixture.storage().initialize(&roots).is_err());
    assert!(!fixture.0.join("data/linux-diff-v1").exists());
    std::fs::remove_dir(fixture.0.join("left")).unwrap();
    std::fs::rename(fixture.0.join("saved-left"), fixture.0.join("left")).unwrap();
    fixture.unchanged(&before);
}

#[test]
fn linked_and_substituted_initializer_parents_refuse_without_outside_writes() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    std::os::unix::fs::symlink(fixture.0.join("metadata"), fixture.0.join("linked")).unwrap();
    assert!(Storage::new(fixture.0.join("linked/new/data"))
        .unwrap()
        .initialize(&fixture.roots())
        .is_err());
    let parent = fixture.0.join("data");
    let saved = fixture.0.join("saved-data");
    let outside = fixture.0.join("metadata");
    storage::INITIALIZE_HOOK.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move |_| {
            if !saved.exists() {
                std::fs::rename(&parent, &saved).unwrap();
                std::os::unix::fs::symlink(&outside, &parent).unwrap();
            }
        }))
    });
    assert!(Storage::new(fixture.0.join("data/new/data"))
        .unwrap()
        .initialize(&fixture.roots())
        .is_err());
    storage::INITIALIZE_HOOK.with(|hook| *hook.borrow_mut() = None);
    assert!(!fixture.0.join("metadata/new").exists());
    std::fs::remove_file(fixture.0.join("data")).unwrap();
    std::fs::rename(fixture.0.join("saved-data"), fixture.0.join("data")).unwrap();
    fixture.unchanged(&before);
}

#[test]
fn whole_and_nested_bind_aliases_refuse_storage_before_creation() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    std::fs::create_dir(fixture.0.join("left/nested")).unwrap();
    std::fs::create_dir(fixture.0.join("alias")).unwrap();
    let namespace = std::fs::read_link("/proc/self/ns/mnt").unwrap();
    let script = r#"import os,pathlib,subprocess,sys
root=pathlib.Path(sys.argv[1]);exe=sys.argv[2];original=sys.argv[3]
assert root.is_absolute() and root.resolve()==root
assert (root/'.paperwing-diff-fixture').read_bytes()==b'paperwing-diff-fixture-v1\n'
assert os.readlink('/proc/self/ns/mnt')!=original
for source in ['left','left/nested']:
 subprocess.run(['mount','--bind',str(root/source),str(root/'alias')],check=True,timeout=10)
 env=os.environ.copy();env['PAPERWING_DIFF_FIXTURE']=str(root);env['PAPERWING_DIFF_MODE']='bind'
 subprocess.run([exe,'--exact','linux_diff::tests::native_child','--ignored','--nocapture','--test-threads=1'],env=env,check=True,timeout=20)
 subprocess.run(['umount',str(root/'alias')],check=True,timeout=10)
"#;
    let output = std::process::Command::new("unshare")
        .args(["-Urnm", "python3", "-c", script])
        .arg(&fixture.0)
        .arg(std::env::current_exe().unwrap())
        .arg(&namespace)
        .output()
        .unwrap();
    std::fs::write(fixture.0.join("bind-stdout.log"), &output.stdout).unwrap();
    std::fs::write(fixture.0.join("bind-stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_link("/proc/self/ns/mnt").unwrap(), namespace);
    std::fs::remove_dir(fixture.0.join("left/nested")).unwrap();
    fixture.unchanged(&before);
}

#[test]
#[ignore]
fn native_child() {
    let path = PathBuf::from(std::env::var("PAPERWING_DIFF_FIXTURE").unwrap());
    assert!(path.is_absolute());
    assert_eq!(path.canonicalize().unwrap(), path);
    assert_eq!(
        std::fs::read(path.join(".paperwing-diff-fixture")).unwrap(),
        b"paperwing-diff-fixture-v1\n"
    );
    let fixture = Fixture(path, crate::test_support::Shared::new());
    let before = fixture.snapshot();
    let mode = std::env::var("PAPERWING_DIFF_MODE").unwrap();
    if mode == "bind" {
        assert!(Storage::new(fixture.0.join("alias/new/data"))
            .unwrap()
            .initialize(&fixture.roots())
            .is_err());
        assert!(!fixture.0.join("alias/new").exists());
        fixture.unchanged(&before);
    } else if mode == "reserve" || mode == "bootstrap" || mode == "holder" {
        let id = std::env::var("PAPERWING_DIFF_CHILD_ID").unwrap();
        if mode == "reserve" {
            let base = fixture.0.clone();
            let id = id.clone();
            storage::DIFF_STORAGE_HOOK.with(|hook| {
                *hook.borrow_mut() = Some(Box::new(move |phase| {
                    if phase == "lock-busy" {
                        std::fs::write(base.join(format!("child-{id}-lock-busy")), b"waiting")
                            .unwrap();
                    }
                    Ok(())
                }))
            });
        }
        if mode == "bootstrap" {
            let base = fixture.0.clone();
            let id = id.clone();
            storage::DIFF_STORAGE_HOOK.with(|hook| {
                *hook.borrow_mut() = Some(Box::new(move |phase| {
                    if phase == "missing-lock" {
                        checkpoint(&base, &id, phase);
                    }
                    Ok(())
                }))
            });
        }
        let storage = fixture.storage();
        if mode == "holder" {
            let directory = storage.initialize(&fixture.roots()).unwrap();
            let _lock = admission::acquire(&directory, &storage.policy, None).unwrap();
            checkpoint(&fixture.0, &id, "held");
        } else {
            let reservation = storage
                .reserve(
                    &fixture.roots(),
                    &AtomicBool::new(false),
                    &[b"child-left".to_vec(), b"child-right".to_vec()],
                )
                .unwrap();
            std::fs::write(
                fixture.0.join(format!("child-{id}-result.json")),
                serde_json::to_vec(&reservation.claim).unwrap(),
            )
            .unwrap();
        }
        fixture.unchanged(&before);
    } else if mode == "manifest" {
        let id = std::env::var("PAPERWING_DIFF_CHILD_ID").unwrap();
        let mut storage = fixture.storage();
        let base = fixture.0.clone();
        storage.hook = Some(Arc::new(move |phase| {
            if phase == "manifest-created" {
                checkpoint(&base, &id, phase);
            }
            Ok(())
        }));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let lease = Arc::new(storage)
                .materialize(
                    fixture.roots(),
                    Arc::new(AtomicBool::new(false)),
                    [b"left\n", b"right\n"],
                )
                .await
                .unwrap();
            lease.finish().await.unwrap();
        });
        fixture.unchanged(&before);
    } else if mode == "counts-bootstrap" || mode == "counts-waiter" {
        let id = std::env::var("PAPERWING_DIFF_CHILD_ID").unwrap();
        let mut storage = fixture.storage();
        let base = fixture.0.clone();
        let child_id = id.clone();
        let bootstrap = mode == "counts-bootstrap";
        storage.hook = Some(Arc::new(move |phase| {
            if bootstrap && phase == "missing-lock" {
                checkpoint(&base, &child_id, phase);
            }
            if phase == "lock-busy" {
                std::fs::write(base.join(format!("child-{child_id}-lock-busy")), b"waiting")
                    .unwrap();
            }
            Ok(())
        }));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = runtime
            .block_on(crate::compare::native_diff_counts(
                crate::compare::NativeDiffTest {
                    storage: Arc::new(storage),
                    roots: fixture.roots(),
                    cancel: Arc::new(AtomicBool::new(false)),
                },
            ))
            .unwrap();
        assert_eq!(result["lines"], serde_json::json!({"added":2,"removed":1}));
        assert_eq!(result["commands"], serde_json::json!({"diff":1}));
        std::fs::write(
            fixture.0.join(format!("child-{id}-result.json")),
            serde_json::to_vec(&result).unwrap(),
        )
        .unwrap();
        fixture.unchanged(&before);
    } else if mode == "foreign-owner" {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let storage = Arc::new(fixture.storage());
        runtime.block_on(async {
            let lease=storage.materialize(fixture.roots(), Arc::new(AtomicBool::new(false)), [b"l", b"r"]).await.unwrap();let path=lease.path().join("left");let file=std::fs::File::open(&path).unwrap();
            rustix::fs::fchown(&file,Some(rustix::fs::Uid::from_raw(1)),Some(rustix::fs::Gid::from_raw(1))).unwrap();
            assert!(lease.finish().await.is_err());assert!(storage.reserve(&fixture.roots(),&AtomicBool::new(false),&[vec![],vec![]]).is_err());
            rustix::fs::fchown(&file,Some(rustix::fs::Uid::from_raw(0)),Some(rustix::fs::Gid::from_raw(0))).unwrap();assert_eq!(std::fs::read(&path).unwrap(),b"l");assert_eq!(usage(&fixture,&storage).allocations,1);
            std::fs::write(fixture.0.join("foreign-owner-proof.json"),b"{\"cleanupRefused\":true,\"admissionRefused\":true,\"fullClaimRetained\":true,\"fixtureOwnershipRestored\":true}\n").unwrap();
        });
        fixture.unchanged(&before);
    } else if mode == "reservation-pause" || mode == "cleanup-pause" {
        let id = std::env::var("PAPERWING_DIFF_CHILD_ID").unwrap();
        let base = fixture.0.clone();
        let target = if mode == "reservation-pause" {
            "before-directory"
        } else {
            "before-claim-removal"
        };
        let mut storage = fixture.storage();
        storage.hook = Some(Arc::new(move |phase| {
            if phase == target {
                checkpoint(&base, &id, phase);
            }
            Ok(())
        }));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let lease = Arc::new(storage)
                .materialize(
                    fixture.roots(),
                    Arc::new(AtomicBool::new(false)),
                    [b"l", b"r"],
                )
                .await
                .unwrap();
            lease.finish().await.unwrap();
        });
        fixture.unchanged(&before);
    } else {
        panic!("Unknown diff child mode");
    }
}

fn usage(fixture: &Fixture, storage: &Storage) -> admission::Inventory {
    let directory = storage.initialize(&fixture.roots()).unwrap();
    let lock = admission::acquire(&directory, &storage.policy, None).unwrap();
    admission::inventory(&directory, &lock, &storage.policy).unwrap()
}
#[test]
fn reservations_stay_fully_charged_across_restart_and_quota_refuses_before_creation() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let cancel = AtomicBool::new(false);
    let mut storage = fixture.storage();
    let empty = usage(&fixture, &storage).bytes;
    storage.policy.bytes = empty + record::OVERHEAD + 2;
    storage.policy.allocations = 1;
    let reservation = storage
        .reserve(&fixture.roots(), &cancel, &[b"a".to_vec(), b"b".to_vec()])
        .unwrap();
    assert_eq!(
        usage(&fixture, &storage).bytes,
        empty + record::OVERHEAD + 2
    );
    assert_eq!(usage(&fixture, &storage).allocations, 1);
    let names = reservation.namespace.names(4).unwrap();
    assert!(storage
        .reserve(&fixture.roots(), &cancel, &[vec![], vec![]])
        .is_err());
    assert_eq!(reservation.namespace.names(4).unwrap(), names);
    let restarted = fixture.storage();
    assert_eq!(
        usage(&fixture, &restarted).bytes,
        empty + record::OVERHEAD + 2
    );
    let directory = reservation
        .namespace
        .create_child(&reservation.claim.directory_name)
        .unwrap();
    directory.write_new("left", b"a").unwrap();
    assert_eq!(
        usage(&fixture, &restarted).bytes,
        empty + record::OVERHEAD + 2
    );
    fixture.unchanged(&before);
}
#[test]
fn unsafe_unknown_malformed_linked_nested_and_foreign_artifacts_block_admission() {
    for kind in [
        "unknown",
        "malformed",
        "link",
        "nested",
        "mode",
        "hardlink",
        "attribute",
    ] {
        let fixture = Fixture::new();
        let before = fixture.snapshot();
        let storage = fixture.storage();
        let reservation = storage
            .reserve(
                &fixture.roots(),
                &AtomicBool::new(false),
                &[b"ab".to_vec(), b"cd".to_vec()],
            )
            .unwrap();
        let namespace = reservation.namespace.path().to_path_buf();
        let directory = reservation
            .namespace
            .create_child(&reservation.claim.directory_name)
            .unwrap();
        match kind {
            "unknown" => std::fs::write(namespace.join("foreign"), b"unknown").unwrap(),
            "malformed" => std::fs::write(
                namespace.join(format!(
                    "reservation-{}.json",
                    reservation.claim.directory_name
                )),
                b"{}",
            )
            .unwrap(),
            "link" => {
                std::os::unix::fs::symlink(fixture.0.join("outside"), directory.path().join("left"))
                    .unwrap()
            }
            "nested" => {
                std::fs::create_dir(directory.path().join("left")).unwrap();
                std::fs::set_permissions(
                    directory.path().join("left"),
                    std::fs::Permissions::from_mode(0o700),
                )
                .unwrap();
            }
            "mode" => {
                directory.write_new("left", b"ab").unwrap();
                std::fs::set_permissions(
                    directory.path().join("left"),
                    std::fs::Permissions::from_mode(0o644),
                )
                .unwrap();
            }
            "hardlink" => {
                directory.write_new("left", b"ab").unwrap();
                std::fs::hard_link(directory.path().join("left"), fixture.0.join("extra-link"))
                    .unwrap();
            }
            "attribute" => {
                let file = directory.file("left", true).unwrap();
                file.write(b"ab").unwrap();
                let file = std::fs::File::open(directory.path().join("left")).unwrap();
                rustix::fs::fsetxattr(file, "user.foreign", b"x", rustix::fs::XattrFlags::empty())
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let mut names = std::fs::read_dir(&namespace)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        names.sort();
        assert!(
            storage
                .reserve(&fixture.roots(), &AtomicBool::new(false), &[vec![], vec![]])
                .is_err(),
            "{kind}"
        );
        let mut after = std::fs::read_dir(&namespace)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        after.sort();
        assert_eq!(names, after, "{kind}");
        fixture.unchanged(&before);
    }
}
#[test]
fn delayed_lock_acquisition_refuses_allocation_and_releases_the_late_flock() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let mut storage = fixture.storage();
    storage.policy.deadline = std::time::Duration::from_millis(100);
    let (hold, released) = std::sync::mpsc::channel::<()>();
    let released = std::sync::Mutex::new(released);
    storage.hook = Some(Arc::new(move |phase| {
        if phase == "missing-lock" {
            assert_eq!(
                released
                    .lock()
                    .unwrap()
                    .recv_timeout(std::time::Duration::from_millis(150)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            );
        }
        Ok(())
    }));
    let roots = fixture.roots();
    let result = {
        let _hook = storage.install_hook();
        storage.reserve(
            &roots,
            &AtomicBool::new(false),
            &[b"left".to_vec(), b"right".to_vec()],
        )
    };
    drop(hold);
    assert!(matches!(result, Err(error) if error.message.contains("expired")));
    let directory = storage.initialize(&roots).unwrap();
    assert_eq!(directory.names(3).unwrap(), vec!["lock"]);
    let lock = admission::acquire(&directory, &admission::Policy::default(), None).unwrap();
    drop(lock);
    fixture.unchanged(&before);
}

#[test]
fn bounded_lock_wait_observes_cancellation_timeout_and_replaced_or_unsafe_identity() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let mut storage = fixture.storage();
    storage.policy.deadline = std::time::Duration::from_millis(100);
    storage.policy.retry = std::time::Duration::from_millis(5);
    let directory = storage.initialize(&fixture.roots()).unwrap();
    let lock = admission::acquire(&directory, &storage.policy, None).unwrap();
    let started = std::time::Instant::now();
    assert!(admission::acquire(&directory, &storage.policy, None)
        .unwrap_err()
        .message
        .contains("expired"));
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    assert!(
        admission::acquire(&directory, &storage.policy, Some(&AtomicBool::new(true)))
            .unwrap_err()
            .cancelled
    );
    drop(lock);
    std::fs::set_permissions(
        directory.path().join("lock"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let started = std::time::Instant::now();
    assert!(admission::acquire(&directory, &storage.policy, None).is_err());
    assert!(started.elapsed() < storage.policy.retry);
    fixture.unchanged(&before);
}
#[test]
fn checksummed_records_reject_unknown_fields_overflow_and_wrong_fixed_names() {
    let fixture = Fixture::new();
    let directory = fixture.storage().initialize(&fixture.roots()).unwrap();
    let mut claim = record::Claim::new(
        directory.identity().unwrap(),
        format!("d-{}", "a".repeat(32)),
        &[b"x".to_vec(), b"y".to_vec()],
    )
    .unwrap();
    let encoded = record::encode(&claim).unwrap();
    assert_eq!(record::decode::<record::Claim>(&encoded).unwrap(), claim);
    let mut value: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    value["extra"] = serde_json::json!(true);
    assert!(record::decode::<record::Claim>(&serde_json::to_vec(&value).unwrap()).is_err());
    claim.reserved_bytes = u64::MAX;
    assert!(claim.validate().is_err());
    claim.reserved_bytes = record::OVERHEAD + 2;
    claim.files[0].name = "right".into();
    assert!(claim.validate().is_err());
    claim.files[0].name = "left".into();
    claim.files[0].sha256 = "Z".repeat(64);
    assert!(claim.validate().is_err());
    assert!(record::decode::<record::Claim>(&vec![b' '; record::JSON_LIMIT + 1]).is_err());
}

fn checkpoint(base: &Path, id: &str, phase: &str) {
    std::fs::write(base.join(format!("child-{id}-{phase}")), b"ready").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !base.join(format!("child-{id}-continue")).exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "Child checkpoint expired"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
struct OwnedChild {
    child: std::process::Child,
    base: PathBuf,
    id: String,
}
impl OwnedChild {
    fn spawn(fixture: &Fixture, mode: &str, id: &str) -> Self {
        let stdout =
            std::fs::File::create(fixture.0.join(format!("child-{id}-stdout.log"))).unwrap();
        let stderr =
            std::fs::File::create(fixture.0.join(format!("child-{id}-stderr.log"))).unwrap();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "linux_diff::tests::native_child",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("PAPERWING_DIFF_FIXTURE", &fixture.0)
            .env("PAPERWING_DIFF_MODE", mode)
            .env("PAPERWING_DIFF_CHILD_ID", id)
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .unwrap();
        Self {
            child,
            base: fixture.0.clone(),
            id: id.into(),
        }
    }
    fn ready(&mut self, phase: &str) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !self
            .base
            .join(format!("child-{}-{phase}", self.id))
            .exists()
        {
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "Child exited before checkpoint"
            );
            assert!(
                std::time::Instant::now() < deadline,
                "Checkpoint unavailable"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    fn release(&self) {
        std::fs::write(
            self.base.join(format!("child-{}-continue", self.id)),
            b"continue",
        )
        .unwrap();
    }
    fn finish(&mut self) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "child {} failed: {}",
                    self.id,
                    std::fs::read_to_string(
                        self.base.join(format!("child-{}-stderr.log", self.id))
                    )
                    .unwrap()
                );
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "Child completion expired"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}
#[test]
fn two_processes_race_missing_lock_and_reopen_only_the_safe_winner() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let mut left = OwnedChild::spawn(&fixture, "bootstrap", "left");
    let mut right = OwnedChild::spawn(&fixture, "bootstrap", "right");
    left.ready("missing-lock");
    right.ready("missing-lock");
    left.release();
    right.release();
    left.finish();
    right.finish();
    assert_eq!(usage(&fixture, &fixture.storage()).allocations, 2);
    fixture.unchanged(&before);
}
#[test]
fn competing_process_releases_lock_within_the_production_deadline() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let mut holder = OwnedChild::spawn(&fixture, "holder", "holder");
    holder.ready("held");
    let started = std::time::Instant::now();
    let mut worker = OwnedChild::spawn(&fixture, "reserve", "waiter");
    worker.ready("lock-busy");
    holder.release();
    holder.finish();
    worker.finish();
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert_eq!(usage(&fixture, &fixture.storage()).allocations, 1);
    fixture.unchanged(&before);
}
#[test]
fn missing_lock_bootstrap_does_not_adopt_a_link_or_unsafe_winner() {
    for unsafe_mode in [false, true] {
        let fixture = Fixture::new();
        let directory = fixture.storage().initialize(&fixture.roots()).unwrap();
        let path = directory.path().join("lock");
        let sentinel = fixture.0.join("outside");
        storage::DIFF_STORAGE_HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move |phase| {
                if phase == "missing-lock" {
                    if unsafe_mode {
                        std::fs::write(&path, b"").unwrap();
                    } else {
                        std::os::unix::fs::symlink(&sentinel, &path).unwrap();
                    }
                }
                Ok(())
            }))
        });
        assert!(directory.try_diff_lock().is_err());
        storage::DIFF_STORAGE_HOOK.with(|hook| *hook.borrow_mut() = None);
        assert_eq!(directory.names(2).unwrap(), vec!["lock"]);
        assert_eq!(
            std::fs::read(fixture.0.join("outside")).unwrap(),
            b"outside-sentinel"
        );
    }
}
#[test]
fn lock_replacement_while_waiting_refuses_instead_of_taking_new_authority() {
    let fixture = Fixture::new();
    let storage = fixture.storage();
    let directory = storage.initialize(&fixture.roots()).unwrap();
    let lock = admission::acquire(&directory, &storage.policy, None).unwrap();
    let path = directory.path().join("lock");
    let saved = directory.path().join("saved-lock");
    storage::DIFF_STORAGE_HOOK.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move |phase| {
            if phase == "lock-busy" && !saved.exists() {
                std::fs::rename(&path, &saved).unwrap();
                std::fs::write(&path, b"").unwrap();
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            Ok(())
        }))
    });
    let error = admission::acquire(&directory, &storage.policy, None).unwrap_err();
    assert!(error.message.contains("identity"));
    storage::DIFF_STORAGE_HOOK.with(|hook| *hook.borrow_mut() = None);
    drop(lock);
}

#[tokio::test]
async fn private_materialization_cleans_only_its_exact_files_and_releases_capacity() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let storage = Arc::new(fixture.storage());
    let lease = storage
        .materialize(
            fixture.roots(),
            Arc::new(AtomicBool::new(false)),
            [b"private-left", b"private-right"],
        )
        .await
        .unwrap();
    let path = lease.path().to_path_buf();
    assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o700);
    for (name, bytes) in [
        ("left", b"private-left".as_slice()),
        ("right", b"private-right".as_slice()),
    ] {
        assert_eq!(std::fs::read(path.join(name)).unwrap(), bytes);
        assert_eq!(
            std::fs::metadata(path.join(name)).unwrap().mode() & 0o777,
            0o600
        );
    }
    assert_eq!(usage(&fixture, &storage).allocations, 1);
    lease.finish().await.unwrap();
    assert!(!path.exists());
    assert_eq!(usage(&fixture, &storage).allocations, 0);
    assert_eq!(storage.work.available_permits(), 4);
    fixture.unchanged(&before);
}
#[tokio::test]
async fn changed_content_identity_metadata_and_substituted_directory_retain_full_claims() {
    for kind in [
        "bytes",
        "inode",
        "mode",
        "attribute",
        "directory",
        "directory-mode",
        "directory-attribute",
    ] {
        let fixture = Fixture::new();
        let before = fixture.snapshot();
        let storage = Arc::new(fixture.storage());
        let lease = storage
            .materialize(
                fixture.roots(),
                Arc::new(AtomicBool::new(false)),
                [b"private-left", b"private-right"],
            )
            .await
            .unwrap();
        let path = lease.path().to_path_buf();
        let charged = usage(&fixture, &storage).bytes;
        match kind {
            "bytes" => std::fs::write(path.join("left"), b"foreign-left").unwrap(),
            "inode" => {
                std::fs::rename(path.join("left"), fixture.0.join("saved-file")).unwrap();
                std::fs::write(path.join("left"), b"private-left").unwrap();
                std::fs::set_permissions(path.join("left"), std::fs::Permissions::from_mode(0o600))
                    .unwrap();
            }
            "mode" => {
                std::fs::set_permissions(path.join("left"), std::fs::Permissions::from_mode(0o644))
                    .unwrap()
            }
            "attribute" => {
                let file = std::fs::File::open(path.join("left")).unwrap();
                rustix::fs::fsetxattr(file, "user.changed", b"x", rustix::fs::XattrFlags::empty())
                    .unwrap();
            }
            "directory-mode" => {
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap()
            }
            "directory-attribute" => {
                let file = std::fs::File::open(&path).unwrap();
                rustix::fs::fsetxattr(file, "user.changed", b"x", rustix::fs::XattrFlags::empty())
                    .unwrap();
            }
            "directory" => {
                std::fs::rename(&path, fixture.0.join("saved-allocation")).unwrap();
                std::fs::create_dir(&path).unwrap();
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(lease.finish().await.is_err(), "{kind}");
        assert_eq!(storage.work.available_permits(), 4);
        let namespace = fixture.0.join("data/linux-diff-v1");
        assert!(std::fs::read_dir(&namespace).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("reservation-")));
        if kind == "bytes" {
            assert_eq!(usage(&fixture, &storage).bytes, charged);
            assert_eq!(std::fs::read(path.join("left")).unwrap(), b"foreign-left");
        }
        fixture.unchanged(&before);
    }
}
#[tokio::test]
async fn cleanup_unlink_sync_and_preclaim_interruption_leave_durable_charges() {
    for phase in [
        "unlink-file",
        "sync-file-removal",
        "remove-directory",
        "manifest-removal",
        "before-claim-removal",
    ] {
        let fixture = Fixture::new();
        let before = fixture.snapshot();
        let mut storage = fixture.storage();
        let enabled = Arc::new(AtomicBool::new(false));
        let hook_enabled = enabled.clone();
        let target = phase.to_string();
        storage.hook = Some(Arc::new(move |phase| {
            if hook_enabled.load(Ordering::Relaxed) && phase == target {
                Err(crate::linux_guard::Error::io(
                    std::io::Error::from_raw_os_error(libc::EIO),
                ))
            } else {
                Ok(())
            }
        }));
        let storage = Arc::new(storage);
        let lease = storage
            .materialize(
                fixture.roots(),
                Arc::new(AtomicBool::new(false)),
                [b"l", b"r"],
            )
            .await
            .unwrap();
        let charged = usage(&fixture, &storage).bytes;
        enabled.store(true, Ordering::Relaxed);
        assert!(lease.finish().await.is_err(), "{phase}");
        assert_eq!(usage(&fixture, &storage).bytes, charged);
        assert_eq!(usage(&fixture, &storage).allocations, 1);
        assert_eq!(storage.work.available_permits(), 4);
        fixture.unchanged(&before);
    }
}
async fn wait_capacity(storage: &Storage) {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while storage.work.available_permits() != 4 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn four_dropped_leases_keep_work_ownership_and_cleanup_without_reacquiring_permits() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let storage = Arc::new(fixture.storage());
    let mut leases = Vec::new();
    for _ in 0..4 {
        leases.push(
            storage
                .materialize(
                    fixture.roots(),
                    Arc::new(AtomicBool::new(false)),
                    [b"l", b"r"],
                )
                .await
                .unwrap(),
        );
    }
    assert_eq!(storage.work.available_permits(), 0);
    let mut holder = OwnedChild::spawn(&fixture, "holder", "drop-holder");
    holder.ready("held");
    drop(leases);
    tokio::task::yield_now().await;
    assert_eq!(storage.work.available_permits(), 0);
    holder.release();
    holder.finish();
    wait_capacity(&storage).await;
    assert_eq!(usage(&fixture, &storage).allocations, 0);
    fixture.unchanged(&before);
}
#[tokio::test]
async fn cleanup_deadline_keeps_all_claims_charged_and_releases_work_permits() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let mut storage = fixture.storage();
    storage.policy.deadline = std::time::Duration::from_millis(100);
    storage.policy.retry = std::time::Duration::from_millis(5);
    let storage = Arc::new(storage);
    let mut leases = Vec::new();
    for _ in 0..4 {
        leases.push(
            storage
                .materialize(
                    fixture.roots(),
                    Arc::new(AtomicBool::new(false)),
                    [b"l", b"r"],
                )
                .await
                .unwrap(),
        );
    }
    let charged = usage(&fixture, &storage).bytes;
    let directory = storage.initialize(&fixture.roots()).unwrap();
    let lock = admission::acquire(&directory, &storage.policy, None).unwrap();
    drop(leases);
    wait_capacity(&storage).await;
    drop(lock);
    assert_eq!(usage(&fixture, &storage).bytes, charged);
    assert_eq!(usage(&fixture, &storage).allocations, 4);
    fixture.unchanged(&before);
}
#[test]
fn manifest_publication_is_serialized_and_a_crashed_torn_manifest_refuses_admission() {
    for crash in [false, true] {
        let fixture = Fixture::new();
        let before = fixture.snapshot();
        let mut publisher = OwnedChild::spawn(&fixture, "manifest", "publisher");
        publisher.ready("manifest-created");
        let namespace = fixture.0.join("data/linux-diff-v1");
        let manifests = std::fs::read_dir(&namespace)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("allocation-")
            })
            .collect::<Vec<_>>();
        assert_eq!(manifests.len(), 1);
        assert_eq!(std::fs::metadata(&manifests[0]).unwrap().len(), 0);
        if crash {
            publisher.child.kill().unwrap();
            publisher.child.wait().unwrap();
            let storage = fixture.storage();
            assert!(storage
                .reserve(&fixture.roots(), &AtomicBool::new(false), &[vec![], vec![]])
                .is_err());
            assert!(manifests[0].exists());
            let claim = std::fs::read_dir(&namespace)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|path| {
                    path.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("reservation-")
                })
                .unwrap();
            let bytes = std::fs::read(claim).unwrap();
            let claim: record::Claim = record::decode(&bytes).unwrap();
            assert_eq!(claim.reserved_bytes, record::OVERHEAD + 11);
        } else {
            let mut competitor = OwnedChild::spawn(&fixture, "reserve", "competitor");
            competitor.ready("lock-busy");
            publisher.release();
            publisher.finish();
            competitor.finish();
            assert_eq!(usage(&fixture, &fixture.storage()).allocations, 1);
        }
        fixture.unchanged(&before);
    }
}

#[test]
fn final_line_counts_and_git_counters_match_across_two_process_bootstrap_and_contention() {
    for bootstrap in [true, false] {
        let fixture = Fixture::new();
        let before = fixture.snapshot();
        let start = std::time::Instant::now();
        if bootstrap {
            let mut left = OwnedChild::spawn(&fixture, "counts-bootstrap", "left");
            let mut right = OwnedChild::spawn(&fixture, "counts-bootstrap", "right");
            left.ready("missing-lock");
            right.ready("missing-lock");
            left.release();
            right.release();
            left.finish();
            right.finish();
        } else {
            let mut holder = OwnedChild::spawn(&fixture, "holder", "holder");
            holder.ready("held");
            let mut waiter = OwnedChild::spawn(&fixture, "counts-waiter", "waiter");
            waiter.ready("lock-busy");
            holder.release();
            holder.finish();
            waiter.finish();
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        assert_eq!(usage(&fixture, &fixture.storage()).allocations, 0);
        fixture.unchanged(&before);
    }
}
#[tokio::test]
async fn final_line_counts_initialize_missing_suffix_and_leave_only_the_private_lock() {
    let _serial = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let path = fixture.0.join("fresh/app/data");
    let storage = Arc::new(Storage::new(path.clone()).unwrap());
    assert!(!path.exists());
    let result = crate::compare::native_diff_counts(crate::compare::NativeDiffTest {
        storage: storage.clone(),
        roots: fixture.roots(),
        cancel: Arc::new(AtomicBool::new(false)),
    })
    .await
    .unwrap();
    assert_eq!(result["lines"], serde_json::json!({"added":2,"removed":1}));
    assert!(path.join("linux-diff-v1/lock").exists());
    assert_eq!(storage.work.available_permits(), 4);
    let namespace = storage.initialize(&fixture.roots()).unwrap();
    assert_eq!(namespace.names(2).unwrap(), vec!["lock"]);
    fixture.unchanged(&before);
}
#[tokio::test(flavor = "current_thread")]
async fn independently_delayed_native_storage_keeps_current_thread_heartbeat_running() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let started = Arc::new(tokio::sync::Notify::new());
    let notified = started.notified();
    let (send, receive) = std::sync::mpsc::channel();
    let receive = std::sync::Mutex::new(receive);
    let signal = started.clone();
    let mut storage = fixture.storage();
    storage.hook = Some(Arc::new(move |phase| {
        if phase == "storage-start" {
            signal.notify_one();
            receive
                .lock()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(3))
                .unwrap();
        }
        Ok(())
    }));
    let storage = Arc::new(storage);
    let materializing = storage.clone();
    let roots = fixture.roots();
    let task = tokio::spawn(async move {
        materializing
            .materialize(roots, Arc::new(AtomicBool::new(false)), [b"left", b"right"])
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(1), notified)
        .await
        .unwrap();
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_millis(5));
    let mut ticks = 0;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while ticks < 10 {
            heartbeat.tick().await;
            ticks += 1;
        }
    })
    .await
    .unwrap();
    assert!(!task.is_finished());
    send.send(()).unwrap();
    task.await.unwrap().unwrap().finish().await.unwrap();
    assert_eq!(ticks, 10);
    fixture.unchanged(&before);
}
#[tokio::test]
async fn cancelling_and_abandoning_waiting_allocation_creates_no_claim_and_returns_capacity() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let storage = Arc::new(fixture.storage());
    let directory = storage.initialize(&fixture.roots()).unwrap();
    let lock = admission::acquire(&directory, &storage.policy, None).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let input_cancel = cancel.clone();
    let running = storage.clone();
    let roots = fixture.roots();
    let started = Arc::new(tokio::sync::Notify::new());
    let signal = started.clone();
    let mut second = fixture.storage();
    second.hook = Some(Arc::new(move |phase| {
        if phase == "lock-busy" {
            signal.notify_one();
        }
        Ok(())
    }));
    let second = Arc::new(second);
    let allocation = second.clone();
    let task = tokio::spawn(async move {
        allocation
            .materialize(roots, input_cancel, [b"l", b"r"])
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(1), started.notified())
        .await
        .unwrap();
    cancel.store(true, Ordering::Relaxed);
    let error = task.await.unwrap().err().unwrap();
    assert!(error.cancelled);
    assert_eq!(directory.names(2).unwrap(), vec!["lock"]);
    assert_eq!(second.work.available_permits(), 4);
    drop(lock);
    let roots = fixture.roots();
    let task = tokio::spawn(async move {
        running
            .materialize(roots, Arc::new(AtomicBool::new(false)), [b"l", b"r"])
            .await
    });
    task.abort();
    let _ = task.await;
    wait_capacity(&storage).await;
    assert_eq!(usage(&fixture, &storage).allocations, 0);
    fixture.unchanged(&before);
}

#[test]
fn initializer_create_races_verify_safe_winners_and_refuse_unsafe_winners() {
    for safe in [true, false] {
        let fixture = Fixture::new();
        let before = fixture.snapshot();
        let root = fixture.0.clone();
        storage::INITIALIZE_HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move |parent| {
                let winner = parent.join("new");
                if parent == root.join("data") && !winner.exists() {
                    if safe {
                        std::fs::create_dir(&winner).unwrap();
                        std::fs::set_permissions(&winner, std::fs::Permissions::from_mode(0o700))
                            .unwrap();
                    } else {
                        std::os::unix::fs::symlink(root.join("metadata"), winner).unwrap();
                    }
                }
            }))
        });
        let result = Storage::new(fixture.0.join("data/new/app"))
            .unwrap()
            .initialize(&fixture.roots());
        assert_eq!(result.is_ok(), safe);
        storage::INITIALIZE_HOOK.with(|hook| *hook.borrow_mut() = None);
        assert!(!fixture.0.join("metadata/app").exists());
        fixture.unchanged(&before);
    }
}
#[tokio::test]
async fn source_capture_is_bounded_and_memory_admission_precedes_owned_inputs_or_artifacts() {
    let _exclusive = crate::test_support::Exclusive::new();
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let storage = Arc::new(fixture.storage());
    let root =
        crate::paths::ReadRoot::new(&fixture.0.join("left"), vec![fixture.0.join("metadata")])
            .unwrap();
    let expected = root.linux_value().unwrap();
    let values = storage
        .capture(vec![root.clone()], &AtomicBool::new(false))
        .await
        .unwrap();
    assert_eq!(values, vec![expected]);
    assert!(storage
        .capture(vec![], &AtomicBool::new(false))
        .await
        .is_err());
    assert!(storage
        .capture(
            vec![root.clone(), root.clone(), root],
            &AtomicBool::new(false)
        )
        .await
        .is_err());
    let budget = crate::linux_guard::BytePermit::acquire(512 * 1024 * 1024).unwrap();
    assert!(storage
        .materialize(
            fixture.roots(),
            Arc::new(AtomicBool::new(false)),
            [b"l", b"r"]
        )
        .await
        .is_err());
    assert!(!fixture.0.join("data/linux-diff-v1").exists());
    drop(budget);
    assert_eq!(storage.work.available_permits(), 4);
    fixture.unchanged(&before);
}
#[test]
fn a_live_storage_owner_refuses_namespace_replacement_before_new_claims() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let storage = fixture.storage();
    let reservation = storage
        .reserve(&fixture.roots(), &AtomicBool::new(false), &[vec![], vec![]])
        .unwrap();
    let namespace = reservation.namespace.path().to_path_buf();
    std::fs::rename(&namespace, fixture.0.join("saved-namespace")).unwrap();
    std::fs::create_dir(&namespace).unwrap();
    std::fs::set_permissions(&namespace, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(storage
        .reserve(&fixture.roots(), &AtomicBool::new(false), &[vec![], vec![]])
        .is_err());
    assert_eq!(std::fs::read_dir(namespace).unwrap().count(), 0);
    assert!(reservation.file.read(record::JSON_LIMIT).is_err());
    fixture.unchanged(&before);
}

#[test]
fn foreign_file_ownership_refuses_cleanup_and_admission_in_an_owned_user_namespace() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let script = r#"import os,pathlib,subprocess,sys
root=pathlib.Path(sys.argv[1]);exe=sys.argv[2]
assert root.is_absolute() and root.resolve()==root
assert (root/'.paperwing-diff-fixture').read_bytes()==b'paperwing-diff-fixture-v1\n'
assert os.geteuid()==0
env=os.environ.copy();env['PAPERWING_DIFF_FIXTURE']=str(root);env['PAPERWING_DIFF_MODE']='foreign-owner'
subprocess.run([exe,'--exact','linux_diff::tests::native_child','--ignored','--nocapture','--test-threads=1'],env=env,check=True,timeout=20)
"#;
    let output = std::process::Command::new("unshare")
        .args([
            "--user",
            "--map-auto",
            "--map-root-user",
            "python3",
            "-c",
            script,
        ])
        .arg(&fixture.0)
        .arg(std::env::current_exe().unwrap())
        .output()
        .unwrap();
    std::fs::write(fixture.0.join("foreign-owner-stdout.log"), &output.stdout).unwrap();
    std::fs::write(fixture.0.join("foreign-owner-stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fixture.0.join("foreign-owner-proof.json").exists());
    fixture.unchanged(&before);
}

#[test]
fn shutdown_without_cleanup_dispatch_retains_artifacts_and_releases_owned_capacity() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let storage = Arc::new(fixture.storage());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let lease = runtime
        .block_on(storage.materialize(
            fixture.roots(),
            Arc::new(AtomicBool::new(false)),
            [b"l", b"r"],
        ))
        .unwrap();
    let path = lease.path().to_path_buf();
    let charged = usage(&fixture, &storage).bytes;
    runtime.shutdown_background();
    drop(lease);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while storage.work.available_permits() != 4 {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(path.join("left").exists());
    assert!(path.join("right").exists());
    assert_eq!(usage(&fixture, &storage).bytes, charged);
    assert_eq!(usage(&fixture, &storage).allocations, 1);
    fixture.unchanged(&before);
}

#[tokio::test]
async fn stale_source_after_reservation_refuses_before_creating_the_allocation_directory() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let root = fixture.0.clone();
    let created = Arc::new(AtomicBool::new(false));
    let observed = created.clone();
    let mut storage = fixture.storage();
    storage.hook = Some(Arc::new(move |phase| {
        if phase == "before-directory" {
            std::fs::rename(root.join("left"), root.join("saved-left")).unwrap();
            std::fs::create_dir(root.join("left")).unwrap();
        }
        if phase == "directory-created" {
            observed.store(true, Ordering::Relaxed);
        }
        Ok(())
    }));
    let storage = Arc::new(storage);
    assert!(storage
        .materialize(
            fixture.roots(),
            Arc::new(AtomicBool::new(false)),
            [b"l", b"r"]
        )
        .await
        .is_err());
    std::fs::remove_dir(fixture.0.join("left")).unwrap();
    std::fs::rename(fixture.0.join("saved-left"), fixture.0.join("left")).unwrap();
    fixture.unchanged(&before);
    assert!(!created.load(Ordering::Relaxed));
}

#[tokio::test]
async fn abandoning_started_materialization_holds_capacity_until_its_independent_cleanup_finishes()
{
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let started = Arc::new(tokio::sync::Notify::new());
    let signal = started.clone();
    let (send, receive) = std::sync::mpsc::channel();
    let receive = std::sync::Mutex::new(receive);
    let mut storage = fixture.storage();
    storage.hook = Some(Arc::new(move |phase| {
        if phase == "before-directory" {
            signal.notify_one();
            receive
                .lock()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(3))
                .unwrap();
        }
        Ok(())
    }));
    let storage = Arc::new(storage);
    let running = storage.clone();
    let roots = fixture.roots();
    let task = tokio::spawn(async move {
        running
            .materialize(roots, Arc::new(AtomicBool::new(false)), [b"l", b"r"])
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(1), started.notified())
        .await
        .unwrap();
    assert_eq!(storage.work.available_permits(), 3);
    task.abort();
    assert!(matches!(task.await,Err(error) if error.is_cancelled()));
    assert_eq!(storage.work.available_permits(), 3);
    send.send(()).unwrap();
    wait_capacity(&storage).await;
    assert_eq!(usage(&fixture, &storage).allocations, 0);
    fixture.unchanged(&before);
}
#[test]
fn killed_reservation_and_preclaim_cleanup_keep_full_charges_after_restart() {
    for (mode, phase) in [
        ("reservation-pause", "before-directory"),
        ("cleanup-pause", "before-claim-removal"),
    ] {
        let fixture = Fixture::new();
        let before = fixture.snapshot();
        let mut child = OwnedChild::spawn(&fixture, mode, "interrupted");
        child.ready(phase);
        let namespace = fixture.0.join("data/linux-diff-v1");
        let mut names = std::fs::read_dir(&namespace)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(names.len(), 2);
        child.child.kill().unwrap();
        let status = child.child.wait().unwrap();
        assert!(!status.success());
        let mut restarted = fixture.storage();
        restarted.policy.allocations = 1;
        let inventory = usage(&fixture, &restarted);
        assert_eq!(inventory.allocations, 1);
        let claim_path = std::fs::read_dir(&namespace)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("reservation-")
            })
            .unwrap();
        let encoded = std::fs::read(&claim_path).unwrap();
        let claim: record::Claim = record::decode(&encoded).unwrap();
        assert_eq!(claim.reserved_bytes, record::OVERHEAD + 2);
        assert!(!namespace.join(&claim.directory_name).exists());
        assert!(restarted
            .reserve(&fixture.roots(), &AtomicBool::new(false), &[vec![], vec![]])
            .is_err());
        let mut after = std::fs::read_dir(&namespace)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        after.sort();
        assert_eq!(after, names);
        std::fs::write(fixture.0.join("interruption-proof.json"),serde_json::to_vec(&serde_json::json!({"phase":phase,"reservedBytes":claim.reserved_bytes,"inventoryBytes":inventory.bytes,"fullChargeRetained":true,"processReaped":true,"powerLoss":false})).unwrap()).unwrap();
        fixture.unchanged(&before);
    }
}

#[test]
fn missing_suffix_does_not_create_an_ancestor_of_absent_protected_metadata() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let metadata = fixture.0.join("uncreated/metadata");
    let roots = vec![Root::open(&fixture.0.join("left"), &[metadata])
        .unwrap()
        .value()
        .unwrap()];
    let storage = Storage::new(fixture.0.join("uncreated/app")).unwrap();
    assert!(storage.initialize(&roots).is_err());
    assert!(!fixture.0.join("uncreated").exists());
    fixture.unchanged(&before);
}

#[test]
fn admission_precharges_namespace_growth_before_accepting_another_reservation() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let mut storage = fixture.storage();
    for _ in 0..128 {
        let current = usage(&fixture, &storage);
        storage.policy.bytes = current.bytes + record::OVERHEAD;
        let reservation =
            storage.reserve(&fixture.roots(), &AtomicBool::new(false), &[vec![], vec![]]);
        if let Ok(reservation) = reservation {
            let lock = admission::acquire(&reservation.namespace, &storage.policy, None).unwrap();
            assert!(
                admission::inventory(&reservation.namespace, &lock, &storage.policy).is_ok(),
                "Accepted reservation exceeds the logical quota after namespace growth"
            );
        }
    }
    fixture.unchanged(&before);
}

#[tokio::test(flavor = "current_thread")]
async fn materialization_retains_artifacts_when_native_overhead_exceeds_its_reservation() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let namespace = fixture.0.join("data/linux-diff-v1");
    let observed = namespace.clone();
    let mut storage = fixture.storage();
    storage.hook = Some(Arc::new(move |phase| {
        if phase == "directory-created" {
            let directory = std::fs::read_dir(&observed)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|path| path.is_dir())
                .unwrap();
            let mut files = Vec::new();
            for index in 0..4096 {
                let path = directory.join(format!("padding-{index:04}-{}", "x".repeat(80)));
                std::fs::write(&path, []).unwrap();
                files.push(path);
            }
            for path in files {
                std::fs::remove_file(path).unwrap();
            }
            assert!(std::fs::metadata(directory).unwrap().size() > record::OVERHEAD);
        }
        Ok(())
    }));
    let storage = Arc::new(storage);
    let result = storage
        .materialize(
            fixture.roots(),
            Arc::new(AtomicBool::new(false)),
            [b"left", b"right"],
        )
        .await;
    assert!(result.is_err());
    let names = std::fs::read_dir(&namespace)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(
        names.iter().any(|name| name.starts_with("reservation-")),
        "Overhead disproval erased its durable claim"
    );
    assert!(
        names.iter().any(|name| name.starts_with("allocation-")),
        "Overhead disproval erased its ready proof"
    );
    let directory = namespace.join(names.iter().find(|name| name.starts_with("d-")).unwrap());
    assert_eq!(std::fs::read(directory.join("left")).unwrap(), b"left");
    assert_eq!(std::fs::read(directory.join("right")).unwrap(), b"right");
    assert_eq!(storage.work.available_permits(), 4);
    fixture.unchanged(&before);
}

fn grow_private_directory(path: &Path, minimum: u64) {
    let mut files = Vec::new();
    for index in 0..16384 {
        if std::fs::metadata(path).unwrap().size() >= minimum {
            break;
        }
        let file = path.join(format!("growth-{index:05}-{}", "x".repeat(80)));
        std::fs::write(&file, []).unwrap();
        files.push(file);
    }
    for file in files {
        std::fs::remove_file(file).unwrap();
    }
    assert!(std::fs::metadata(path).unwrap().size() >= minimum);
}

#[test]
fn insufficient_namespace_prepayment_refuses_before_missing_app_data_is_created() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let parent = fixture.0.join("missing/app");
    let mut storage = Storage::new(parent.clone()).unwrap();
    storage.policy.bytes = storage::DIFF_NAMESPACE_BYTES + 4095;
    assert!(storage.initialize(&fixture.roots()).is_err());
    assert!(!fixture.0.join("missing").exists());
    fixture.unchanged(&before);
}

#[test]
fn non_ext4_storage_refuses_before_initializing_a_missing_suffix() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let parent = PathBuf::from("/dev/shm").join(format!(
        "paperwing-diff-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    assert!(!parent.exists());
    let result = Storage::new(parent.join("app/data"))
        .unwrap()
        .initialize(&fixture.roots());
    assert!(result.unwrap_err().message.contains("ext4"));
    assert!(!parent.exists());
    fixture.unchanged(&before);
}

#[test]
fn namespace_high_water_is_fully_prepaid_across_restart_and_stops_new_entries() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let storage = fixture.storage();
    let directory = storage.initialize(&fixture.roots()).unwrap();
    let lock = admission::acquire(&directory, &storage.policy, None).unwrap();
    let empty = admission::inventory(&directory, &lock, &storage.policy)
        .unwrap()
        .bytes;
    grow_private_directory(directory.path(), storage::DIFF_NAMESPACE_BYTES - 65536 + 1);
    assert!(directory.native_size().unwrap() <= storage::DIFF_NAMESPACE_BYTES);
    assert_eq!(
        admission::inventory(&directory, &lock, &storage.policy)
            .unwrap()
            .bytes,
        empty
    );
    drop(lock);
    let restarted = fixture.storage();
    assert_eq!(usage(&fixture, &restarted).bytes, empty);
    let names = directory.names(4).unwrap();
    assert!(restarted
        .reserve(&fixture.roots(), &AtomicBool::new(false), &[vec![], vec![]])
        .is_err());
    assert_eq!(directory.names(4).unwrap(), names);
    fixture.unchanged(&before);
}

#[test]
fn production_reservation_count_boundary_refuses_before_an_extra_claim() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let storage = fixture.storage();
    let directory = storage.initialize(&fixture.roots()).unwrap();
    let lock = admission::acquire(&directory, &storage.policy, None).unwrap();
    let identity = directory.identity().unwrap();
    for index in 0..1024 {
        let claim = record::Claim::new(
            identity.clone(),
            format!("d-{index:032x}"),
            &[vec![], vec![]],
        )
        .unwrap();
        let bytes = record::encode(&claim).unwrap();
        directory
            .write_new(
                &format!("reservation-{}.json", claim.directory_name),
                &bytes,
            )
            .unwrap();
    }
    assert_eq!(
        admission::inventory(&directory, &lock, &storage.policy)
            .unwrap()
            .allocations,
        1024
    );
    drop(lock);
    let names = directory.names(1026).unwrap();
    assert!(storage
        .reserve(&fixture.roots(), &AtomicBool::new(false), &[vec![], vec![]])
        .is_err());
    assert_eq!(directory.names(1026).unwrap(), names);
    fixture.unchanged(&before);
}

#[tokio::test(flavor = "current_thread")]
async fn late_namespace_exhaustion_preserves_claims_before_directory_and_manifest_creation() {
    for phase in ["before-directory", "directory-created"] {
        let fixture = Fixture::new();
        let before = fixture.snapshot();
        let namespace = fixture.0.join("data/linux-diff-v1");
        let observed = namespace.clone();
        let mut storage = fixture.storage();
        storage.hook = Some(Arc::new(move |current| {
            if current == phase {
                grow_private_directory(&observed, storage::DIFF_NAMESPACE_BYTES - 65536 + 1);
            }
            Ok(())
        }));
        let storage = Arc::new(storage);
        assert!(storage
            .materialize(
                fixture.roots(),
                Arc::new(AtomicBool::new(false)),
                [b"left", b"right"]
            )
            .await
            .is_err());
        let names = std::fs::read_dir(&namespace)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(names.iter().any(|name| name.starts_with("reservation-")));
        assert!(!names.iter().any(|name| name.starts_with("allocation-")));
        assert_eq!(
            names.iter().any(|name| name.starts_with("d-")),
            phase == "directory-created"
        );
        assert_eq!(storage.work.available_permits(), 4);
        fixture.unchanged(&before);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn disproved_namespace_growth_retains_claim_contents_and_releases_work() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let namespace = fixture.0.join("data/linux-diff-v1");
    let observed = namespace.clone();
    let armed = Arc::new(AtomicBool::new(false));
    let mut storage = fixture.storage();
    storage.hook = Some(Arc::new(move |phase| {
        if phase == "directory-created" {
            armed.store(true, Ordering::Relaxed);
        }
        if phase == "before-entry-postcheck" && armed.swap(false, Ordering::Relaxed) {
            let previous = std::fs::metadata(&observed).unwrap().size();
            grow_private_directory(&observed, previous + 65537);
        }
        Ok(())
    }));
    let storage = Arc::new(storage);
    let error = storage
        .materialize(
            fixture.roots(),
            Arc::new(AtomicBool::new(false)),
            [b"left", b"right"],
        )
        .await
        .err()
        .unwrap();
    assert!(error.retained);
    let names = std::fs::read_dir(&namespace)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(names.iter().any(|name| name.starts_with("reservation-")));
    assert!(names.iter().any(|name| name.starts_with("allocation-")));
    let directory = namespace.join(names.iter().find(|name| name.starts_with("d-")).unwrap());
    assert_eq!(std::fs::read(directory.join("left")).unwrap(), b"left");
    assert_eq!(std::fs::read(directory.join("right")).unwrap(), b"right");
    assert_eq!(storage.work.available_permits(), 4);
    fixture.unchanged(&before);
}
