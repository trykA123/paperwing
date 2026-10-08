use super::*;
use crate::github::compare::fixture::source;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::oneshot;

fn hooks() -> &'static Mutex<HashMap<PathBuf, oneshot::Sender<()>>> {
    static HOOKS: OnceLock<Mutex<HashMap<PathBuf, oneshot::Sender<()>>>> = OnceLock::new();
    HOOKS.get_or_init(Mutex::default)
}

pub(super) fn waiting(root: &std::path::Path) {
    if let Some(sender) = hooks().lock().unwrap().remove(root) {
        let _ = sender.send(());
    }
}

async fn contended_cache(write: bool) {
    let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
    let temp = super::tests::Temp::new();
    let request = super::tests::request(source("github.com"));
    let cache = Arc::new(Cache::new(temp.0.clone()));
    let revision = crate::credentials::metadata_revision(&request.source).unwrap();
    let sha = "d".repeat(40);
    let key = super::key(&request, &sha, revision);
    let blob = Blob {
        bytes: Some(b"old revision\n".to_vec()),
        size: Some(13),
        binary: false,
    };
    if !write {
        disk::write(&cache.root, &key, &blob, cache.capacity).unwrap();
    }
    std::fs::create_dir_all(&cache.root).unwrap();
    let blocker = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(cache.root.join("cache.lock"))
        .unwrap();
    blocker.lock().unwrap();
    let (started, waiting) = oneshot::channel();
    hooks().lock().unwrap().insert(cache.root.clone(), started);
    let operation = tokio::spawn({
        let cache = cache.clone();
        let request = request.clone();
        let key = key.clone();
        async move {
            if write {
                cache
                    .save(&request, &key, revision, &blob)
                    .await
                    .map(|_| None)
            } else {
                cache.read(&request, &sha, None).await.map(Some)
            }
        }
    });
    let reached_lock = tokio::time::timeout(Duration::from_secs(5), waiting).await;
    let id = request.source.id.clone();
    let mut changing = tauri::async_runtime::spawn_blocking(move || {
        let before = crate::credentials::revision(&id);
        crate::credentials::advance_revision(&id, || ()) > before
    });
    let changed_before_release = tokio::time::timeout(Duration::from_secs(5), &mut changing).await;
    drop(blocker);
    let finished_before_release = changed_before_release.is_ok();
    let revision_result = match changed_before_release {
        Ok(result) => result,
        Err(_) => changing.await,
    };
    let operation_result = operation.await;
    hooks().lock().unwrap().remove(&cache.root);
    let changed = revision_result.unwrap() && finished_before_release;
    let outcome = operation_result.unwrap();
    assert!(
        matches!(reached_lock, Ok(Ok(()))),
        "cache operation never reached the held file lock"
    );
    assert!(
        changed,
        "cache file lock blocked credential revision checks and changes"
    );
    assert!(
        outcome.is_err(),
        "a stale cache operation succeeded after credentials changed"
    );
    if write {
        assert!(!cache.root.join(key).exists(), "stale blob was published");
    }
}

#[tokio::test]
async fn contended_blob_read_does_not_block_revisions_or_return_stale_content() {
    contended_cache(false).await;
}

#[tokio::test]
async fn contended_blob_write_does_not_block_revisions_or_publish_stale_content() {
    contended_cache(true).await;
}
