use crate::benchmark::benchmark_snapshot;
use crate::finder::FinderRequest;
use crate::platform::Fixture;
use crate::search::{RepoTarget, SearchRequest};
use crate::search_engine::Engine;
use std::sync::Arc;

#[tokio::test]
async fn production_search_jobs_export_logical_benchmark_operations() {
    let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("search-benchmark");
    let root = fixture.0.join("repo");
    std::fs::create_dir(&root).unwrap();
    for args in [
        &["init", "-q"][..],
        &["config", "core.autocrlf", "false"][..],
    ] {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success());
    }
    std::fs::write(root.join("needle.rs"), b"needle\n").unwrap();
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["add", "."])
        .status()
        .unwrap()
        .success());
    let path = root.to_str().unwrap().to_string();
    let before = benchmark_snapshot().unwrap();
    let code = crate::search_job::run_with_engine(
        SearchRequest {
            pattern: "needle".into(),
            repos: vec![RepoTarget {
                path: path.clone(),
                git_ref: None,
            }],
            ..SearchRequest::default()
        },
        crate::search_job::Job {
            id: 47,
            concurrency: 1,
            cancel: Arc::default(),
            send: Arc::new(|_| {}),
        },
        Engine::BuiltIn,
    )
    .await
    .unwrap();
    assert_eq!(code.matches, 1);
    let files = crate::finder_job::run_job(
        FinderRequest {
            repos: vec![path],
            query: "needle".into(),
            ..FinderRequest::default()
        },
        crate::finder_job::Job {
            id: 48,
            cancel: Arc::default(),
            send: Arc::new(|_| {}),
            options: crate::settings::SearchOptions::default(),
        },
    )
    .await
    .unwrap();
    assert_eq!(files.matches, 1);
    let after = benchmark_snapshot().unwrap();
    for key in ["ipc.content/search-code", "ipc.files/find-file"] {
        let count = |snapshot: &serde_json::Value| {
            snapshot["aggregates"][key]["count"].as_u64().unwrap_or(0)
        };
        assert!(count(&after) > count(&before));
    }
    for operation in ["search-code", "find-file"] {
        assert!(after["events"].as_array().unwrap().iter().any(|event| {
            event["operation"] == operation && event["durationMs"].as_f64().unwrap() >= 0.0
        }));
        assert!(after["commands"][operation].is_null());
    }
}
