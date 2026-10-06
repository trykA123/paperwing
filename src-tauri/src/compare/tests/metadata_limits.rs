use super::*;
use std::os::unix::fs::PermissionsExt;

async fn side(fixture: &Fixture, commit: &str, job: &Job) -> Resolved {
    let context = fixture.context(CompareRef::Commit { sha: commit.into() });
    let safe = read_root(&context, job).await.unwrap();
    let config = fixture
        .git(&["config", "--get-regexp", "^diff\\.(algorithm|renamelimit)$"])
        .await;
    let diff_config = decode(&config)
        .unwrap()
        .lines()
        .flat_map(|line| {
            let (key, value) = line.split_once(' ').unwrap();
            ["-c".to_string(), format!("{key}={value}")]
        })
        .collect();
    let object_format = ObjectFormat::read(&context.root, job).await.unwrap();
    let mut resolved = Resolved {
        diff_config,
        reader: git::BatchReader::new(context.root.clone(), job.cancel.clone()),
        context,
        safe,
        commit: commit.into(),
        object_format,
        files: BTreeMap::new(),
    };
    resolved.files = inventory(&resolved, job).await.unwrap();
    resolved
}

#[tokio::test]
async fn long_path_inventory_preserves_metadata_when_raw_headers_would_exceed_capture_limits() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.git(&["config", "core.filemode", "true"]).await;
    fixture
        .git(&["config", "diff.algorithm", "histogram"])
        .await;
    fixture.git(&["config", "diff.renameLimit", "50000"]).await;
    let directory = "d".repeat(220);
    let suffix = "p".repeat(144);
    let paths: Vec<_> = (0..19_000)
        .map(|index| format!("{directory}/{index:06}-{suffix}"))
        .collect();
    for path in &paths {
        fixture.write(path, b"\0\n");
    }
    fixture.stage_paths(paths.clone()).await;
    let base = fixture.commit_staged("base").await;
    for path in &paths {
        std::fs::set_permissions(
            fixture.0.join("repo").join(path),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    let head = fixture.commit("mode changes").await;
    let job = Job {
        context: "long-path-metadata".into(),
        ..fixture.job()
    };
    let left = side(&fixture, &base, &job).await;
    let right = side(&fixture, &head, &job).await;
    let old = legacy::metadata(&left, &right, &job).await.unwrap();
    let new = diff_metadata(&left, &right, &job).await.unwrap();
    assert_eq!(new.lines, old.lines);
    assert!(!new.lines.is_empty());
    assert!(new.renames.is_empty() && old.renames.is_empty());
    assert_eq!(new.reason, old.reason);
}
