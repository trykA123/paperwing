use super::*;
use std::io::Write;

#[derive(Serialize)]
struct ResultData<'a> {
    raw: &'a Summary,
    display: &'a Summary,
    history: &'a History,
    options: &'a Options,
    files: &'a [FileRow],
}

#[tokio::test]
#[ignore]
async fn measure_cold_comparison() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let root = PathBuf::from(crate::env_names::var("SKEIN_COLD_ROOT").unwrap());
    assert!(root.starts_with(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()));
    assert_eq!(
        std::fs::read_to_string(root.join(".skein-disposable")).unwrap(),
        "skein-disposable-fixture-v1\n"
    );
    let engine = crate::env_names::var("SKEIN_COLD_ENGINE").unwrap();
    assert!(["old", "new"].contains(&engine.as_str()));
    let workload = crate::env_names::var("SKEIN_COLD_WORKLOAD").unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let base = manifest["base"].as_str().unwrap();
    let head = manifest["head"].as_str().unwrap();
    let mut roots = [root.join("checkouts/right"), root.join("checkouts/right")];
    let references = match workload.as_str() {
        "refs" => [
            CompareRef::Commit { sha: base.into() },
            CompareRef::Commit { sha: head.into() },
        ],
        "cross" => {
            roots[0] = root.join("checkouts/left");
            [CompareRef::Head, CompareRef::Head]
        }
        "working" => [
            CompareRef::Commit { sha: base.into() },
            CompareRef::WorkingTree,
        ],
        _ => panic!("Unknown measurement workload"),
    };
    let fixture = Fixture::new().await;
    let service = fixture.service();
    let job = fixture.job();
    #[cfg(target_os = "linux")]
    let job = {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let data = fixture
            .diff_data()
            .with_file_name(format!("cold-measure-{}-{nonce}", std::process::id()));
        assert!(!data.exists());
        Job {
            count_root: Some(data.clone()),
            diff: Some(Arc::new(crate::linux_diff::Storage::new(data).unwrap())),
            ..job
        }
    };
    let contexts = std::array::from_fn(|index| Context {
        endpoint: Endpoint {
            set_id: "fixture-set".into(),
            item_id: "fixture-item".into(),
            reference: references[index].clone(),
        },
        root: roots[index].clone(),
        workspace_root: root.join("checkouts"),
    });
    let options = Options {
        normalize_eol: crate::env_names::var("SKEIN_COLD_NORMALIZE_EOL")
            .map(|value| match value.as_str() {
                "true" => true,
                "false" => false,
                _ => panic!("Invalid SKEIN_COLD_NORMALIZE_EOL"),
            })
            .unwrap_or(true),
        ignore_whitespace: false,
    };
    let before = crate::benchmark::benchmark_snapshot().unwrap()["commands"].clone();
    println!("MEASURE_BEGIN");
    std::io::stdout().flush().unwrap();
    let started = std::time::Instant::now();
    let prepared = if engine == "old" {
        legacy::prepare(&service, "cold-measure", 1, contexts, options, &job)
            .await
            .unwrap()
    } else {
        service
            .prepare("cold-measure", 1, contexts, options, &job)
            .await
            .unwrap()
    };
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    let after = crate::benchmark::benchmark_snapshot().unwrap()["commands"].clone();
    let commands: BTreeMap<_, _> = after
        .as_object()
        .unwrap()
        .iter()
        .filter_map(|(name, count)| {
            let count = count.as_u64().unwrap() - before[name].as_u64().unwrap_or(0);
            (count > 0).then_some((name, count))
        })
        .collect();
    println!(
        "MEASURE_END {}",
        serde_json::json!({ "engine": engine, "workload": workload, "timeMs": elapsed, "gitProcesses": commands.values().sum::<u64>(), "commands": commands })
    );
    let data = ResultData {
        raw: &prepared.view.raw,
        display: &prepared.view.display,
        history: &prepared.view.history,
        options: &prepared.view.options,
        files: &prepared.rows,
    };
    let output = crate::env_names::var("SKEIN_COLD_RESULT").unwrap();
    let output = PathBuf::from(output);
    assert!(output.starts_with(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()));
    std::fs::write(output, serde_json::to_vec(&data).unwrap()).unwrap();
    prepared.close_readers().await;
    close_readers(&job.readers).await;
    #[cfg(target_os = "linux")]
    assert!(git::runner_idle());
}
