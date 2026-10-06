use super::*;
use std::path::PathBuf;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.skillify/evidence/paperwing/12/native")
            .join(format!("fixture-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(path.join(".paperwing-guard-fixture"), b"paperwing-guard-fixture-v1\n").unwrap();
        std::fs::create_dir(path.join("repo")).unwrap();
        std::fs::create_dir(path.join("repo/.git")).unwrap();
        std::fs::write(path.join("repo/file"), b"before").unwrap();
        std::fs::write(path.join("outside"), b"outside-sentinel").unwrap();
        let backup = std::fs::read(path.join("repo/file")).unwrap();
        std::fs::write(path.join("repo/file"), b"restore-drill").unwrap();
        std::fs::write(path.join("repo/file"), &backup).unwrap();
        assert_eq!(std::fs::read(path.join("repo/file")).unwrap(), backup);
        Self(path.canonicalize().unwrap())
    }
    fn root(&self) -> Root { Root::open(&self.0.join("repo"), &[]).unwrap() }
}
impl Drop for Fixture {
    fn drop(&mut self) { assert_eq!(std::fs::read(self.0.join("outside")).unwrap(), b"outside-sentinel"); }
}

#[test]
fn confined_reads_refuse_links_metadata_aliases_and_changed_roots() {
    let fixture = Fixture::new(); let root = fixture.root();
    assert_eq!(root.read("file", 32).unwrap().unwrap().bytes, b"before");
    assert!(root.read("../outside", 32).is_err());
    assert!(root.read(".git/config", 32).is_err());
    std::fs::rename(fixture.0.join("repo/file"), fixture.0.join("repo/original")).unwrap();
    symlink(fixture.0.join("outside"), fixture.0.join("repo/file")).unwrap();
    assert_eq!(std::fs::read(fixture.0.join("repo/file")).unwrap(), b"outside-sentinel");
    assert!(root.read("file", 32).is_err());
    std::fs::rename(fixture.0.join("repo"), fixture.0.join("moved")).unwrap();
    std::fs::create_dir(fixture.0.join("repo")).unwrap();
    assert!(root.revalidate().is_err());
}

#[test]
fn guarded_replace_create_metadata_and_stage_cleanup() {
    let fixture = Fixture::new(); let root = fixture.root();
    root.probe_write().unwrap();
    let path=fixture.0.join("repo/file");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    rustix::fs::setxattr(&path,"user.paperwing-fixture",b"preserved",rustix::fs::XattrFlags::empty()).unwrap();
    let parent = root.parent("file", false).unwrap(); let before=parent.snapshot().unwrap();
    let stage=parent.stage(b"after", &before).unwrap();
    let published=parent.publish(stage,&before).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(),b"after");
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,0o640);
    assert!(matches!(parent.snapshot().unwrap(), Snapshot::Regular{identity,..} if identity==published.identity));
    let mut value=[0;32];let count=rustix::fs::getxattr(&path,"user.paperwing-fixture",&mut value).unwrap();
    assert_eq!(&value[..count],b"preserved");
    assert!(std::fs::read_dir(fixture.0.join("repo")).unwrap().all(|entry| !entry.unwrap().file_name().to_string_lossy().starts_with(".paperwing-stage-")));
    let new=root.parent("nested/new",true).unwrap();let missing=new.snapshot().unwrap();
    assert_eq!(missing,Snapshot::Missing);new.publish(new.stage(b"new",&missing).unwrap(),&missing).unwrap();
    let expected=new.snapshot().unwrap();new.remove_created(&expected).unwrap();
    assert!(!fixture.0.join("repo/nested/new").exists());
}

#[test]
fn private_storage_exclusive_files_and_lock_lifetime() {
    let fixture=Fixture::new();let directory=PrivateDir::open(&fixture.0,"private").unwrap();
    let file=directory.file("backup",true).unwrap();file.write(b"backup").unwrap();
    assert_eq!(file.read(32).unwrap(),b"backup");assert!(directory.file("backup",true).is_err());
    let lock=directory.lock().unwrap();lock.revalidate().unwrap();assert!(directory.lock().is_err());drop(lock);directory.lock().unwrap();
    assert_eq!(std::fs::metadata(fixture.0.join("private")).unwrap().permissions().mode() & 0o777,0o700);
    assert_eq!(std::fs::metadata(fixture.0.join("private/backup")).unwrap().permissions().mode() & 0o777,0o600);
}

fn writer(path: &std::path::Path, bytes: &str) {
    let result = std::process::Command::new("python3").arg("-c")
        .arg("import pathlib,sys;pathlib.Path(sys.argv[1]).write_bytes(sys.argv[2].encode())")
        .arg(path).arg(bytes).status().unwrap();
    assert!(result.success());
}

#[test]
fn fresh_validation_preserves_external_edits_and_hardlink_aliases() {
    let fixture=Fixture::new();let root=fixture.root();let parent=root.parent("file",false).unwrap();
    let expected=parent.snapshot().unwrap();let stage=parent.stage(b"ours",&expected).unwrap();
    writer(&fixture.0.join("repo/file"),"external");
    assert!(parent.publish(stage,&expected).is_err());
    assert_eq!(std::fs::read(fixture.0.join("repo/file")).unwrap(),b"external");
    std::fs::hard_link(fixture.0.join("repo/file"),fixture.0.join("repo/alias")).unwrap();
    assert!(parent.snapshot().is_err());
    assert_eq!(std::fs::read(fixture.0.join("repo/alias")).unwrap(),b"external");
    let new=root.parent("new",false).unwrap();let missing=new.snapshot().unwrap();let stage=new.stage(b"ours",&missing).unwrap();
    writer(&fixture.0.join("repo/new"),"raced-create");
    assert!(new.publish(stage,&missing).is_err());
    assert_eq!(std::fs::read(fixture.0.join("repo/new")).unwrap(),b"raced-create");
}

#[test]
fn final_check_races_follow_the_accepted_practical_contract() {
    let fixture=Fixture::new();let root=fixture.root();let parent=root.parent("file",false).unwrap();
    let expected=parent.snapshot().unwrap();let stage=parent.stage(b"ours",&expected).unwrap();
    parent.publish_after_check(stage,&expected,|| {writer(&fixture.0.join("repo/file"),"later-edit");Ok(())}).unwrap();
    assert_eq!(std::fs::read(fixture.0.join("repo/file")).unwrap(),b"ours");
    let new=root.parent("new",false).unwrap();let missing=new.snapshot().unwrap();let stage=new.stage(b"ours",&missing).unwrap();
    let failure=new.publish_after_check(stage,&missing,|| {writer(&fixture.0.join("repo/new"),"later-create");Ok(())}).unwrap_err();
    assert!(!failure.applied);assert_eq!(std::fs::read(fixture.0.join("repo/new")).unwrap(),b"later-create");
    let created=new.snapshot().unwrap();
    new.remove_after_check(&created,|| {writer(&fixture.0.join("repo/new"),"later-edited-created");Ok(())}).unwrap();
    assert!(!fixture.0.join("repo/new").exists());
}

#[test]
fn moved_ancestors_and_metadata_pointer_changes_refuse_mutation() {
    let fixture=Fixture::new();let root=fixture.root();
    std::fs::create_dir(fixture.0.join("repo/folder")).unwrap();
    std::fs::write(fixture.0.join("repo/folder/file"),b"before").unwrap();
    let parent=root.parent("folder/file",false).unwrap();let expected=parent.snapshot().unwrap();let stage=parent.stage(b"ours",&expected).unwrap();
    std::fs::rename(fixture.0.join("repo/folder"),fixture.0.join("moved-folder")).unwrap();
    std::fs::create_dir(fixture.0.join("repo/folder")).unwrap();
    assert!(parent.publish(stage,&expected).is_err());
    assert_eq!(std::fs::read(fixture.0.join("moved-folder/file")).unwrap(),b"before");
    std::fs::write(fixture.0.join("repo/.git/commondir"),b"before-pointer").unwrap();
    let bound=fixture.root();std::fs::write(fixture.0.join("repo/.git/commondir"),b"later-pointer").unwrap();
    assert!(bound.revalidate().is_err());
    assert!(bound.parent("file",false).is_err());
    let refreshed=fixture.root();std::fs::hard_link(fixture.0.join("repo/.git/commondir"),fixture.0.join("repo/metadata-alias")).unwrap();
    assert!(refreshed.read("metadata-alias",64).is_err());
}

#[test]
fn access_acl_roundtrip_preserves_the_kernel_validated_metadata() {
    let fixture=Fixture::new();let path=fixture.0.join("repo/file");
    let mut acl=2u32.to_le_bytes().to_vec();
    for (tag,permission,id) in [(1u16,6u16,u32::MAX),(2,4,unsafe {libc::geteuid()}+1),(4,0,u32::MAX),(16,4,u32::MAX),(32,0,u32::MAX)] {
        acl.extend_from_slice(&tag.to_le_bytes());acl.extend_from_slice(&permission.to_le_bytes());acl.extend_from_slice(&id.to_le_bytes());
    }
    rustix::fs::setxattr(&path,"system.posix_acl_access",&acl,rustix::fs::XattrFlags::empty()).unwrap();
    let parent=fixture.root().parent("file",false).unwrap();let expected=parent.snapshot().unwrap();
    parent.publish(parent.stage(b"after",&expected).unwrap(),&expected).unwrap();
    let mut value=[0u8;256];let length=rustix::fs::getxattr(&path,"system.posix_acl_access",&mut value).unwrap();
    assert_eq!(&value[..length],acl.as_slice());
}

#[test]
fn bounded_private_files_and_handle_ownership_fail_closed() {
    let fixture=Fixture::new();let root=fixture.root();
    assert!(root.read("file",2).is_err());
    let depth=std::iter::repeat_n("folder",65).collect::<Vec<_>>().join("/");
    assert!(root.parent(&depth,false).is_err());
    let initial=HANDLES.load(Ordering::Acquire);let clones=(0..100).map(|_|root.clone()).collect::<Vec<_>>();
    assert_eq!(HANDLES.load(Ordering::Acquire),initial);drop(clones);
    let mut permits=Vec::new();while let Ok(permit)=Permit::acquire(){permits.push(permit);}
    assert!(root.read("file",32).is_err());drop(permits);assert_eq!(root.read("file",32).unwrap().unwrap().bytes,b"before");
    let memory=BytePermit::acquire(512*1024*1024).unwrap();assert!(BytePermit::acquire(1).is_err());drop(memory);
    let private=PrivateDir::open(&fixture.0,"private").unwrap();
    std::fs::set_permissions(fixture.0.join("private"),std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(private.file("backup",true).is_err());assert!(!fixture.0.join("private/backup").exists());
}

#[test]
fn isolated_storage_failures_and_mount_crossings() {
    if let Some(root)=std::env::var_os("PAPERWING_GUARD_IO_ROOT") {
        let root=PathBuf::from(root);assert!(root.is_absolute() && root.canonicalize().unwrap()==root);
        assert_eq!(std::fs::read_to_string(root.join(".paperwing-guard-fixture")).unwrap(),"paperwing-guard-fixture-v1\n");
        let mount=root.join("namespace-mount");
        let phase=std::env::var("PAPERWING_GUARD_IO_PHASE").unwrap();
        if phase=="full" {
            std::fs::write(mount.join("readable"),b"readable").unwrap();
            let readonly=Root::open(&mount,&[]).unwrap();assert_eq!(readonly.read("readable",32).unwrap().unwrap().bytes,b"readable");
            assert!(readonly.probe_write().is_err());
            let source=root.join("cross-device-source");std::fs::write(&source,b"retained-source").unwrap();
            let error=rustix::fs::renameat_with(rustix::fs::CWD,&source,rustix::fs::CWD,mount.join("target"),rustix::fs::RenameFlags::NOREPLACE).unwrap_err();
            assert_eq!(error,rustix::io::Errno::XDEV);assert_eq!(std::fs::read(&source).unwrap(),b"retained-source");assert!(!mount.join("target").exists());
            let confined=Root::open(&root,&[]).unwrap();assert!(confined.read("namespace-mount/readable",32).is_err());
            std::fs::write(mount.join("fill"),b"").unwrap();let file=Handle::absolute(&mount.join("fill"),rustix::fs::OFlags::RDWR).unwrap();
            assert_eq!(file.write(&vec![b'x';128*1024]).unwrap_err().code,Some(libc::ENOSPC));
        } else {
            assert_eq!(phase,"readonly");
            let error=Handle::open(rustix::fs::CWD,&mount.join("new"),rustix::fs::OFlags::CREATE|rustix::fs::OFlags::EXCL|rustix::fs::OFlags::RDWR,rustix::fs::Mode::from_raw_mode(0o600),rustix::fs::ResolveFlags::NO_SYMLINKS).unwrap_err();
            assert_eq!(error.code,Some(libc::EROFS));assert!(!mount.join("new").exists());
            assert_eq!(std::fs::read(mount.join("readable")).unwrap(),b"readable");
        }
        return;
    }
    let fixture=Fixture::new();std::fs::create_dir(fixture.0.join("namespace-mount")).unwrap();
    let namespace=std::fs::read_link("/proc/self/ns/mnt").unwrap();
    let script=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scripts/testing/linux-guard-io.py");
    let result=std::process::Command::new("unshare").args(["-Urnm","python3"]).arg(script).arg(&fixture.0)
        .arg(std::env::current_exe().unwrap()).arg(&namespace).output().unwrap();
    assert!(result.status.success(),"{}",String::from_utf8_lossy(&result.stderr));
    assert!(fixture.0.join("io-report.json").exists());
    assert_eq!(std::fs::read_link("/proc/self/ns/mnt").unwrap(),namespace);
}

#[test]
fn published_mutation_failures_preserve_applied_outcome_and_changed_bytes() {
    let fixture=Fixture::new();let root=fixture.root();let parent=root.parent("file",false).unwrap();
    let before=parent.snapshot().unwrap();let stage=parent.stage(b"published",&before).unwrap();
    let fail=|_: &Handle| Err(Error::io(std::io::Error::from_raw_os_error(libc::EIO)));
    let error=parent.publish_with_sync(stage,&before,fail).unwrap_err();
    assert!(error.applied);assert_eq!(error.error.code,Some(libc::EIO));
    assert_eq!(std::fs::read(fixture.0.join("repo/file")).unwrap(),b"published");
    assert!(parent.validate(&before).is_err());
    assert!(std::fs::read_dir(fixture.0.join("repo")).unwrap().all(|entry| !entry.unwrap().file_name().to_string_lossy().starts_with(".paperwing-stage-")));
    let created=root.parent("new",false).unwrap();let missing=created.snapshot().unwrap();
    created.publish(created.stage(b"created",&missing).unwrap(),&missing).unwrap();
    let expected=created.snapshot().unwrap();let error=created.remove_with_sync(&expected,fail).unwrap_err();
    assert!(error.applied);assert_eq!(error.error.code,Some(libc::EIO));
    assert!(!fixture.0.join("repo/new").exists());
}

#[test]
fn private_diff_materialization_cleans_owned_files_and_preserves_substitutions() {
    let fixture=Fixture::new();
    let mut temporary=storage::Temporary::new(&fixture.0).unwrap();let path=temporary.path().unwrap();
    let left=temporary.write("left",b"private-left").unwrap();let right=temporary.write("right",b"private-right").unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,0o700);
    for file in [&left,&right] { assert_eq!(std::fs::metadata(file).unwrap().permissions().mode() & 0o777,0o600); }
    assert_eq!(std::fs::read(&left).unwrap(),b"private-left");drop(temporary);assert!(!path.exists());
    let mut temporary=storage::Temporary::new(&fixture.0).unwrap();let path=temporary.path().unwrap();
    let left=temporary.write("left",b"owned").unwrap();std::fs::rename(&left,path.join("saved-owned")).unwrap();
    std::fs::write(&left,b"substituted").unwrap();drop(temporary);
    assert_eq!(std::fs::read(&left).unwrap(),b"substituted");
    assert_eq!(std::fs::read(path.join("saved-owned")).unwrap(),b"owned");
    let mut temporary=storage::Temporary::new(&fixture.0).unwrap();
    let left=temporary.write("left",b"owned").unwrap();std::fs::write(&left,b"changed").unwrap();drop(temporary);
    assert_eq!(std::fs::read(&left).unwrap(),b"changed");
}

#[test]
fn newly_created_commondir_invalidates_root_reads_parents_and_stages() {
    let fixture=Fixture::new();let root=fixture.root();let parent=root.parent("file",false).unwrap();
    let expected=parent.snapshot().unwrap();let stage=parent.stage(b"ours",&expected).unwrap();
    std::fs::write(fixture.0.join("repo/.git/commondir"),b"../new-common").unwrap();
    assert!(root.revalidate().is_err());assert!(root.probe_write().is_err());
    assert!(root.read("file",32).is_err());assert!(root.parent("file",false).is_err());
    assert!(parent.revalidate().is_err());assert!(parent.publish(stage,&expected).is_err());
    assert_eq!(std::fs::read(fixture.0.join("repo/file")).unwrap(),b"before");
}

#[test]
fn persisted_guard_values_are_lossless_and_reopen_fresh_authority() {
    let identity=Identity {device:(u32::MAX,19),inode:u64::MAX-1,mount:(1u64<<53)+7};
    let encoded=serde_json::to_vec(&identity).unwrap();
    assert_eq!(serde_json::from_slice::<Identity>(&encoded).unwrap(),identity);
    assert!(serde_json::from_str::<Identity>(r#"{"device":[1,2],"inode":3,"mount":4,"extra":true}"#).is_err());
    let fixture=Fixture::new();let root=fixture.root();let value=root.value().unwrap();
    let decoded=serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(Root::reopen(&decoded).unwrap().value().unwrap(),value);
    std::fs::write(fixture.0.join("repo/.git/commondir"),b"../later").unwrap();
    assert!(Root::reopen(&value).is_err());
    let fixture=Fixture::new();std::fs::create_dir(fixture.0.join("repo/nested")).unwrap();
    let root=fixture.root();let parent=root.parent("nested/file",false).unwrap();let ancestors=parent.ancestors().unwrap();
    std::fs::rename(fixture.0.join("repo/nested"),fixture.0.join("old-nested")).unwrap();std::fs::create_dir(fixture.0.join("repo/nested")).unwrap();
    assert!(root.parent("nested/file",false).unwrap().matches_ancestors(&ancestors).is_err());
}

#[test]
fn private_existing_lookup_and_owned_deletion_refuse_unknown_links() {
    let fixture=Fixture::new();let private=PrivateDir::open(&fixture.0,"private").unwrap();
    assert!(private.lookup("missing").is_err());assert!(!fixture.0.join("private/missing").exists());
    let child=private.create_child("record").unwrap();child.write_new("backup",b"verified").unwrap();
    let entries=child.entries(4).unwrap();assert_eq!(entries.len(),1);
    child.unlink_owned("backup",&entries[0].identity,b"verified").unwrap();
    private.remove_empty("record",&child.identity().unwrap()).unwrap();
    symlink(fixture.0.join("outside"),fixture.0.join("private/unknown")).unwrap();assert!(private.entries(4).is_err());
    assert_eq!(std::fs::read(fixture.0.join("outside")).unwrap(),b"outside-sentinel");
}

#[test]
fn persisted_security_rejects_unbounded_or_unsupported_values_before_apply() {
    let mut security=metadata::Security::new_file();security.mode=0o4644;assert!(security.validate().is_err());
    security.mode=0o644;security.attributes.insert("security.unknown".into(),vec![1]);assert!(security.validate().is_err());
    security.attributes.clear();security.attributes.insert("user.large".into(),vec![0;65537]);assert!(security.validate().is_err());
    security.attributes.clear();security.attributes.insert("user.valid".into(),vec![255;65536]);security.validate().unwrap();
    let fixture=Fixture::new();let parent=fixture.root().parent("file",false).unwrap();let expected=parent.snapshot().unwrap();
    let mut security=match &expected {Snapshot::Regular{security,..}=>security.clone(),_=>unreachable!()};security.mode=0o640;
    security.attributes.insert("user.restored".into(),b"restored".to_vec());
    let stage=parent.stage(b"restored",&expected).unwrap();parent.prepare_security(&stage,&security).unwrap();let proof=stage.proof().unwrap();
    parent.publish_restore(stage,&expected,&security).unwrap();
    assert!(matches!(parent.snapshot().unwrap(),Snapshot::Regular{identity,security:actual,..} if identity==proof.file && actual==security));
}
