use super::runner::{execute, execute_inner, activity_snapshot, cancel_activity, clear_activity, ExitObserver, Observer, OutputPolicy, Request, TEST_RUNNER_LOCK};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

pub(super) struct Fixture(pub(super) PathBuf);

impl Fixture {
    pub(super) fn new() -> Self {
        let parent = std::env::var_os("PAPERWING_PROCESS_EVIDENCE").map_or_else(std::env::temp_dir, |path| {
            let path = PathBuf::from(path);
            assert!(path.is_absolute() && path.canonicalize().unwrap() == path);
            assert_eq!(std::fs::read_to_string(path.join(".paperwing-process-evidence")).unwrap(), "paperwing-process-evidence-v1\n");
            path
        });
        let root = parent.join(format!("paperwing-process-{}-{}", std::process::id(), NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir(&root).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(root.join(".paperwing-process-fixture"), "paperwing-process-fixture-v1\n").unwrap();
        Self(root)
    }

    pub(super) fn helper() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/testing/process-fixtures/helper.py")
    }

    fn alias(&self, mode: &str) -> String {
        let quote = |value: &Path| format!("'{}'", value.to_str().unwrap().replace('\'', "'\\''"));
        format!("alias.paperwing-process-fixture=!python3 {} {} {mode}", quote(&Self::helper()), quote(&self.0))
    }

    fn processes(&self) -> Vec<u32> {
        let value: serde_json::Value = serde_json::from_slice(&std::fs::read(self.0.join("pids.json")).unwrap()).unwrap();
        value["processes"].as_array().unwrap().iter().map(|item| item["pid"].as_u64().unwrap() as u32).collect()
    }

    pub(super) async fn gone(&self) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.processes().iter().any(|pid| Path::new("/proc").join(pid.to_string()).exists()) {
            assert!(Instant::now() < deadline, "fixture descendants remain after cleanup");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    fn cleanup(&self) {
        if self.0.join("pids.json").exists() {
            let status = std::process::Command::new("python3").arg(Self::helper()).arg(&self.0).arg("cleanup").status().unwrap();
            assert!(status.success());
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.cleanup();
        if std::env::var_os("PAPERWING_PROCESS_EVIDENCE").is_none() { std::fs::remove_dir_all(&self.0).unwrap(); }
    }
}

#[tokio::test]
async fn unprotected_child_exit_leaves_descendants_and_output_pipes_open() {
    let fixture = Fixture::new();
    let mut child = tokio::process::Command::new("python3").arg(Fixture::helper()).arg(&fixture.0).arg("pipes")
        .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    assert!(child.wait().await.unwrap().success());
    assert!(fixture.processes()[1..].iter().all(|pid| Path::new("/proc").join(pid.to_string()).exists()));
    let mut bytes = Vec::new();
    assert!(tokio::time::timeout(Duration::from_millis(100), stdout.read_to_end(&mut bytes)).await.is_err());
}

#[tokio::test]
async fn linux_descendant_cleanup_closes_pipes_and_retains_completed_mutations() {
    let _guard = TEST_RUNNER_LOCK.lock().await;
    for mode in ["pipes", "mutation"] {
        let fixture = Fixture::new();
        let alias = fixture.alias(mode);
        let start = Instant::now();
        let result = execute(Request { args: &["-c", &alias, "paperwing-process-fixture"], context: "linux-held-pipes",
            timeout: Duration::from_secs(5), expected: &[0], policy: OutputPolicy::Text }, None).await;
        assert!(result.is_ok(), "{}", result.err().unwrap());
        assert_eq!(result.unwrap().code, Some(0));
        assert!(start.elapsed() < Duration::from_secs(1));
        fixture.gone().await;
        if mode == "mutation" { assert_eq!(std::fs::read_to_string(fixture.0.join("mutation-completed")).unwrap(), "completed\n"); }
    }
}

#[tokio::test]
async fn linux_repeated_cancellation_stops_helpers_and_keeps_an_unrelated_process_alive() {
    let _guard = TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new();
    let mut control = tokio::process::Command::new("python3").args(["-c", "import time; time.sleep(60)"])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true).spawn().unwrap();
    let alias = fixture.alias("running");
    let observer: Observer = Arc::new(|_, text| {
        if text != "fixture-ready" { return; }
        let entry = activity_snapshot().into_iter().find(|entry| entry.context == "linux-helper-cancel").unwrap();
        assert!(cancel_activity(entry.id.clone()));
        assert!(cancel_activity(entry.id));
    });
    let result = execute(Request { args: &["-c", &alias, "paperwing-process-fixture"], context: "linux-helper-cancel",
        timeout: Duration::from_secs(5), expected: &[0], policy: OutputPolicy::Text }, Some(observer)).await;
    assert_eq!(result.err().unwrap(), "Git command cancelled");
    fixture.gone().await;
    let entry = activity_snapshot().into_iter().find(|entry| entry.context == "linux-helper-cancel").unwrap();
    assert_eq!(entry.state, "cancelled");
    assert!(!cancel_activity(entry.id));
    assert!(control.try_wait().unwrap().is_none());
    control.kill().await.unwrap();
}

#[tokio::test]
async fn linux_cleanup_retains_registration_and_rejects_canceling_a_completed_action() {
    let _guard = TEST_RUNNER_LOCK.lock().await;
    let after_exit: ExitObserver = Arc::new(|_, code| {
        assert_eq!(code, "0");
        let entry = activity_snapshot().into_iter().find(|entry| entry.context == "linux-cleanup-registration").unwrap();
        assert!(!cancel_activity(entry.id.clone()));
        assert!(clear_activity().retained.contains(&entry.id));
        assert!(!super::runner::resources_idle());
        Ok(())
    });
    let result = execute_inner(Request { args: &["--version"], context: "linux-cleanup-registration", expected: &[0],
        timeout: Duration::from_secs(5), policy: OutputPolicy::Text }, None, None, None, Some(after_exit)).await;
    assert_eq!(result.unwrap().code, Some(0));
    assert!(super::runner::resources_idle());
}

#[tokio::test]
async fn linux_dropped_callers_finish_owned_cleanup_and_publish_a_final_activity() {
    let _guard = TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new();
    let alias = fixture.alias("running");
    let task = tokio::spawn(async move {
        execute(Request { args: &["-c", &alias, "paperwing-process-fixture"], context: "linux-dropped-caller",
            timeout: Duration::from_secs(5), expected: &[0], policy: OutputPolicy::Text }, None).await
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    while !fixture.0.join("pids.json").exists() {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    task.abort();
    assert!(matches!(task.await, Err(error) if error.is_cancelled()));
    let deadline = Instant::now() + Duration::from_secs(1);
    let entry = loop {
        let entry = activity_snapshot().into_iter().find(|entry| entry.context == "linux-dropped-caller").unwrap();
        if entry.state != "running" { break entry; }
        assert!(Instant::now() < deadline, "dropped caller leaves Activity running");
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert_eq!(entry.state, "cancelled");
    assert!(!cancel_activity(entry.id));
    fixture.gone().await;
    assert!(super::runner::resources_idle());
}

#[tokio::test]
async fn linux_timeout_reaps_helpers_and_releases_job_resources() {
    let _guard = TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new();
    let alias = fixture.alias("running");
    let start = Instant::now();
    let result = execute(Request { args: &["-c", &alias, "paperwing-process-fixture"], context: "linux-helper-timeout",
        timeout: Duration::from_millis(500), expected: &[0], policy: OutputPolicy::Text }, None).await;
    assert_eq!(result.err().unwrap(), "Git command timed out");
    assert!(start.elapsed() < Duration::from_secs(2));
    fixture.gone().await;
    let entry = activity_snapshot().into_iter().find(|entry| entry.context == "linux-helper-timeout").unwrap();
    assert_eq!(entry.state, "timedOut");
    assert!(super::runner::resources_idle());
}

#[tokio::test]
async fn linux_detached_helpers_report_failure_and_release_owned_resources() {
    let _guard = TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new();
    let alias = fixture.alias("escape");
    let result = execute(Request { args: &["-c", &alias, "paperwing-process-fixture"], context: "linux-detached-helper",
        timeout: Duration::from_secs(5), expected: &[0], policy: OutputPolicy::Text }, None).await;
    assert!(result.err().unwrap().contains("helper may have detached"));
    let entry = activity_snapshot().into_iter().find(|entry| entry.context == "linux-detached-helper").unwrap();
    assert_eq!(entry.state, "failed");
    assert_eq!(entry.exit_code, Some(0));
    assert!(fixture.processes()[1..].iter().all(|pid| Path::new("/proc").join(pid.to_string()).exists()));
    assert!(super::runner::resources_idle());
    fixture.cleanup();
    fixture.gone().await;
}

#[tokio::test]
async fn linux_observed_completion_wins_against_cancel_and_deadline_during_reap() {
    use super::runner::linux_job::{pause_before_reap, ExitHook};
    let _guard = TEST_RUNNER_LOCK.lock().await;
    for cancel in [true, false] {
        let fixture = Fixture::new();
        let alias = fixture.alias("mutation");
        let (entered, reached) = tokio::sync::oneshot::channel();
        let (release, blocked) = tokio::sync::oneshot::channel();
        pause_before_reap(ExitHook { entered, release: blocked });
        let external = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = external.clone();
        let task = tokio::spawn(async move {
            execute_inner(Request { args: &["-c", &alias, "paperwing-process-fixture"], context: "linux-completed-before-reap",
                timeout: Duration::from_millis(500), expected: &[0], policy: OutputPolicy::Text }, None, Some(flag), None, None).await
        });
        tokio::time::timeout(Duration::from_secs(2), reached).await.unwrap().unwrap();
        let entry = activity_snapshot().into_iter().find(|entry| entry.context == "linux-completed-before-reap" && entry.state == "running").unwrap();
        assert!(!cancel_activity(entry.id));
        if cancel { external.store(true, Ordering::Relaxed); }
        let result = tokio::time::timeout(Duration::from_secs(2), task).await.unwrap().unwrap().unwrap();
        assert_eq!(result.code, Some(0));
        let _ = release.send(());
        assert_eq!(std::fs::read_to_string(fixture.0.join("mutation-completed")).unwrap(), "completed\n");
        fixture.gone().await;
        assert!(super::runner::resources_idle());
    }
}

#[tokio::test]
async fn linux_clear_keeps_registered_cleanup_states_until_the_final_result() {
    let _guard = TEST_RUNNER_LOCK.lock().await;
    for state in ["cancelled", "timedOut"] {
        clear_activity();
        let after_exit: ExitObserver = Arc::new(move |_, _| {
            super::runner::publish_cleanup_state("linux-clear-cleanup", state);
            let entry = activity_snapshot().into_iter().find(|entry| entry.context == "linux-clear-cleanup").unwrap();
            let cleared = clear_activity();
            assert!(cleared.retained.contains(&entry.id));
            assert!(cleared.running.iter().any(|retained| retained.id == entry.id));
            assert!(!super::runner::resources_idle());
            Ok(())
        });
        let result = execute_inner(Request { args: &["--version"], context: "linux-clear-cleanup", expected: &[0],
            timeout: Duration::from_secs(5), policy: OutputPolicy::Text }, None, None, None, Some(after_exit)).await;
        assert_eq!(result.unwrap().code, Some(0));
        let entry = activity_snapshot().into_iter().find(|entry| entry.context == "linux-clear-cleanup").unwrap();
        assert_eq!(entry.state, "completed");
        assert!(super::runner::resources_idle());
    }
}

#[tokio::test]
async fn linux_post_spawn_capture_failure_retains_registration_and_permits_until_reap() {
    use super::runner::linux_job::{inject_capture_failure, CaptureHook};
    let _guard = TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new();
    let alias = fixture.alias("running");
    let (entered, reached) = tokio::sync::oneshot::channel();
    let (release, blocked) = tokio::sync::oneshot::channel();
    inject_capture_failure(CaptureHook { ready: Some(fixture.0.join("pids.json")), entered, release: blocked });
    let task = tokio::spawn(async move {
        execute(Request { args: &["-c", &alias, "paperwing-process-fixture"], context: "linux-capture-failure",
            timeout: Duration::from_secs(5), expected: &[0], policy: OutputPolicy::Text }, None).await
    });
    let pid = tokio::time::timeout(Duration::from_secs(2), reached).await.unwrap().unwrap();
    assert!(Path::new("/proc").join(pid.to_string()).exists());
    let entry = activity_snapshot().into_iter().find(|entry| entry.context == "linux-capture-failure").unwrap();
    assert_eq!(entry.state, "running");
    assert!(clear_activity().retained.contains(&entry.id));
    assert!(!super::runner::resources_idle());
    release.send(()).unwrap();
    assert!(task.await.unwrap().err().unwrap().contains("Could not start Git"));
    assert!(!Path::new("/proc").join(pid.to_string()).exists());
    fixture.gone().await;
    assert!(super::runner::resources_idle());
    let entry = activity_snapshot().into_iter().find(|entry| entry.context == "linux-capture-failure").unwrap();
    assert_eq!(entry.state, "failed");
}
