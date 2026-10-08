use super::recovery_controls::Authority;
use super::*;
use crate::linux_guard::{mutation::Snapshot, root::RootValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Case {
    ProofWithDirectory,
    ProofOnly,
    LaterEditWithDirectory,
    LaterEditProofOnly,
}
impl Case {
    fn later(self) -> bool {
        matches!(
            self,
            Self::LaterEditWithDirectory | Self::LaterEditProofOnly
        )
    }
    fn directory(self) -> bool {
        matches!(
            self,
            Self::ProofWithDirectory | Self::LaterEditWithDirectory
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Metadata {
    device: u64,
    inode: u64,
    uid: u32,
    gid: u32,
    mode: u32,
    links: u64,
}
impl Metadata {
    fn capture(path: &Path) -> Self {
        let stat = std::fs::symlink_metadata(path).unwrap();
        Self {
            device: stat.dev(),
            inode: stat.ino(),
            uid: stat.uid(),
            gid: stat.gid(),
            mode: stat.mode(),
            links: stat.nlink(),
        }
    }
    fn security(&self) -> (u32, u32, u32) {
        (self.uid, self.gid, self.mode)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Kind {
    File,
    Directory,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Native {
    metadata: Metadata,
    kind: Kind,
    bytes: Option<Vec<u8>>,
    sha256: Option<String>,
    children: BTreeMap<String, Native>,
}
fn capture(path: &Path) -> Native {
    let stat = std::fs::symlink_metadata(path).unwrap();
    assert!(stat.is_file() || stat.is_dir());
    let bytes = stat.is_file().then(|| std::fs::read(path).unwrap());
    let children = if stat.is_dir() {
        std::fs::read_dir(path)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry.file_name().into_string().unwrap(),
                    capture(&entry.path()),
                )
            })
            .collect()
    } else {
        assert_eq!(stat.nlink(), 1);
        BTreeMap::new()
    };
    Native {
        metadata: Metadata::capture(path),
        kind: if stat.is_file() {
            Kind::File
        } else {
            Kind::Directory
        },
        sha256: bytes.as_ref().map(|bytes| hash(bytes)),
        bytes,
        children,
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    schema: String,
    version: u32,
    fixture: PathBuf,
    root: PathBuf,
    data: PathBuf,
    row: Case,
    root_value: RootValue,
    root_security: Metadata,
    create_record: String,
    replace_record: String,
    linked_parent: String,
    cleanup_parent: String,
    cleanup_proof: String,
    expected_create: Vec<u8>,
    expected_replace: Vec<u8>,
    before_replace: Native,
    parent_artifacts: BTreeMap<String, Native>,
    destination_parents: BTreeMap<String, Metadata>,
    cleanup_target: Native,
    source: Native,
    sentinel: Native,
}
fn parent_artifacts(namespace: &Path) -> BTreeMap<String, Native> {
    std::fs::read_dir(namespace)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            let name = entry.file_name().into_string().unwrap();
            (name.starts_with("p-") || name.starts_with("parent-cleanup-"))
                .then(|| (name, capture(&entry.path())))
        })
        .collect()
}
fn preserve(manifest: &Manifest) {
    assert_eq!(
        parent_artifacts(&manifest.data.join("linux-recovery-v1")),
        manifest.parent_artifacts
    );
    assert_eq!(
        capture(&manifest.fixture.join("outside")),
        manifest.sentinel
    );
    assert_eq!(capture(&manifest.root.join("source")), manifest.source);
    assert_eq!(Metadata::capture(&manifest.root), manifest.root_security);
    assert_eq!(
        capture(&manifest.root.join("cleanup/target")),
        manifest.cleanup_target
    );
    for (name, metadata) in &manifest.destination_parents {
        assert_eq!(Metadata::capture(&manifest.root.join(name)), *metadata);
    }
}
pub(super) fn produce() {
    let row: Case = serde_json::from_value(
        crate::env_names::var("SKEIN_PARENT_COMPAT_CASE")
            .unwrap()
            .into(),
    )
    .unwrap();
    let path = PathBuf::from(crate::env_names::var("SKEIN_PARENT_COMPAT_FIXTURE").unwrap());
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../.skillify/evidence/skein/14/a2/compatibility/fixtures")
        .canonicalize()
        .unwrap();
    assert!(path.is_absolute() && path.parent().unwrap() == base && !path.exists());
    std::fs::create_dir(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(
        path.join(".skein-parent-compatibility-fixture"),
        b"skein-parent-compatibility-v1\n",
    )
    .unwrap();
    for name in ["repo", "repo/.git", "data"] {
        std::fs::create_dir(path.join(name)).unwrap();
        std::fs::set_permissions(path.join(name), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    assert_eq!(
        rustix::fs::statfs(&path).unwrap().f_type,
        libc::EXT4_SUPER_MAGIC
    );
    std::fs::write(path.join("repo/file"), b"before").unwrap();
    std::fs::set_permissions(
        path.join("repo/file"),
        std::fs::Permissions::from_mode(0o640),
    )
    .unwrap();
    std::fs::write(path.join("repo/source"), b"preserved-source").unwrap();
    std::fs::write(path.join("outside"), b"outside-sentinel").unwrap();
    let before_replace = capture(&path.join("repo/file"));
    let source = capture(&path.join("repo/source"));
    let sentinel = capture(&path.join("outside"));
    std::fs::write(path.join("restore-backup"), b"before").unwrap();
    std::fs::write(path.join("repo/file"), b"restore-drill").unwrap();
    std::fs::write(
        path.join("repo/file"),
        std::fs::read(path.join("restore-backup")).unwrap(),
    )
    .unwrap();
    assert_eq!(capture(&path.join("repo/file")), before_replace);
    assert_eq!(capture(&path.join("repo/source")), source);
    assert_eq!(capture(&path.join("outside")), sentinel);
    std::fs::write(path.join("before.json"), serde_json::to_vec_pretty(&serde_json::json!({"beforeReplace":before_replace,"source":source,"sentinel":sentinel})).unwrap()).unwrap();
    std::fs::write(path.join("restore-proof.json"), serde_json::to_vec_pretty(&serde_json::json!({"exactTargetRestore":true,"sourcePreserved":true,"sentinelPreserved":true})).unwrap()).unwrap();
    let root = Root::open(&path.join("repo"), &[]).unwrap();
    let mut journal = Journal::open_guarded(&path.join("data"), &[root.value().unwrap()]).unwrap();
    let created = journal
        .replace_with_parents(
            &root.preview_parents("nested/create").unwrap(),
            &Snapshot::Missing,
            b"created",
            &mut Authority,
        )
        .unwrap();
    let expected = root.parent("file", false).unwrap().snapshot().unwrap();
    let replaced = journal
        .replace_with_parents(
            &root.preview_parents("file").unwrap(),
            &expected,
            b"after",
            &mut Authority,
        )
        .unwrap();
    assert!(replaced.parent_record.is_none());
    let cleanup = journal
        .replace_with_parents(
            &root.preview_parents("cleanup/target").unwrap(),
            &Snapshot::Missing,
            b"cleanup",
            &mut Authority,
        )
        .unwrap();
    let linked_parent = created.parent_record.unwrap();
    let cleanup_parent = cleanup.parent_record.unwrap();
    assert_ne!(linked_parent, cleanup_parent);
    for id in [&linked_parent, &cleanup_parent] {
        let loaded = journal.load_parent(id).unwrap();
        loaded.intent.validate().unwrap();
        assert!(matches!(loaded.revision.state, ParentState::Linked { .. }));
        assert!(loaded.directory.native_size().unwrap() <= 4096);
    }
    journal.fault = Some(if row.directory() {
        "parentCleanupPrepared"
    } else {
        "parentCleanupDirectory"
    });
    let result = journal.cleanup_parent(&cleanup_parent, true);
    if row.directory() {
        assert!(result.is_err());
    } else {
        assert!(!result.unwrap().complete);
    }
    journal.fault = None;
    let cleanup_proof = format!("parent-cleanup-{cleanup_parent}.json");
    let encoded = journal
        .directory
        .file(&cleanup_proof, false)
        .unwrap()
        .read(LIMIT)
        .unwrap();
    let proof: parents::record::ParentCleanup = parent_decode(&encoded).unwrap();
    proof.validate().unwrap();
    assert_eq!(proof.id, cleanup_parent);
    assert_eq!(
        journal.directory.path().join(&cleanup_parent).is_dir(),
        row.directory()
    );
    let row_proof = journal
        .list_parents()
        .unwrap()
        .into_iter()
        .find(|row| row.id == cleanup_parent)
        .unwrap();
    assert_eq!(
        row_proof.stage,
        parents::ParentRecoveryStage::CleanupPending
    );
    assert_eq!(journal.parent_usage().unwrap().records, 2);
    if row.later() {
        std::fs::write(path.join("repo/nested/create"), b"later-created").unwrap();
        std::fs::write(path.join("repo/file"), b"later-replaced").unwrap();
        std::fs::set_permissions(
            path.join("repo/nested/create"),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        std::fs::set_permissions(
            path.join("repo/file"),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
    }
    let manifest = Manifest {
        schema: "skein-parent-compatibility".into(),
        version: 1,
        fixture: path.clone(),
        root: path.join("repo"),
        data: path.join("data"),
        row,
        root_value: root.value().unwrap(),
        root_security: Metadata::capture(&path.join("repo")),
        create_record: created.file.record,
        replace_record: replaced.file.record,
        linked_parent,
        cleanup_parent,
        cleanup_proof,
        expected_create: std::fs::read(path.join("repo/nested/create")).unwrap(),
        expected_replace: std::fs::read(path.join("repo/file")).unwrap(),
        before_replace,
        parent_artifacts: parent_artifacts(journal.directory.path()),
        destination_parents: ["nested", "cleanup"]
            .into_iter()
            .map(|name| {
                (
                    name.into(),
                    Metadata::capture(&path.join("repo").join(name)),
                )
            })
            .collect(),
        cleanup_target: capture(&path.join("repo/cleanup/target")),
        source,
        sentinel,
    };
    assert_eq!(
        journal.export(&manifest.create_record, false).unwrap(),
        b"created"
    );
    assert_eq!(
        journal.export(&manifest.replace_record, true).unwrap(),
        b"before"
    );
    assert_eq!(
        journal.export(&manifest.replace_record, false).unwrap(),
        b"after"
    );
    preserve(&manifest);
    drop(journal);
    let reader = Journal::open_existing(&manifest.data).unwrap().unwrap();
    assert_eq!(reader.list_parents().unwrap().len(), 2);
    drop(reader);
    std::fs::write(
        path.join("producer-manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(path.join("producer-result.json"), serde_json::to_vec_pretty(&serde_json::json!({"row":row,"pid":std::process::id(),"actualModernPublication":true,"actualCleanupProof":true,"journalClosed":true,"completeParentArtifacts":manifest.parent_artifacts,"source":manifest.source,"sentinel":manifest.sentinel})).unwrap()).unwrap();
}
