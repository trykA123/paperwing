use super::*;
use crate::platform::Fixture;
use std::path::Path;
use std::time::{Duration, Instant};

fn git(root: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repo(fixture: &Fixture, name: &str, paths: &[&str]) -> String {
    let root = fixture.0.join(name);
    std::fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["config", "core.autocrlf", "false"]);
    for relative in paths {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }
    git(&root, &["add", "."]);
    root.to_str().unwrap().into()
}

async fn recorded(
    request: FinderRequest,
    options: SearchOptions,
    cancel_on_first: bool,
) -> (Summary, Vec<Outbound>) {
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let sink = recorded.clone();
    let cancel = Arc::new(AtomicBool::new(false));
    let stop = cancel.clone();
    let send: Emit = Arc::new(move |event| {
        if cancel_on_first && matches!(event, Outbound::Matches(_)) {
            stop.store(true, Ordering::Relaxed);
        }
        sink.lock().unwrap().push(event);
    });
    let summary = run_job(
        request,
        Job {
            id: 47,
            cancel,
            send,
            options,
        },
    )
    .await
    .unwrap();
    let events = recorded.lock().unwrap().clone();
    (summary, events)
}

fn last(events: &[Outbound]) -> &MatchesPayload {
    events
        .iter()
        .rev()
        .find_map(|event| match event {
            Outbound::Matches(payload) => Some(payload),
            _ => None,
        })
        .unwrap()
}

#[tokio::test]
async fn streamed_top_results_are_bounded_sorted_and_deduplicated_across_repositories() {
    let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("finder-ranked");
    let first = repo(
        &fixture,
        "a",
        &["src/main-file.rs", "src/much-further-file.rs", "other.txt"],
    );
    let second = repo(&fixture, "b", &["main-file.rs"]);
    let request = FinderRequest {
        repos: vec![first.clone(), format!("{first}/"), second],
        query: "mf".into(),
        max_results: Some(2),
    };
    let (summary, events) = recorded(request, SearchOptions::default(), false).await;
    assert_eq!((summary.scanned, summary.matches), (4, 3));
    assert!(summary.errors.is_empty());
    let found = last(&events);
    assert_eq!(found.matches.len(), 2);
    assert!(found.matches[0].score >= found.matches[1].score);
    assert!(found.matches.iter().all(|hit| !hit.positions.is_empty()));
    let sequences: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            Outbound::Matches(payload) => Some(payload.sequence),
            _ => None,
        })
        .collect();
    assert!(sequences.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Outbound::Done(_)))
            .count(),
        1
    );
}

#[tokio::test]
async fn finder_settings_control_untracked_files_and_exact_matching() {
    let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("finder-settings");
    let path = repo(&fixture, "a", &["src/my-hit.rs", ".gitignore"]);
    let root = Path::new(&path);
    std::fs::write(root.join(".gitignore"), b"ignored*\n").unwrap();
    for name in [
        "untracked-hit.rs",
        "ignored-hit.rs",
        "ignored-tracked-hit.rs",
    ] {
        std::fs::write(root.join(name), b"").unwrap();
    }
    git(root, &["add", "-f", "ignored-tracked-hit.rs"]);
    let request = FinderRequest {
        repos: vec![path],
        query: "hit".into(),
        ..Default::default()
    };
    let (_, tracked) = recorded(request.clone(), SearchOptions::default(), false).await;
    assert_eq!(last(&tracked).matches.len(), 2);
    let options = SearchOptions {
        search_files: SearchFiles::TrackedAndUntracked,
        finder_matching: FinderMatching::ExactSubstring,
        ..Default::default()
    };
    let (_, untracked) = recorded(request.clone(), options, false).await;
    assert_eq!(last(&untracked).matches.len(), 2);
    assert!(last(&untracked)
        .matches
        .iter()
        .all(|hit| !hit.path.starts_with("ignored")));
    let fuzzy_request = FinderRequest {
        query: "mhr".into(),
        ..request
    };
    let (_, exact) = recorded(fuzzy_request.clone(), options, false).await;
    assert!(last(&exact).matches.is_empty());
    let (_, fuzzy) = recorded(fuzzy_request, SearchOptions::default(), false).await;
    assert_eq!(last(&fuzzy).matches.len(), 1);
}

#[tokio::test]
async fn cancellation_after_the_first_streamed_result_stops_active_finder_work() {
    let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("finder-cancel");
    let names: Vec<_> = (0..1000)
        .map(|index| format!("file-{index:04}.rs"))
        .collect();
    let paths: Vec<_> = names.iter().map(String::as_str).collect();
    let path = repo(&fixture, "a", &paths);
    let (summary, events) = recorded(
        FinderRequest {
            repos: vec![path],
            query: "file".into(),
            ..Default::default()
        },
        SearchOptions::default(),
        true,
    )
    .await;
    assert!(summary.cancelled);
    assert!(summary.scanned < 1000);
    assert!(summary.errors.is_empty());
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Outbound::Done(_)))
            .count(),
        1
    );
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn twenty_thousand_files_stream_first_results_under_150_ms() {
    let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("finder-20k");
    let names: Vec<_> = (0..20_000)
        .map(|index| format!("src/file-{index:05}.rs"))
        .collect();
    let paths: Vec<_> = names.iter().map(String::as_str).collect();
    let path = repo(&fixture, "a", &paths);
    let first = Arc::new(Mutex::new(None));
    let seen = first.clone();
    let latest = Arc::new(Mutex::new(None));
    let results = latest.clone();
    let started = Instant::now();
    let send: Emit = Arc::new(move |event| {
        if let Outbound::Matches(payload) = event {
            if !payload.matches.is_empty() {
                seen.lock()
                    .unwrap()
                    .get_or_insert_with(|| started.elapsed());
            }
            *results.lock().unwrap() = Some(payload);
        }
    });
    let summary = run_job(
        FinderRequest {
            repos: vec![path],
            query: "fi rs".into(),
            ..Default::default()
        },
        Job {
            id: 47,
            cancel: Arc::default(),
            send,
            options: SearchOptions::default(),
        },
    )
    .await
    .unwrap();
    let elapsed = first.lock().unwrap().unwrap();
    eprintln!(
        "20k finder: first={}ms total={}ms scanned={}",
        elapsed.as_secs_f64() * 1000.0,
        started.elapsed().as_secs_f64() * 1000.0,
        summary.scanned
    );
    assert!(
        elapsed < Duration::from_millis(150),
        "first result took {elapsed:?}"
    );
    assert_eq!((summary.scanned, summary.matches), (20_000, 20_000));
    assert!(summary.errors.is_empty());
    assert_eq!(latest.lock().unwrap().as_ref().unwrap().matches.len(), 100);
}
