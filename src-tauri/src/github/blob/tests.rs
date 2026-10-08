use super::*;
use crate::github::compare::fixture::{source, Reply, Server};
use crate::github::http::fixture::Binding;

pub(super) struct Temp(pub(super) PathBuf);
impl Temp {
    pub(super) fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let root = crate::test_support::tmp_root().join(format!(
            "p40-blob-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

pub(super) fn request(source: crate::settings::Source) -> Request {
    let url = format!("https://{}/admin/repo.git", source.host);
    Request::from_url(source, &url, ["a".repeat(40), "b".repeat(40)]).unwrap()
}

#[tokio::test]
async fn two_blob_reads_and_reopened_cache_make_one_raw_request_on_both_hosts() {
    for host in ["github.com", "gitext.company.com"] {
        let source = source(host);
        let server = Server::new(|request, _| {
            assert!(request
                .to_ascii_lowercase()
                .contains("accept: application/vnd.github.raw\r\n"));
            assert!(request.contains("/git/blobs/"));
            Reply {
                status: 200,
                headers: String::new(),
                body: b"text\n".to_vec(),
            }
        })
        .await;
        let _binding = Binding::new(&source.id, &server.base);
        let request = request(source);
        let temp = Temp::new();
        let cache = Cache::new(temp.0.clone());
        assert_eq!(cache.capacity, 200 * 1024 * 1024);
        let sha = "d".repeat(40);
        let (left, right) = tokio::join!(
            cache.read(&request, &sha, None),
            cache.read(&request, &sha, None)
        );
        assert_eq!(left.unwrap().bytes, Some(b"text\n".to_vec()));
        assert_eq!(right.unwrap().bytes, Some(b"text\n".to_vec()));
        let cache = Cache::new(temp.0.clone());
        assert_eq!(
            cache.read(&request, &sha, None).await.unwrap().bytes,
            Some(b"text\n".to_vec())
        );
        assert_eq!(server.count(), 1);
    }
}

#[tokio::test]
async fn large_or_binary_blobs_are_metadata_only_and_disabled_provider_never_writes() {
    let source = source("gitint.company.com");
    let server = Server::new(|request, _| {
        if request.contains(&"d".repeat(40)) {
            return Reply {
                status: 200,
                headers: String::new(),
                body: b"binary\0content".to_vec(),
            };
        }
        Reply {
            status: 200,
            headers: String::new(),
            body: vec![b'x'; MAX_BYTES + 1],
        }
    })
    .await;
    let _binding = Binding::new(&source.id, &server.base);
    let mut request = request(source);
    let temp = Temp::new();
    let cache = Cache::new(temp.0.clone());
    let binary = cache.read(&request, &"d".repeat(40), None).await.unwrap();
    assert!(binary.binary && binary.bytes.is_none());
    assert_eq!(
        cache.read(&request, &"d".repeat(40), None).await.unwrap(),
        binary
    );
    assert!(cache
        .read(&request, &"e".repeat(40), None)
        .await
        .unwrap()
        .bytes
        .is_none());
    assert!(cache
        .read(&request, &"f".repeat(40), Some(MAX_BYTES as u64 + 1))
        .await
        .unwrap()
        .bytes
        .is_none());
    assert_eq!(server.count(), 2);
    request.source.enabled = false;
    let fresh = Cache::new(temp.0.join("disabled"));
    assert!(fresh.read(&request, &"d".repeat(40), None).await.is_err());
    assert!(!fresh.root.exists());
    assert_eq!(server.count(), 2);
}

#[test]
fn capacity_evicts_least_recently_used_blob_and_promotion_retains_hot_blob() {
    let temp = Temp::new();
    let root = temp.0.join("github-blobs");
    let blob = Blob {
        bytes: Some(b"text".to_vec()),
        size: Some(4),
        binary: false,
    };
    let keys = ["a".repeat(64), "b".repeat(64), "c".repeat(64)];
    disk::write(&root, &keys[0], &blob, 10).unwrap();
    disk::write(&root, &keys[1], &blob, 10).unwrap();
    for (index, key) in keys[..2].iter().enumerate() {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join(key))
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(
                std::time::UNIX_EPOCH + std::time::Duration::from_secs(index as u64 + 1),
            ))
            .unwrap();
    }
    assert!(disk::read(&root, &keys[0], 10).unwrap().is_some());
    disk::write(&root, &keys[2], &blob, 10).unwrap();
    assert!(disk::read(&root, &keys[0], 10).unwrap().is_some());
    assert!(disk::read(&root, &keys[1], 10).unwrap().is_none());
    assert!(disk::read(&root, &keys[2], 10).unwrap().is_some());
}

#[test]
fn abandoned_cache_writes_are_removed_and_existing_files_still_obey_the_cap() {
    let temp = Temp::new();
    let root = temp.0.join("github-blobs");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("pending-1-1"), b"abandoned").unwrap();
    let blob = Blob {
        bytes: Some(b"text".to_vec()),
        size: Some(4),
        binary: false,
    };
    let key = "d".repeat(64);
    disk::write(&root, &key, &blob, 10).unwrap();
    assert!(!root.join("pending-1-1").exists());
    disk::write(&root, &key, &blob, 0).unwrap();
    assert!(!root.join(key).exists());
}
