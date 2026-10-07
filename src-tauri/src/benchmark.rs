use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const EVENT_LIMIT: usize = 16384;
pub(crate) const OPERATIONS: &[&str] = &["other", "version", "rev-parse", "status", "ls-files", "ls-tree", "cat-file", "diff", "diff-tree", "var", "log", "show", "fetch", "clone", "checkout", "switch", "pull", "push", "add", "reset", "commit", "branch", "config", "for-each-ref", "symbolic-ref", "rev-list", "remote", "stash", "check-ignore", "check-attr"];
pub(crate) const PHASES: &[&str] = &["git.queue", "git.process", "compare.queue", "compare.prepare", "compare.inventory", "compare.metadata", "compare.history", "ipc.open", "ipc.refresh", "ipc.files", "ipc.content", "ui.request", "ui.files-ready", "ui.first-render", "ui.complete", "editor.import", "editor.construct", "editor.diff"];

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    version: u8,
    sample: u32,
    phase: &'static str,
    operation: &'static str,
    duration_ms: f64,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Aggregate {
    count: u64,
    total_ms: f64,
    max_ms: f64,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recorder {
    commands: BTreeMap<&'static str, u64>,
    aggregates: BTreeMap<String, Aggregate>,
    events: VecDeque<Event>,
    dropped_events: u64,
    write_failures: u64,
    #[serde(skip)]
    output: Option<std::fs::File>,
    #[serde(skip)]
    sample: u32,
}

static RECORDER: OnceLock<Mutex<Recorder>> = OnceLock::new();

fn recorder() -> &'static Mutex<Recorder> {
    RECORDER.get_or_init(|| Mutex::new(Recorder::default()))
}

pub fn operation(args: &[&str]) -> &'static str {
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if matches!(*arg, "-C" | "-c" | "--git-dir" | "--work-tree") { index += 2; continue; }
        if *arg == "--version" { return "version"; }
        if arg.starts_with('-') { index += 1; continue; }
        return OPERATIONS.iter().copied().find(|known| known == arg).unwrap_or("other");
    }
    "other"
}

impl Recorder {
    fn command(&mut self, operation: &'static str) {
        *self.commands.entry(operation).or_default() += 1;
    }

    fn record(&mut self, phase: &'static str, operation: &'static str, duration_ms: f64) {
        let aggregate = self.aggregates.entry(format!("{phase}/{operation}")).or_default();
        aggregate.count += 1;
        aggregate.total_ms += duration_ms;
        aggregate.max_ms = aggregate.max_ms.max(duration_ms);
        let event = Event { version: 1, sample: self.sample, phase, operation, duration_ms };
        if let Some(output) = &mut self.output {
            let written = serde_json::to_writer(&mut *output, &event).map_err(std::io::Error::other)
                .and_then(|()| output.write_all(b"\n"));
            if written.is_err() { self.write_failures += 1; }
        }
        if self.events.len() == EVENT_LIMIT { self.events.pop_front(); self.dropped_events += 1; }
        self.events.push_back(event);
    }
}

#[cfg(feature = "test-profile")]
pub fn initialize(path: &std::path::Path, sample: u32) -> Result<(), String> {
    let output = std::fs::OpenOptions::new().create_new(true).write(true).open(path)
        .map_err(|_| "Benchmark trace already exists or cannot be created")?;
    let mut recorder = recorder().lock().map_err(|_| "Benchmark recorder unavailable")?;
    recorder.output = Some(output);
    recorder.sample = sample;
    Ok(())
}

pub fn command(operation: &'static str) {
    if let Ok(mut recorder) = recorder().lock() { recorder.command(operation); }
}

pub struct Span {
    phase: &'static str,
    operation: &'static str,
    started: Instant,
}

impl Span {
    pub fn new(phase: &'static str, operation: &'static str) -> Self {
        Self { phase, operation, started: Instant::now() }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Ok(mut recorder) = recorder().lock() {
            recorder.record(self.phase, self.operation, self.started.elapsed().as_secs_f64() * 1000.0);
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendEvent {
    phase: String,
    duration_ms: f64,
}

#[tauri::command]
pub fn benchmark_record(event: FrontendEvent) -> Result<(), String> {
    let phase = PHASES.iter().copied().find(|known| *known == event.phase && (known.starts_with("ui.") || known.starts_with("editor.")))
        .ok_or("Unknown benchmark phase")?;
    if !event.duration_ms.is_finite() || event.duration_ms < 0.0 || event.duration_ms > 3_600_000.0 {
        return Err("Invalid benchmark duration".into());
    }
    recorder().lock().map_err(|_| "Benchmark recorder unavailable")?.record(phase, "other", event.duration_ms);
    Ok(())
}

#[tauri::command]
pub fn benchmark_snapshot() -> Result<serde_json::Value, String> {
    let recorder = recorder().lock().map_err(|_| "Benchmark recorder unavailable")?;
    serde_json::to_value(&*recorder).map_err(|_| "Benchmark snapshot unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_survive_retention_and_keys_remain_bounded() {
        let mut recorder = Recorder::default();
        for _ in 0..EVENT_LIMIT + 73 {
            recorder.command("status");
            recorder.record("git.process", "status", 1.0);
        }
        assert_eq!(recorder.commands["status"], (EVENT_LIMIT + 73) as u64);
        assert_eq!(recorder.events.len(), EVENT_LIMIT);
        assert_eq!(recorder.dropped_events, 73);
        assert_eq!(recorder.aggregates.len(), 1);
        assert_eq!(recorder.aggregates["git.process/status"].count, (EVENT_LIMIT + 73) as u64);
    }

    #[test]
    fn classification_and_serialization_cannot_disclose_command_inputs() {
        assert_eq!(operation(&["-C", "/private/root", "-c", "credential=secret", "status"]), "status");
        assert_eq!(operation(&["private-source-url"]), "other");
        let mut recorder = Recorder::default();
        recorder.command(operation(&["-C", "/private/root", "diff", "secret-file"]));
        recorder.record("git.process", "diff", 5.0);
        let text = serde_json::to_string(&recorder).unwrap();
        for secret in ["private", "secret", "argv", "root", "content", "cacheKey"] { assert!(!text.contains(secret)); }
    }

    #[test]
    fn frontend_rejects_unknown_nonfinite_and_negative_measurements() {
        for (phase, duration_ms) in [("private-file", 1.0), ("ui.complete", f64::NAN), ("ui.complete", f64::INFINITY), ("ui.complete", -1.0)] {
            assert!(benchmark_record(FrontendEvent { phase: phase.into(), duration_ms }).is_err());
        }
    }

    #[tokio::test]
    async fn real_git_command_counts_outlive_activity_retention_and_clear() {
        let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
        let before = recorder().lock().unwrap().commands.get("version").copied().unwrap_or(0);
        for _ in 0..80 { crate::git::buffered(&["--version"], "benchmark-test", &[0]).await.unwrap(); }
        let retained = crate::git::activity_snapshot().len();
        assert!(retained <= 64);
        crate::git::clear_activity();
        assert_eq!(recorder().lock().unwrap().commands["version"] - before, 80);
    }
}
