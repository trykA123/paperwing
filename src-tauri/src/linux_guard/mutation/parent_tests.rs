use super::*;
use crate::linux_journal::Journal;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicU64 = AtomicU64::new(1);
struct Fixture {
    _budget: crate::test_support::Shared,
    path: PathBuf,
    before: serde_json::Value,
}
fn snapshot(path: &Path) -> serde_json::Value {
    let stat = std::fs::symlink_metadata(path).unwrap();
    serde_json::json!({"device":stat.dev(),"inode":stat.ino(),"uid":stat.uid(),"gid":stat.gid(),"mode":stat.mode(),"bytes":if stat.is_file(){Some(std::fs::read(path).unwrap())}else{None}})
}
impl Fixture {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.skillify/evidence/skein/14/a2/native/parents")
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
        for name in ["repo", "repo/.git", "data", "metadata", "alias"] {
            std::fs::create_dir(path.join(name)).unwrap();
        }
        std::fs::write(path.join("repo/file"), b"before").unwrap();
        std::fs::write(path.join("outside"), b"outside-sentinel").unwrap();
        let before = serde_json::json!({"source":snapshot(&path.join("repo/file")),"sentinel":snapshot(&path.join("outside")),"metadata":snapshot(&path.join("repo/.git"))});
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
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let after = serde_json::json!({"source":snapshot(&self.path.join("repo/file")),"sentinel":snapshot(&self.path.join("outside")),"metadata":snapshot(&self.path.join("repo/.git"))});
        assert_eq!(after, self.before);
        std::fs::write(
            self.path.join("after.json"),
            serde_json::to_vec(&after).unwrap(),
        )
        .unwrap();
    }
}
#[derive(Default)]
struct Authority {
    revoke: Arc<AtomicBool>,
    refreshed: u32,
    refresh: Option<Box<dyn FnOnce()>>,
}
impl OperationAuthority for Authority {
    fn refresh(&mut self) -> Result<(), Error> {
        self.refreshed += 1;
        if let Some(action) = self.refresh.take() {
            action();
        }
        Ok(())
    }
    fn check(&self) -> Result<(), Error> {
        if self.revoke.load(Ordering::Acquire) {
            Err(Error::conflict("Owned authority revoked"))
        } else {
            Ok(())
        }
    }
}
fn create(root: &Root, plan: &ParentPlan, created: &[AncestorValue]) -> ParentMutation {
    root.create_parent_component(
        ParentCreation { plan, created },
        &mut Authority::default(),
        || Ok(()),
    )
}
#[test]
fn value_only_preview_creates_nothing_and_sixty_three_components_create_exclusively() {
    let fixture = Fixture::new();
    let root = fixture.root();
    let zero = root.preview_parents("file").unwrap();
    assert!(zero.missing.is_empty());
    root.planned_parent(ParentCreation {
        plan: &zero,
        created: &[],
    })
    .unwrap();
    let path = (0..63)
        .map(|_| "p")
        .chain(std::iter::once("file"))
        .collect::<Vec<_>>()
        .join("/");
    let plan = root.preview_parents(&path).unwrap();
    assert_eq!(plan.missing.len(), 63);
    assert!(!fixture.path.join("repo/p").exists());
    assert_eq!(
        serde_json::from_slice::<ParentPlan>(&serde_json::to_vec(&plan).unwrap()).unwrap(),
        plan
    );
    let mut created = Vec::new();
    for next in &plan.missing {
        let result = create(&root, &plan, &created);
        assert!(result.applied);
        assert!(result.error.is_none());
        created.push(AncestorValue {
            relative: next.into(),
            identity: result.identity.unwrap(),
        });
        assert_eq!(
            std::fs::metadata(fixture.path.join("repo").join(next))
                .unwrap()
                .mode()
                & 0o777,
            0o700
        );
    }
    let parent = root
        .planned_parent(ParentCreation {
            plan: &plan,
            created: &created,
        })
        .unwrap();
    assert_eq!(parent.ancestors().unwrap(), created);
    assert_eq!(parent.snapshot().unwrap(), Snapshot::Missing);
    assert!(root.preview_parents(&(path + "/too-deep")).is_err());
}
#[test]
fn preview_refuses_links_nested_metadata_and_invalid_or_changed_prefixes() {
    let fixture = Fixture::new();
    let root = fixture.root();
    symlink(fixture.path.join("outside"), fixture.path.join("repo/link")).unwrap();
    assert!(root.preview_parents("link/file").is_err());
    assert!(root.preview_parents(".git/missing/file").is_err());
    std::fs::create_dir_all(fixture.path.join("repo/nested/.git")).unwrap();
    assert!(root.preview_parents("nested/missing/file").is_err());
    std::fs::create_dir(fixture.path.join("repo/existing")).unwrap();
    let plan = root.preview_parents("existing/new/file").unwrap();
    assert_eq!(plan.existing.len(), 1);
    let mut invalid = plan.clone();
    invalid.missing[0] = "gap/new".into();
    assert!(invalid.validate().is_err());
    std::fs::rename(
        fixture.path.join("repo/existing"),
        fixture.path.join("original-prefix"),
    )
    .unwrap();
    std::fs::create_dir(fixture.path.join("repo/existing")).unwrap();
    let result = create(&root, &plan, &[]);
    assert!(!result.applied);
    assert!(result.error.is_some());
    assert!(!fixture.path.join("repo/existing/new").exists());
}
#[test]
fn exclusive_parent_creation_refuses_existing_winner_and_final_revocation() {
    let fixture = Fixture::new();
    let root = fixture.root();
    let plan = root.preview_parents("winner/child/file").unwrap();
    std::fs::create_dir(fixture.path.join("repo/winner")).unwrap();
    let result = create(&root, &plan, &[]);
    assert!(!result.applied);
    assert!(result.identity.is_none());
    assert!(!fixture.path.join("repo/winner/child").exists());
    let plan = root.preview_parents("cancelled/file").unwrap();
    let mut authority = Authority::default();
    let revoke = authority.revoke.clone();
    let result = root.create_parent_component(
        ParentCreation {
            plan: &plan,
            created: &[],
        },
        &mut authority,
        move || {
            revoke.store(true, Ordering::Release);
            Ok(())
        },
    );
    assert!(!result.applied);
    assert!(result.error.is_some());
    assert_eq!(authority.refreshed, 1);
    assert!(!fixture.path.join("repo/cancelled").exists());
}
#[test]
fn slow_refresh_substitution_is_observed_before_parent_creation() {
    let fixture = Fixture::new();
    let root = fixture.root();
    std::fs::create_dir(fixture.path.join("repo/prefix")).unwrap();
    let plan = root.preview_parents("prefix/new/file").unwrap();
    let path = fixture.path.clone();
    let mut authority = Authority {
        refresh: Some(Box::new(move || {
            std::fs::rename(path.join("repo/prefix"), path.join("saved-prefix")).unwrap();
            std::fs::create_dir(path.join("repo/prefix")).unwrap();
        })),
        ..Authority::default()
    };
    let result = root.create_parent_component(
        ParentCreation {
            plan: &plan,
            created: &[],
        },
        &mut authority,
        || Ok(()),
    );
    assert!(!result.applied);
    assert!(result.error.is_some());
    assert!(!fixture.path.join("repo/prefix/new").exists());
}

#[test]
fn already_created_parent_permission_change_refuses_the_next_component() {
    let fixture = Fixture::new();
    let root = fixture.root();
    let plan = root.preview_parents("first/next/file").unwrap();
    let first = create(&root, &plan, &[]);
    assert!(first.error.is_none());
    let created = [AncestorValue {
        relative: plan.missing[0].clone().into(),
        identity: first.identity.unwrap(),
    }];
    std::fs::set_permissions(
        fixture.path.join("repo/first"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let next = create(&root, &plan, &created);
    assert!(!next.applied);
    assert!(next.error.is_some());
    assert!(!fixture.path.join("repo/first/next").exists());
}

#[test]
fn guarded_constructor_refuses_stale_roots_and_substituted_initialization_prefix() {
    let fixture = Fixture::new();
    let value = fixture.root().value().unwrap();
    std::fs::rename(
        fixture.path.join("repo/.git"),
        fixture.path.join("saved-metadata"),
    )
    .unwrap();
    std::fs::create_dir(fixture.path.join("repo/.git")).unwrap();
    let missing = fixture.path.join("stale-data/suffix");
    assert!(Journal::open_guarded(&missing, std::slice::from_ref(&value)).is_err());
    assert!(!fixture.path.join("stale-data").exists());
    std::fs::remove_dir(fixture.path.join("repo/.git")).unwrap();
    std::fs::rename(
        fixture.path.join("saved-metadata"),
        fixture.path.join("repo/.git"),
    )
    .unwrap();
    std::fs::create_dir(fixture.path.join("data/prefix")).unwrap();
    let target = fixture.path.join("data/prefix/missing");
    let path = fixture.path.clone();
    crate::linux_guard::storage::INITIALIZE_HOOK.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move |current| {
            if current == path.join("data/prefix") {
                std::fs::rename(path.join("data/prefix"), path.join("saved-prefix")).unwrap();
                std::fs::create_dir(path.join("data/prefix")).unwrap();
            }
        }))
    });
    let result = Journal::open_guarded(&target, &[value]);
    crate::linux_guard::storage::INITIALIZE_HOOK.with(|hook| hook.borrow_mut().take());
    assert!(result.is_err());
    assert!(!target.exists());
    assert!(!fixture.path.join("saved-prefix/missing").exists());
}
#[test]
fn post_mkdir_reopen_and_sync_failures_report_applied_and_retain_directory() {
    for phase in ["mkdir", "reopened", "parentSync"] {
        let fixture = Fixture::new();
        let root = fixture.root();
        let plan = root.preview_parents("new/file").unwrap();
        parents::PARENT_HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move |current| {
                if current == phase {
                    Err(Error::io(std::io::Error::from_raw_os_error(libc::EIO)))
                } else {
                    Ok(())
                }
            }))
        });
        let result = create(&root, &plan, &[]);
        parents::PARENT_HOOK.with(|hook| hook.borrow_mut().take());
        assert!(result.applied);
        assert!(result.error.is_some());
        assert_eq!(result.identity.is_some(), phase != "mkdir");
        assert!(fixture.path.join("repo/new").is_dir());
        assert!(!fixture.path.join("repo/new/file").exists());
        std::fs::write(fixture.path.join("outcome.json"),serde_json::to_vec(&serde_json::json!({"phase":phase,"applied":result.applied,"identityCaptured":result.identity.is_some(),"directoryRetained":true})).unwrap()).unwrap();
    }
}
#[test]
fn guarded_constructor_checks_storage_separation_before_any_missing_suffix() {
    let fixture = Fixture::new();
    let root = fixture.root();
    let value = root.value().unwrap();
    let inside = fixture.path.join("repo/missing/data");
    assert!(Journal::open_guarded(&inside, std::slice::from_ref(&value)).is_err());
    assert!(!fixture.path.join("repo/missing").exists());
    let root = Root::open(
        &fixture.path.join("repo"),
        &[fixture.path.join("metadata/absent/protected")],
    )
    .unwrap();
    let app_data = fixture.path.join("metadata/absent/store");
    assert!(Journal::open_guarded(&app_data, &[root.value().unwrap()]).is_err());
    assert!(!fixture.path.join("metadata/absent").exists());
    let outside = fixture.path.join("outside-data/one/two");
    let journal = Journal::open_guarded(&outside, std::slice::from_ref(&value)).unwrap();
    journal.parent_authority(&value).unwrap();
    assert!(outside.join("linux-recovery-v1/lock").is_file());
    drop(journal);
    let legacy = Journal::open(&outside).unwrap();
    assert!(legacy.parent_authority(&value).is_err());
}
#[test]
fn noncreating_reader_leaves_missing_storage_and_missing_lock_untouched() {
    let fixture = Fixture::new();
    let missing = fixture.path.join("missing/data");
    assert!(Journal::open_existing(&missing).unwrap().is_none());
    assert!(!fixture.path.join("missing").exists());
    assert!(Journal::open_existing(&fixture.path.join("data"))
        .unwrap()
        .is_none());
    let namespace = fixture.path.join("data/linux-recovery-v1");
    std::fs::create_dir(&namespace).unwrap();
    std::fs::set_permissions(&namespace, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(Journal::open_existing(&fixture.path.join("data")).is_err());
    assert!(!namespace.join("lock").exists());
    let journal = Journal::open(&fixture.path.join("data")).unwrap();
    drop(journal);
    let reader = Journal::open_existing(&fixture.path.join("data"))
        .unwrap()
        .unwrap();
    std::fs::rename(fixture.path.join("repo"), fixture.path.join("saved-root")).unwrap();
    assert!(reader.list().unwrap().is_empty());
    std::fs::rename(fixture.path.join("saved-root"), fixture.path.join("repo")).unwrap();
}
struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn guarded_recovery_initializer_refuses_whole_and_nested_bind_aliases() {
    for mode in ["whole", "nested"] {
        let fixture = Fixture::new();
        let exe = std::env::current_exe().unwrap();
        let script="import os,pathlib,subprocess,sys\np=pathlib.Path(sys.argv[1]);mode=sys.argv[3]\nsource=p/'repo'\nif mode=='nested':\n source=p;source=p/'nested-mount';source.mkdir();subprocess.run(['mount','--bind',str(p/'repo'),str(source)],check=True)\nsubprocess.run(['mount','--bind',str(source),str(p/'alias')],check=True)\nenv=dict(os.environ,SKEIN_PARENT_BIND_FIXTURE=str(p),SKEIN_PARENT_BIND_MODE=mode)\nsubprocess.run([sys.argv[2],'--exact','linux_guard::mutation::parent_tests::native_bind_constructor_helper','--ignored','--nocapture','--test-threads=1'],env=env,check=True,timeout=15)\n";
        let log = std::fs::File::create(fixture.path.join("bind.log")).unwrap();
        let mut child = Child(
            std::process::Command::new("unshare")
                .args(["-Urnm", "python3", "-c", script])
                .arg(&fixture.path)
                .arg(&exe)
                .arg(mode)
                .stdout(log.try_clone().unwrap())
                .stderr(log)
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        let status = loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < deadline, "Owned bind child timeout");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(
            status.success(),
            "{}",
            std::fs::read_to_string(fixture.path.join("bind.log")).unwrap()
        );
        assert!(!fixture.path.join("repo/missing").exists());
        std::fs::write(
            fixture.path.join("reaping-proof.json"),
            b"{\"ownedChildReaped\":true,\"originalMountNamespacePreserved\":true}\n",
        )
        .unwrap();
    }
}
#[test]
#[ignore]
fn native_bind_constructor_helper() {
    let path = PathBuf::from(crate::env_names::var_os("SKEIN_PARENT_BIND_FIXTURE").unwrap());
    assert!(path.is_absolute());
    assert_eq!(
        std::fs::read(path.join(".skein-parent-fixture")).unwrap(),
        b"skein-parent-fixture-v1\n"
    );
    let root = Root::open(&path.join("repo"), &[]).unwrap();
    let value = root.value().unwrap();
    assert!(Journal::open_guarded(&path.join("alias/missing/data"), &[value]).is_err());
    assert!(!path.join("repo/missing").exists());
}
