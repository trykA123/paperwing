use super::snapshot;
use crate::stash::tests::{git_in, read, repo, text};

#[tokio::test]
async fn large_stash_snapshots_use_metadata_and_do_not_hit_the_capture_limit_regression() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let dir = repo();
    let path = text(&dir);
    let bulk = dir.join("bulk");
    std::fs::create_dir(&bulk).unwrap();
    for index in 0..35_000 {
        let name = format!("{index:05}-{}", "x".repeat(234));
        std::fs::write(bulk.join(name), "untracked").unwrap();
    }
    let before = snapshot(&path).await.unwrap();
    std::fs::write(bulk.join("new-file"), "partial apply").unwrap();
    assert_ne!(snapshot(&path).await.unwrap(), before);
    let entries: Vec<_> = crate::git::activity_snapshot()
        .into_iter()
        .map(|entry| serde_json::to_value(entry).unwrap())
        .filter(|entry| entry["context"] == format!("Stash state: {path}"))
        .collect();
    assert!(entries
        .iter()
        .any(|entry| entry["stdoutBytes"].as_u64().unwrap() > 8 * 1024 * 1024));
    assert!(entries.iter().all(|entry| entry["output"]
        .as_array()
        .unwrap()
        .iter()
        .all(|output| output["stream"] == "metadata")));
    assert_eq!(read(&dir, "a.txt"), "one\n");
    git_in(&dir, &["diff", "--exit-code"]);
    std::fs::remove_dir_all(&dir).unwrap();
}
