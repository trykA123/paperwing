use super::*;
fn stage(path: &Path) -> PathBuf {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with(".paperwing-stage-")
        })
        .unwrap()
}
pub(super) fn substitute(path: &Path, mode: &str) {
    let parent = path.join("repo/prefix");
    match mode {
        "targetBytes" => std::fs::write(parent.join("target"), b"attacker").unwrap(),
        "targetIdentity" => {
            if parent.join("target").exists() {
                std::fs::rename(parent.join("target"), path.join("saved-target")).unwrap();
            }
            std::fs::write(parent.join("target"), b"attacker").unwrap();
        }
        "stageBytes" => std::fs::write(stage(&parent).join("content"), b"attacker-stage").unwrap(),
        "stageIdentity" => {
            let content = stage(&parent).join("content");
            std::fs::rename(&content, path.join("saved-content")).unwrap();
            std::fs::write(&content, b"after").unwrap();
            std::fs::set_permissions(&content, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        "stageDirectory" => {
            let saved = stage(&parent);
            std::fs::rename(&saved, path.join("saved-stage")).unwrap();
            std::fs::create_dir(&saved).unwrap();
            std::fs::set_permissions(&saved, std::fs::Permissions::from_mode(0o700)).unwrap();
            std::fs::write(saved.join("content"), b"after").unwrap();
            std::fs::set_permissions(
                saved.join("content"),
                std::fs::Permissions::from_mode(0o600),
            )
            .unwrap();
        }
        "root" => {
            std::fs::rename(path.join("repo"), path.join("saved-root")).unwrap();
            std::fs::create_dir(path.join("repo")).unwrap();
        }
        "metadata" => {
            std::fs::rename(path.join("repo/.git"), path.join("saved-metadata")).unwrap();
            std::fs::create_dir(path.join("repo/.git")).unwrap();
        }
        "prefix" => {
            std::fs::rename(&parent, path.join("saved-prefix")).unwrap();
            std::fs::create_dir(&parent).unwrap();
            std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        "lock" => {
            let lock = path.join("data/linux-recovery-v1/lock");
            std::fs::rename(&lock, path.join("saved-lock")).unwrap();
            std::fs::write(&lock, b"").unwrap();
            std::fs::set_permissions(&lock, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        "namespace" => {
            let namespace = path.join("data/linux-recovery-v1");
            std::fs::rename(&namespace, path.join("saved-namespace")).unwrap();
            std::fs::create_dir(&namespace).unwrap();
            std::fs::set_permissions(&namespace, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        _ => panic!("Unknown owned substitution"),
    }
}
fn check(mode: &'static str, replace: bool, missing: bool, final_slot: bool) {
    let fixture = Fixture::new();
    if !missing {
        std::fs::create_dir(fixture.path.join("repo/prefix")).unwrap();
    }
    if replace {
        std::fs::write(fixture.path.join("repo/prefix/target"), b"target-before").unwrap();
    }
    let journal = fixture.journal();
    let root = fixture.root();
    let expected = if missing {
        Snapshot::Missing
    } else {
        root.parent("prefix/target", false)
            .unwrap()
            .snapshot()
            .unwrap()
    };
    let plan = fixture.plan("prefix/target");
    let mut authority = Authority::default();
    let path = fixture.path.clone();
    if final_slot {
        parents::binding::FINAL_HOOK.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || {
                substitute(&path, mode);
                Ok(())
            }))
        });
    } else {
        let refresh = if missing { 4 } else { 3 };
        authority.action = Some(Box::new(move |count| {
            if count == refresh {
                let (start_send, start_recv) = std::sync::mpsc::channel();
                let (done_send, done_recv) = std::sync::mpsc::channel();
                let target = path.clone();
                let worker = std::thread::spawn(move || {
                    start_recv
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .unwrap();
                    substitute(&target, mode);
                    done_send.send(()).unwrap();
                });
                start_send.send(()).unwrap();
                done_recv
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
                worker.join().unwrap();
            }
            Ok(())
        }));
    }
    let result = journal.replace_with_parents(&plan, &expected, b"after", &mut authority);
    parents::binding::FINAL_HOOK.with(|slot| *slot.borrow_mut() = None);
    if mode == "root" {
        assert!(std::fs::read_dir(fixture.path.join("repo"))
            .unwrap()
            .next()
            .is_none());
        std::fs::remove_dir(fixture.path.join("repo")).unwrap();
        std::fs::rename(fixture.path.join("saved-root"), fixture.path.join("repo")).unwrap();
    }
    if mode == "metadata" {
        assert!(std::fs::read_dir(fixture.path.join("repo/.git"))
            .unwrap()
            .next()
            .is_none());
        std::fs::remove_dir(fixture.path.join("repo/.git")).unwrap();
        std::fs::rename(
            fixture.path.join("saved-metadata"),
            fixture.path.join("repo/.git"),
        )
        .unwrap();
    }
    let error = result.unwrap_err();
    assert!(!error.error.applied, "{mode}");
    assert!(error.error.record.is_some(), "{mode}");
    assert_eq!(error.parent_record.is_some(), missing, "{mode}");
    assert_eq!(error.created.len(), usize::from(missing), "{mode}");
    let target = fixture.path.join(if mode == "prefix" {
        "saved-prefix/target"
    } else {
        "repo/prefix/target"
    });
    if mode.starts_with("target") {
        assert_eq!(std::fs::read(&target).unwrap(), b"attacker");
    } else if replace {
        assert_eq!(std::fs::read(&target).unwrap(), b"target-before");
    } else {
        assert!(!target.exists(), "{mode}");
    }
    std::fs::write(fixture.path.join("substitution-proof.json"), serde_json::to_vec(&serde_json::json!({"case":mode,"replace":replace,"createdParent":missing,"phase":if final_slot {"finalSlot"} else {"delayedRefresh"},"fileRecord":error.error.record,"parentRecord":error.parent_record,"applied":error.error.applied,"durableCreatedCount":error.created.len(),"attackerBytesRetained":true,"sourceAndSentinelPreserved":true})).unwrap()).unwrap();
}
#[test]
fn delayed_refresh_native_substitutions_refuse_before_create_or_replace() {
    for (replace, missing) in [(false, false), (true, false), (false, true)] {
        for mode in [
            "targetBytes",
            "targetIdentity",
            "stageBytes",
            "stageIdentity",
            "stageDirectory",
            "root",
            "metadata",
            "prefix",
            "lock",
            "namespace",
        ] {
            check(mode, replace, missing, false);
        }
    }
}
#[test]
fn final_slot_native_substitutions_refuse_before_create_or_replace() {
    for (replace, missing) in [(false, false), (true, false), (false, true)] {
        for mode in [
            "targetBytes",
            "targetIdentity",
            "stageBytes",
            "stageIdentity",
            "stageDirectory",
            "root",
            "metadata",
            "prefix",
            "lock",
            "namespace",
        ] {
            check(mode, replace, missing, true);
        }
    }
}
#[test]
fn final_slot_caller_revocation_refuses_before_create_or_replace() {
    for replace in [false, true] {
        let fixture = Fixture::new();
        if replace {
            std::fs::write(fixture.path.join("repo/target"), b"target-before").unwrap();
        }
        let journal = fixture.journal();
        let expected = fixture
            .root()
            .parent("target", false)
            .unwrap()
            .snapshot()
            .unwrap();
        let mut authority = Authority::default();
        let revoked = authority.revoked.clone();
        parents::binding::FINAL_HOOK.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || {
                revoked.store(true, Ordering::Release);
                Ok(())
            }))
        });
        let error = journal
            .replace_with_parents(&fixture.plan("target"), &expected, b"after", &mut authority)
            .unwrap_err();
        parents::binding::FINAL_HOOK.with(|slot| *slot.borrow_mut() = None);
        assert!(!error.error.applied && error.error.record.is_some());
        if replace {
            assert_eq!(
                std::fs::read(fixture.path.join("repo/target")).unwrap(),
                b"target-before"
            );
        } else {
            assert!(!fixture.path.join("repo/target").exists());
        }
    }
}
