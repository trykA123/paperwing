use super::*;
use crate::linux_guard::folders::Fixture;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

fn settings(fixture: &Fixture, names: &[&str]) -> crate::settings::Settings {
    crate::settings::Settings {
        sources: vec![],
        workspace: serde_json::json!({"root":fixture.0, "layout":"flat", "sets":[{"id":"set", "items": names.iter().enumerate().map(|(i, name)| serde_json::json!({"id":format!("item-{i}"),"name":name,"url":"https://example.test/repo","ref":{"type":"branch","name":"main"}})).collect::<Vec<_>>()}]}),
    }
}

#[test]
fn registered_folder_trash_writes_info_and_native_restore_preserves_identity() {
    let fixture = Fixture::new("trash-restore-");
    let source = fixture.repository("repo space%é");
    let original = std::fs::metadata(&source).unwrap();
    std::fs::set_permissions(source.join("file"), std::fs::Permissions::from_mode(0o640)).unwrap();
    let data = fixture.0.join("data");
    let result = trash_folders(&settings(&fixture, &["repo space%é"]), "set", &data).unwrap();
    assert_eq!(result[0].state, "trashed");
    assert!(!source.exists());
    let files = data.join("Trash/files");
    let moved = std::fs::read_dir(&files)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let info = data.join("Trash/info").join(format!(
        "{}.trashinfo",
        moved.file_name().unwrap().to_str().unwrap()
    ));
    let text = std::fs::read_to_string(&info).unwrap();
    assert!(text.starts_with(&format!(
        "[Trash Info]\nPath={}\nDeletionDate=",
        encode_path(&source)
    )));
    assert!(text.contains("repo%20space%25%C3%A9"));
    let date = text
        .lines()
        .last()
        .unwrap()
        .strip_prefix("DeletionDate=")
        .unwrap();
    assert_eq!(date.len(), 19);
    for (index, separator) in [(4, '-'), (7, '-'), (10, 'T'), (13, ':'), (16, ':')] {
        assert_eq!(date.as_bytes()[index], separator as u8);
    }
    assert_eq!(std::fs::metadata(&info).unwrap().mode() & 0o777, 0o600);
    assert_eq!(std::fs::metadata(&moved).unwrap().ino(), original.ino());
    Directory::open(&moved)
        .unwrap()
        .move_to(&Directory::open(&fixture.0).unwrap(), "repo space%é")
        .unwrap();
    assert_eq!(std::fs::metadata(&source).unwrap().ino(), original.ino());
    assert_eq!(
        std::fs::read(source.join("file")).unwrap(),
        b"restorable data"
    );
    assert_eq!(
        std::fs::metadata(source.join("file")).unwrap().mode() & 0o777,
        0o640
    );
    assert!(info.exists());
}

#[test]
fn shared_unregistered_and_ineligible_folders_stay_on_disk() {
    let fixture = Fixture::new("trash-admission-");
    for name in ["shared", "eligible", "unregistered"] {
        fixture.repository(name);
    }
    std::fs::create_dir(fixture.0.join("plain")).unwrap();
    let mut saved = settings(&fixture, &["shared", "eligible", "plain"]);
    let shared = saved.workspace["sets"][0]["items"][0].clone();
    saved.workspace["sets"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"id":"other", "items":[shared]}));
    let outcomes = trash_folders(&saved, "set", &fixture.0.join("data")).unwrap();
    assert_eq!(
        outcomes
            .iter()
            .map(|outcome| outcome.state.as_str())
            .collect::<Vec<_>>(),
        ["skipped", "trashed", "skipped"]
    );
    for name in ["shared", "unregistered", "plain"] {
        assert!(fixture.0.join(name).exists());
    }
    assert!(trash_folders(&saved, "unknown", &fixture.0.join("data")).is_err());
}

#[test]
fn pre_rename_failures_remove_info_and_retain_source_data() {
    for change in ["root", "metadata", "parent"] {
        let fixture = Fixture::new("trash-conflict-");
        let path = fixture.repository("repo");
        let source = Directory::open(&path).unwrap();
        let root = Root::open(&path, &[path.join(".git/config")]).unwrap();
        let trash = Trash::for_source(&source, &fixture.0.join("data")).unwrap();
        match change {
            "root" => {
                std::fs::rename(&path, fixture.0.join("saved")).unwrap();
                std::fs::create_dir(&path).unwrap();
            }
            "metadata" => {
                std::fs::rename(path.join(".git"), path.join("saved-git")).unwrap();
                std::fs::create_dir(path.join(".git")).unwrap();
            }
            _ => {
                std::fs::rename(&fixture.0, fixture.0.with_extension("saved")).unwrap();
                std::fs::create_dir(&fixture.0).unwrap();
            }
        }
        assert!(trash.recycle(&source, &root).is_err());
        let retained = if change == "parent" {
            fixture.0.with_extension("saved").join("repo")
        } else if change == "root" {
            fixture.0.join("saved")
        } else {
            path
        };
        assert_eq!(
            std::fs::read(retained.join("file")).unwrap(),
            b"restorable data"
        );
        let data = if change == "parent" {
            fixture.0.with_extension("saved").join("data")
        } else {
            fixture.0.join("data")
        };
        assert_eq!(
            std::fs::read_dir(data.join("Trash/info")).unwrap().count(),
            0
        );
        if change == "parent" {
            std::fs::remove_dir(&fixture.0).unwrap();
            std::fs::rename(fixture.0.with_extension("saved"), &fixture.0).unwrap();
        }
    }
}

#[test]
fn cross_device_and_name_collisions_never_replace_or_copy_source_data() {
    let fixture = Fixture::new("trash-cross-device-");
    let path = fixture.repository("repo");
    let source = Directory::open(&path).unwrap();
    let cross = crate::platform::Fixture::new("trash-cross-device");
    let target = Directory::open(&cross.0).unwrap();
    assert!(!source.same_mount(&target));
    assert!(source
        .move_to(&target, "repo")
        .unwrap_err()
        .to_string()
        .contains("Cross-device"));
    assert!(!cross.0.join("repo").exists());
    assert_eq!(
        std::fs::read(path.join("file")).unwrap(),
        b"restorable data"
    );
    let cross_path = cross.0.clone();
    drop(target);
    drop(cross);
    assert!(!cross_path.exists());
    let target = Directory::open(&fixture.0).unwrap();
    std::fs::create_dir(fixture.0.join("collision")).unwrap();
    std::fs::write(fixture.0.join("collision/sentinel"), b"keep").unwrap();
    assert_eq!(
        source.move_to(&target, "collision").unwrap_err().code,
        Some(libc::EEXIST)
    );
    assert_eq!(
        std::fs::read(fixture.0.join("collision/sentinel")).unwrap(),
        b"keep"
    );
    assert_eq!(
        std::fs::read(path.join("file")).unwrap(),
        b"restorable data"
    );
}

#[test]
fn unsafe_trash_privacy_and_symlinks_leave_registered_sources_unchanged() {
    for linked in [false, true] {
        let fixture = Fixture::new("trash-storage-");
        let source = fixture.repository("repo");
        let data = fixture.0.join("data");
        std::fs::create_dir(&data).unwrap();
        if linked {
            std::os::unix::fs::symlink(&fixture.0, data.join("Trash")).unwrap();
        } else {
            std::fs::create_dir(data.join("Trash")).unwrap();
            std::fs::set_permissions(data.join("Trash"), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        let outcomes = trash_folders(&settings(&fixture, &["repo"]), "set", &data).unwrap();
        assert_eq!(outcomes[0].state, "failed");
        assert_eq!(
            std::fs::read(source.join("file")).unwrap(),
            b"restorable data"
        );
    }
}

#[test]
fn volume_trash_uses_sticky_shared_storage_or_private_fallback_and_relative_info() {
    for mode in ["absent", "sticky", "unsafe", "linked"] {
        let fixture = Fixture::new("trash-volume-");
        let path = fixture.repository("repo");
        let source = Directory::open(&path).unwrap();
        let root = Root::open(&path, &[path.join(".git/config")]).unwrap();
        if mode == "linked" {
            std::os::unix::fs::symlink(&fixture.0, fixture.0.join(".Trash")).unwrap();
        }
        if mode == "sticky" || mode == "unsafe" {
            std::fs::create_dir(fixture.0.join(".Trash")).unwrap();
            std::fs::set_permissions(
                fixture.0.join(".Trash"),
                std::fs::Permissions::from_mode(if mode == "sticky" { 0o1777 } else { 0o777 }),
            )
            .unwrap();
        }
        let top = Directory::open(&fixture.0).unwrap();
        let trash = Trash::open_volume(&top).unwrap();
        let uid = std::fs::metadata(&fixture.0).unwrap().uid();
        assert_eq!(
            trash.directory.path,
            if mode == "sticky" {
                fixture.0.join(".Trash").join(uid.to_string())
            } else {
                fixture.0.join(format!(".Trash-{uid}"))
            }
        );
        trash.recycle(&source, &root).unwrap();
        let payload = std::fs::read_dir(&trash.files.path)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let info = trash.info.path.join(format!(
            "{}.trashinfo",
            payload.file_name().unwrap().to_str().unwrap()
        ));
        assert!(std::fs::read_to_string(info)
            .unwrap()
            .contains("\nPath=repo\n"));
        Directory::open(&payload)
            .unwrap()
            .move_to(&top, "repo")
            .unwrap();
        assert_eq!(
            std::fs::read(path.join("file")).unwrap(),
            b"restorable data"
        );
    }
}

#[test]
fn malformed_registration_keeps_every_folder_and_refuses_an_empty_success() {
    let fixture = Fixture::new("trash-invalid-registration-");
    let path = fixture.repository("repo");
    let mut saved = settings(&fixture, &["repo"]);
    saved.workspace["sets"][0]["items"][0]["name"] = serde_json::json!("../repo");
    assert!(trash_folders(&saved, "set", &fixture.0.join("data")).is_err());
    assert_eq!(
        std::fs::read(path.join("file")).unwrap(),
        b"restorable data"
    );
    assert!(!fixture.0.join("data").exists());
}

#[test]
fn rename_collision_discards_info_and_retries_without_overwriting() {
    let fixture = Fixture::new("trash-rename-collision-");
    let path = fixture.repository("repo");
    let source = Directory::open(&path).unwrap();
    let root = Root::open(&path, &[path.join(".git/config")]).unwrap();
    let trash = Trash::for_source(&source, &fixture.0.join("data")).unwrap();
    let mut collision = None;
    let mut attempts = 0;
    trash
        .recycle_with(&source, &root, |target, name, renamed| {
            attempts += 1;
            if attempts == 1 {
                let path = target.path.join(name);
                std::fs::create_dir(&path).unwrap();
                std::fs::write(path.join("sentinel"), b"keep").unwrap();
                collision = Some(name.to_string());
            }
            source.move_to_tracked(target, name, renamed)
        })
        .unwrap();
    assert_eq!(attempts, 2);
    let collision = collision.unwrap();
    assert!(!trash
        .info
        .path
        .join(format!("{collision}.trashinfo"))
        .exists());
    assert_eq!(std::fs::read_dir(&trash.info.path).unwrap().count(), 1);
    assert_eq!(
        std::fs::read(trash.files.path.join(collision).join("sentinel")).unwrap(),
        b"keep"
    );
    assert!(!source.path.exists());
}

#[test]
fn post_rename_failure_keeps_info_and_restorable_payload() {
    let fixture = Fixture::new("trash-after-rename-");
    let path = fixture.repository("repo");
    let source = Directory::open(&path).unwrap();
    let root = Root::open(&path, &[path.join(".git/config")]).unwrap();
    let trash = Trash::for_source(&source, &fixture.0.join("data")).unwrap();
    let mut payload = None;
    assert!(trash
        .recycle_with(&source, &root, |target, name, renamed| {
            payload = Some(source.move_to_tracked(target, name, renamed)?);
            Err(crate::linux_guard::Error::io(rustix::io::Errno::IO))
        })
        .is_err());
    let payload = payload.unwrap();
    assert!(!source.path.exists());
    assert_eq!(std::fs::read_dir(&trash.info.path).unwrap().count(), 1);
    assert_eq!(
        std::fs::read(payload.join("file")).unwrap(),
        b"restorable data"
    );
    Directory::open(&payload)
        .unwrap()
        .move_to(&Directory::open(&fixture.0).unwrap(), "repo")
        .unwrap();
    assert_eq!(
        std::fs::read(path.join("file")).unwrap(),
        b"restorable data"
    );
}
