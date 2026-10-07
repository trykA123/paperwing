#[path = "batch.rs"]
mod batch;
#[path = "activity.rs"]
mod activity;
#[path = "locale.rs"]
pub(super) mod locale;
#[path = "cancel.rs"]
mod cancel;
#[path = "binary.rs"]
pub(super) mod binary;
pub(crate) use batch::BatchReader;
use serde::Serialize;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use std::sync::{Mutex, OnceLock, atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering}};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tauri::AppHandle;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::task::JoinSet;

use activity::Emit;
use cancel::{is_cancelled, Stop};
use super::{redact, redaction::{cached_secrets, remember_secrets, secret_key, Redaction}};

#[cfg(target_os = "linux")]
#[path = "linux_job.rs"]
pub(super) mod linux_job;
#[cfg(target_os = "linux")]
type Child = linux_job::Job;
#[cfg(not(target_os = "linux"))]
type Child = tokio::process::Child;

const OUTPUT_LIMIT: usize = 64 * 1024;
pub(crate) const CAPTURE_LIMIT: usize = 8 * 1024 * 1024;
pub(super) static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static SOURCES: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
struct RunningJob { id: String, cancelled: Arc<AtomicBool>, stop: Arc<Stop>, accepts_cancel: bool }
type RunningJobs = Vec<RunningJob>;
static RUNNING: OnceLock<Mutex<RunningJobs>> = OnceLock::new();
static SLOTS: OnceLock<Semaphore> = OnceLock::new();
#[cfg(test)]
pub(crate) fn require_runner_lock(lock: &tokio::sync::Mutex<()>) {
    assert!(lock.try_lock().is_err(), "tests that spawn Git must hold test_support::git_runner()");
}
#[cfg(test)]
pub(crate) static TEST_RUNNER_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityOutput {
    pub(super) sequence: u64,
    pub(super) stream: String,
    pub(super) text: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub(super) id: String,
    pub(super) context: String,
    pub(super) argv: Vec<String>,
    pub(super) sequence: u64,
    pub(super) started_at: u128,
    pub(super) elapsed_ms: u128,
    pub(super) state: String,
    pub(super) exit_code: Option<i32>,
    pub(super) output: Vec<ActivityOutput>,
    pub(super) truncated: bool,
    pub(super) stdout_bytes: usize,
    pub(super) stderr_bytes: usize,
}

pub fn attach(app: AppHandle) {
    activity::attach(app);
    std::thread::spawn(binary::resolve);
}

pub fn configure_sources(ids: Vec<String>) {
    *SOURCES.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap() = ids;
    #[cfg(not(test))]
    tauri::async_runtime::spawn(async {
        let _ = redaction_secrets(configured_sources(), &None, &Stop::new()).await;
    });
}

async fn read_secret(source_id: String) -> Result<Option<String>, String> {
    #[cfg(test)]
    if let Some(result) = fixture_secret(&source_id) {
        SECRET_READS.fetch_add(1, AtomicOrdering::SeqCst);
        return result;
    }
    crate::credentials::read(source_id).await
}

#[cfg(test)]
pub(crate) static SECRET_READS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

async fn redaction_secrets(
    ids: Vec<String>,
    cancellation: &Option<Arc<AtomicBool>>,
    stop: &Stop,
) -> Result<(Vec<String>, bool), String> {
    let key = secret_key(&ids);
    if let Some(secrets) = cached_secrets(&key) { return Ok((secrets, false)); }
    let mut secrets = Vec::new();
    for source_id in ids {
        if is_cancelled(cancellation, stop) { return Err("Git command cancelled".into()); }
        match read_secret(source_id).await {
            Ok(Some(token)) if !token.is_empty() => secrets.push(token),
            Ok(_) => (),
            Err(_) => return Ok((Vec::new(), true)),
        }
    }
    remember_secrets(key, &secrets);
    Ok((secrets, false))
}

pub(super) fn configured_sources() -> Vec<String> {
    SOURCES.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap().clone()
}

pub fn activity_snapshot() -> Vec<Activity> { activity::snapshot() }

pub fn clear_activity() -> ClearedActivity {
    let jobs = RUNNING.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap();
    let retained = jobs.iter().map(|job| job.id.clone()).collect();
    let running = activity::retain_active(|id| jobs.iter().any(|job| job.id == id));
    ClearedActivity { running, retained, through: NEXT_ID.load(AtomicOrdering::Relaxed).saturating_sub(1) }
}

#[derive(Serialize)]
pub struct ClearedActivity {
    pub(super) running: Vec<Activity>,
    pub(super) retained: Vec<String>,
    pub(super) through: u64,
}

#[cfg(target_os = "linux")]
fn reject_cancellation(id: &str) {
    if let Some(jobs) = RUNNING.get() {
        if let Some(job) = jobs.lock().unwrap().iter_mut().find(|job| job.id == id) { job.accepts_cancel = false; }
    }
}

pub fn cancel_activity(id: String) -> bool {
    let entries = RUNNING.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap();
    if let Some(job) = entries.iter().find(|job| job.id == id && job.accepts_cancel) {
        job.cancelled.store(true, AtomicOrdering::Relaxed);
        job.stop.wake();
        true
    } else {
        false
    }
}

#[derive(Clone, Copy)]
pub enum OutputPolicy {
    Text,
    Metadata,
}

pub struct Request<'a> {
    pub args: &'a [&'a str],
    pub context: &'a str,
    pub timeout: Duration,
    pub expected: &'a [i32],
    pub policy: OutputPolicy,
}

pub struct Captured {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub code: Option<i32>,
    redaction: Redaction,
}

impl Captured {
    pub fn safe(&self, text: &str) -> String { self.redaction.safe(text) }

    pub fn last_error(&self) -> String {
        if self.redaction.quiet() { return self.safe(""); }
        super::last_error(&String::from_utf8_lossy(&self.stderr))
    }
}

pub(super) type Observer = Arc<dyn Fn(&str, &str) + Send + Sync>;
#[cfg(test)]
pub(super) type ExitObserver = Arc<dyn Fn(&str, &str) -> Result<(), String> + Send + Sync>;

pub type StdoutSink = Arc<dyn Fn(&[u8]) -> bool + Send + Sync>;
type SinkHook = (StdoutSink, Arc<AtomicBool>, Arc<Stop>);

pub(super) async fn drain(reader: impl AsyncRead + Unpin, stream: &'static str, sender: tokio::sync::mpsc::Sender<(String, String)>, secrets: Vec<String>, policy: OutputPolicy) -> Result<(Vec<u8>, usize, bool), String> {
    drain_with(reader, stream, sender, secrets, policy, None).await
}

async fn drain_with(reader: impl AsyncRead + Unpin, stream: &'static str, sender: tokio::sync::mpsc::Sender<(String, String)>, secrets: Vec<String>, policy: OutputPolicy, sink: Option<SinkHook>) -> Result<(Vec<u8>, usize, bool), String> {
    let mut reader = reader;
    let mut buffer = [0; 4096];
    let mut captured = Vec::new();
    let mut pending = Vec::new();
    let mut total = 0;
    let mut oversized = false;
    let mut omitted = false;
    loop {
        let count = reader.read(&mut buffer).await.map_err(|_| "Could not read Git output".to_string())?;
        if count == 0 { break; }
        total += count;
        if let Some((sink, sunk, stop)) = &sink {
            if sink(&buffer[..count]) { sunk.store(true, AtomicOrdering::Relaxed); stop.wake(); break; }
            continue;
        }
        let room = CAPTURE_LIMIT.saturating_sub(captured.len());
        captured.extend_from_slice(&buffer[..count.min(room)]);
        if matches!(policy, OutputPolicy::Metadata) { continue; }
        for byte in &buffer[..count] {
            let boundary = *byte == b'\r' || *byte == b'\n';
            let incomplete_authority = boundary && pending.windows(3).rposition(|bytes| bytes == b"://")
                .is_some_and(|scheme| !pending[scheme + 3..].iter().any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@')));
            if boundary && !incomplete_authority {
                if oversized {
                    let _ = sender.send((stream.into(), "[oversized output record omitted]".into())).await;
                } else if !pending.is_empty() {
                    let text = redact(&String::from_utf8_lossy(&pending), &secrets);
                    let _ = sender.send((stream.into(), text)).await;
                }
                pending.clear();
                oversized = false;
            } else if !oversized {
                pending.push(*byte);
                if pending.len() > 16 * 1024 {
                    pending.clear();
                    oversized = true;
                    omitted = true;
                }
            }
        }
    }
    if oversized {
        let _ = sender.send((stream.into(), "[oversized output record omitted]".into())).await;
    } else if !pending.is_empty() {
        let _ = sender.send((stream.into(), redact(&String::from_utf8_lossy(&pending), &secrets))).await;
    }
    Ok((captured, total, omitted || total > CAPTURE_LIMIT))
}

pub async fn execute(request: Request<'_>, observer: Option<Observer>) -> Result<Captured, String> {
    #[cfg(test)]
    { execute_inner(request, observer, None, None, None).await }
    #[cfg(not(test))]
    { execute_inner(request, observer, None, None).await }
}

pub async fn execute_cancellable(request: Request<'_>, cancellation: Arc<AtomicBool>) -> Result<Captured, String> {
    execute_cancellable_input(request, cancellation, None).await
}

pub async fn execute_input(request: Request<'_>, input: Option<&[u8]>) -> Result<Captured, String> {
    if input.is_some_and(|bytes| bytes.len() > 256 * 1024) { return Err("Git input exceeded the limit".into()); }
    #[cfg(test)]
    { execute_inner(request, None, None, input, None).await }
    #[cfg(not(test))]
    { execute_inner(request, None, None, input).await }
}

pub async fn execute_cancellable_input(request: Request<'_>, cancellation: Arc<AtomicBool>, input: Option<&[u8]>) -> Result<Captured, String> {
    if input.is_some_and(|bytes| bytes.len() > 256 * 1024) { return Err("Git input exceeded the limit".into()); }
    #[cfg(test)]
    { execute_inner(request, None, Some(cancellation), input, None).await }
    #[cfg(not(test))]
    { execute_inner(request, None, Some(cancellation), input).await }
}

pub async fn execute_streaming(request: Request<'_>, cancellation: Arc<AtomicBool>, sink: StdoutSink) -> Result<Captured, String> {
    #[cfg(test)]
    { execute_core(request, None, Some(cancellation), None, Some(sink), None).await }
    #[cfg(not(test))]
    { execute_core(request, None, Some(cancellation), None, Some(sink)).await }
}

pub(super) async fn execute_inner(request: Request<'_>, observer: Option<Observer>, cancellation: Option<Arc<AtomicBool>>, input: Option<&[u8]>, #[cfg(test)] after_exit: Option<ExitObserver>) -> Result<Captured, String> {
    #[cfg(test)]
    { execute_core(request, observer, cancellation, input, None, after_exit).await }
    #[cfg(not(test))]
    { execute_core(request, observer, cancellation, input, None).await }
}

async fn execute_core(request: Request<'_>, observer: Option<Observer>, cancellation: Option<Arc<AtomicBool>>, input: Option<&[u8]>, sink: Option<StdoutSink>, #[cfg(test)] after_exit: Option<ExitObserver>) -> Result<Captured, String> {
    #[cfg(test)]
    require_runner_lock(&TEST_RUNNER_LOCK);
    #[cfg(target_os = "linux")]
    {
        let args: Vec<_> = request.args.iter().map(|arg| arg.to_string()).collect();
        let context = request.context.to_string();
        let expected = request.expected.to_vec();
        let timeout = request.timeout;
        let policy = request.policy;
        let input = input.map(<[u8]>::to_vec);
        let stop = Stop::new();
        let _call = CancelOnDrop(stop.clone());
        tokio::spawn(async move {
            let args: Vec<_> = args.iter().map(String::as_str).collect();
            run_inner(Request { args: &args, context: &context, expected: &expected, timeout, policy }, observer,
                cancellation, input.as_deref(), stop, sink, #[cfg(test)] after_exit).await
        }).await.map_err(|_| "Git runner task failed".to_string())?
    }
    #[cfg(not(target_os = "linux"))]
    run_inner(request, observer, cancellation, input, Stop::new(), sink, #[cfg(test)] after_exit).await
}

#[cfg(target_os = "linux")]
struct CancelOnDrop(Arc<Stop>);

#[cfg(target_os = "linux")]
impl Drop for CancelOnDrop {
    fn drop(&mut self) { self.0.request(); }
}

#[cfg(target_os = "linux")]
struct Registration(String);

#[cfg(target_os = "linux")]
impl Drop for Registration {
    fn drop(&mut self) {
        RUNNING.get().unwrap().lock().unwrap().retain(|job| job.id != self.0);
        let mut incomplete = activity::find_running(&self.0);
        if let Some(activity) = &mut incomplete {
            activity.state = "failed".into();
            activity.sequence += 1;
            activity.output.push(ActivityOutput { sequence: activity.sequence, stream: "runner".into(),
                text: "Git runner stopped before finalizing its job.".into() });
            activity::publish(activity, true);
        }
    }
}

type Drained = (Vec<u8>, usize, bool);
enum StreamResult { Input, Stdout(Drained), Stderr(Drained) }

struct Streams {
    receiver: tokio::sync::mpsc::Receiver<(String, String)>,
    tasks: JoinSet<Result<StreamResult, String>>,
}

impl Streams {
    fn new(mut stdin: Option<tokio::process::ChildStdin>, stdout: tokio::process::ChildStdout,
        stderr: tokio::process::ChildStderr, input: Option<&[u8]>, secrets: &[String], policy: OutputPolicy, sink: Option<SinkHook>) -> Self {
        let mut tasks = JoinSet::new();
        let input = input.map(<[u8]>::to_vec);
        tasks.spawn(async move {
            if let (Some(stdin), Some(input)) = (&mut stdin, input) {
                stdin.write_all(&input).await.map_err(|_| "Could not write Git input")?;
                stdin.shutdown().await.map_err(|_| "Could not close Git input")?;
            }
            Ok(StreamResult::Input)
        });
        let (sender, receiver) = tokio::sync::mpsc::channel(32);
        let stderr_sender = sender.clone();
        let stdout_secrets = secrets.to_vec();
        let stderr_secrets = secrets.to_vec();
        tasks.spawn(async move { drain_with(stdout, "stdout", sender, stdout_secrets, policy, sink).await.map(StreamResult::Stdout) });
        tasks.spawn(async move { drain(stderr, "stderr", stderr_sender, stderr_secrets, policy).await.map(StreamResult::Stderr) });
        Self { receiver, tasks }
    }

    async fn finish(&mut self, activity: &mut Activity, logged: &mut usize, observer: Option<&Observer>, start: Instant) -> Result<(Drained, Drained), String> {
        while let Some((stream, text)) = self.receiver.recv().await {
            record_output(activity, logged, stream, text, observer, start);
        }
        let (mut stdout, mut stderr) = (None, None);
        while let Some(result) = self.tasks.join_next().await {
            match result.map_err(|_| "Git stream task failed")?? {
                StreamResult::Input => (), StreamResult::Stdout(value) => stdout = Some(value), StreamResult::Stderr(value) => stderr = Some(value),
            }
        }
        Ok((stdout.ok_or("Missing Git stdout result")?, stderr.ok_or("Missing Git stderr result")?))
    }
}

fn write_root(args: &[&str]) -> Option<std::path::PathBuf> {
    write_root_from(args, std::env::current_dir())
}

fn write_root_from(
    args: &[&str],
    cwd: std::io::Result<std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    let mut root = cwd.unwrap_or_default();
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        match *arg {
            "-C" => { root = root.join(args.get(index + 1)?); index += 2; }
            "-c" | "--git-dir" | "--work-tree" => index += 2,
            "fetch" | "pull" => return Some(root),
            value if value.starts_with('-') => index += 1,
            _ => return None,
        }
    }
    None
}

async fn run_inner(request: Request<'_>, observer: Option<Observer>, cancellation: Option<Arc<AtomicBool>>, input: Option<&[u8]>, stop: Arc<Stop>, sink: Option<StdoutSink>, #[cfg(test)] after_exit: Option<ExitObserver>) -> Result<Captured, String> {
    if let Some(root) = write_root(request.args) { BatchReader::close_root(&root).await?; }
    if request.args.windows(2).any(|args| args == ["worktree", "remove"]) {
        let root = request.args.windows(2).find(|args| args[0] == "-C").map(|args| std::path::PathBuf::from(args[1]));
        if let Some(root) = &root { BatchReader::close_root(root).await?; }
        if let Some(path) = request.args.last().filter(|arg| !arg.starts_with('-')) {
            let path = std::path::Path::new(path);
            let path = if path.is_absolute() { path.to_path_buf() } else { root.unwrap_or_default().join(path) };
            BatchReader::close_root(&path).await?;
        }
    }
    #[cfg(feature = "benchmark")]
    let operation = crate::benchmark::operation(request.args);
    #[cfg(feature = "benchmark")]
    crate::benchmark::command(operation);
    #[cfg(feature = "benchmark")]
    let queue = crate::benchmark::Span::new("git.queue", operation);
    let _watch = cancel::watch(cancellation.as_ref(), &stop);
    let _filesystem = cancel::admitted(filesystem_gate().read(), &cancellation, &stop).await?;
    let _permit = cancel::admitted(SLOTS.get_or_init(|| Semaphore::new(32)).acquire(), &cancellation, &stop).await?
        .map_err(|_| "Git runner unavailable")?;
    if is_cancelled(&cancellation, &stop) { return Err("Git command cancelled".into()); }
    #[cfg(feature = "benchmark")]
    drop(queue);
    #[cfg(feature = "benchmark")]
    let _process = crate::benchmark::Span::new("git.process", operation);
    let (secrets, quiet) = redaction_secrets(configured_sources(), &cancellation, &stop).await?;
    let redaction = if quiet { Redaction::Quiet } else { Redaction::Ready(secrets) };
    let observer = if quiet { None } else { observer };
    if is_cancelled(&cancellation, &stop) { return Err("Git command cancelled".into()); }
    let start = Instant::now();
    let cancelled = Arc::new(AtomicBool::new(false));
    let id = {
        let mut jobs = RUNNING.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap();
        let id = format!("git-{}", NEXT_ID.fetch_add(1, AtomicOrdering::Relaxed));
        jobs.push(RunningJob { id: id.clone(), cancelled: cancelled.clone(), stop: stop.clone(), accepts_cancel: true });
        id
    };
    let mut activity = Activity {
        id: id.clone(), context: if quiet { "Git operation".into() } else { redaction.safe(request.context) },
        argv: if quiet { Vec::new() } else { request.args.iter().map(|arg| redaction.safe(arg)).collect() },
        sequence: 0, started_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(), elapsed_ms: 0,
        state: "running".into(), exit_code: None, output: Vec::new(), truncated: false, stdout_bytes: 0, stderr_bytes: 0,
    };
    activity::publish(&activity, true);
    #[cfg(target_os = "linux")]
    let _registration = Registration(id.clone());
    let result: Result<Captured, String> = async {
        let mut command = git();
        command.args(request.args).stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(target_os = "linux")]
        let mut child = Child::spawn(&mut command, Some(&id)).await.map_err(|_| "Could not start Git. Check Git installation and Linux pidfd/waitid process support.".to_string())?;
        #[cfg(not(target_os = "linux"))]
        let mut child = command.spawn()
            .map_err(|_| "Could not start Git".to_string())?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().ok_or("Missing Git stdout")?;
        let stderr = child.stderr.take().ok_or("Missing Git stderr")?;
        let streaming = sink.is_some();
        let sunk = Arc::new(AtomicBool::new(false));
        let hook = sink.map(|sink| (sink, sunk.clone(), stop.clone()));
        let mut streams = Streams::new(stdin, stdout, stderr, input, redaction.secrets(), if quiet { OutputPolicy::Metadata } else { request.policy }, hook);
        let deadline = tokio::time::sleep(request.timeout);
        tokio::pin!(deadline);
        let mut logged = 0;
        let mut stopped = None;
        let mut flush_due: Option<Instant> = None;
        let status = loop {
            tokio::select! {
                status = child.wait() => break status.map_err(|_| "Could not wait for Git".to_string())?,
                _ = &mut deadline => { stopped = Some("timedOut"); break terminate(&mut child).await?; },
                _ = cancel::until(&stop, || sunk.load(AtomicOrdering::Relaxed) || cancelled.load(AtomicOrdering::Relaxed) || is_cancelled(&cancellation, &stop)) => {
                    stopped = Some(if sunk.load(AtomicOrdering::Relaxed) { "sunk" } else { "cancelled" });
                    break terminate(&mut child).await?;
                }
                Some((stream, text)) = streams.receiver.recv() => match record_output(&mut activity, &mut logged, stream, text, observer.as_ref(), start) {
                    Some(Emit::Sent) => flush_due = None,
                    Some(Emit::Deferred(due)) => flush_due = Some(due),
                    None => (),
                },
                _ = tokio::time::sleep_until(tokio::time::Instant::from_std(flush_due.unwrap_or_else(Instant::now))), if flush_due.is_some() => {
                    activity::publish(&activity, true);
                    flush_due = None;
                }
            }
        };
        {
            let mut jobs = RUNNING.get().unwrap().lock().unwrap();
            #[cfg(target_os = "linux")]
            if let Some(job) = jobs.iter_mut().find(|job| job.id == id) { job.accepts_cancel = false; }
            #[cfg(not(target_os = "linux"))]
            jobs.retain(|job| job.id != id);
            #[cfg(not(target_os = "linux"))]
            if stopped.is_none() && (cancelled.load(AtomicOrdering::Relaxed) || cancellation.as_ref().is_some_and(|flag| flag.load(AtomicOrdering::Relaxed))) { stopped = Some("cancelled"); }
        }
        activity.exit_code = status.code();
        #[cfg(target_os = "linux")]
        if status.code().is_some() { stopped = None; }
        #[cfg(test)]
        let exit_check = after_exit.map_or(Ok(()), |after_exit| after_exit("exit", &status.code().unwrap_or(-1).to_string()));
        let readers = async {
            #[cfg(test)]
            exit_check?;
            streams.finish(&mut activity, &mut logged, observer.as_ref(), start).await
        };
        let drained = tokio::time::timeout(Duration::from_secs(2), readers).await;
        if !matches!(drained, Ok(Ok(_))) { streams.tasks.shutdown().await; }
        if let Some(state) = stopped.filter(|state| *state != "sunk") { activity.state = state.into(); }
        let ((stdout, stdout_bytes, stdout_truncated), (stderr, stderr_bytes, stderr_truncated)) = drained.map_err(|_| {
            if cfg!(target_os = "linux") { "Git output drain timed out. A helper may have detached from the job's process group." }
            else { "Git output drain timed out" }.to_string()
        })??;
        activity.stdout_bytes = stdout_bytes;
        activity.stderr_bytes = stderr_bytes;
        activity.truncated |= stdout_truncated || stderr_truncated;
        if !quiet && matches!(request.policy, OutputPolicy::Metadata) {
            record_output(&mut activity, &mut logged, "metadata".into(), format!("Content omitted: {stdout_bytes} stdout bytes, {stderr_bytes} stderr bytes"), None, start);
        }
        let sunk_stop = stopped == Some("sunk") || (stopped.is_none() && sunk.load(AtomicOrdering::Relaxed));
        if sunk_stop { stopped = None; }
        if let Some(state) = stopped {
            activity.state = state.into();
            return Err(if state == "cancelled" { "Git command cancelled" } else { "Git command timed out" }.into());
        }
        if (!streaming && stdout_bytes > CAPTURE_LIMIT) || stderr_bytes > CAPTURE_LIMIT {
            return Err("Git output exceeded the capture limit".into());
        }
        activity.state = if sunk_stop || status.code().is_some_and(|code| request.expected.contains(&code)) { "completed" } else { "failed" }.into();
        Ok(Captured { stdout, stderr: if quiet { Vec::new() } else { redaction.safe(&String::from_utf8_lossy(&stderr)).into_bytes() },
            code: status.code(), redaction: redaction.clone() })
    }.await.map_err(|error: String| if quiet { error } else { redaction.safe(&error) });
    if activity.state == "running" { activity.state = "failed".into(); }
    if let Err(error) = &result {
        activity.sequence += 1;
        activity.output.push(ActivityOutput { sequence: activity.sequence, stream: "runner".into(), text: if quiet { error.clone() } else { redaction.safe(error) } });
    }
    activity.sequence += 1;
    activity.elapsed_ms = start.elapsed().as_millis();
    activity::publish(&activity, true);
    RUNNING.get().unwrap().lock().unwrap().retain(|job| job.id != id);
    result
}

async fn terminate(child: &mut Child) -> Result<std::process::ExitStatus, String> {
    #[cfg(windows)]
    if let Some(pid) = child.id() {
        let mut command = tokio::process::Command::new("taskkill");
        command.args(["/PID", &pid.to_string(), "/T", "/F"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true).creation_flags(0x0800_0000);
        let _ = tokio::time::timeout(Duration::from_secs(2), command.status()).await;
    }
    child.start_kill().map_err(|_| "Could not stop Git".to_string())?;
    tokio::time::timeout(Duration::from_secs(2), child.wait()).await
        .map_err(|_| "Git termination timed out".to_string())?.map_err(|_| "Could not reap Git".to_string())
}

fn record_output(activity: &mut Activity, logged: &mut usize, stream: String, text: String, observer: Option<&Observer>, start: Instant) -> Option<Emit> {
    if let Some(observer) = observer { observer(&stream, &text); }
    if *logged + text.len() > OUTPUT_LIMIT || activity.output.len() >= 512 {
        activity.truncated = true;
        return None;
    }
    *logged += text.len();
    activity.sequence += 1;
    activity.elapsed_ms = start.elapsed().as_millis();
    activity.output.push(ActivityOutput { sequence: activity.sequence, stream, text });
    Some(activity::publish(activity, false))
}

pub fn filesystem_gate() -> &'static tokio::sync::RwLock<()> {
    static GATE: std::sync::OnceLock<tokio::sync::RwLock<()>> = std::sync::OnceLock::new();
    GATE.get_or_init(|| tokio::sync::RwLock::new(()))
}

pub async fn buffered(args: &[&str], context: &str, expected: &[i32]) -> Result<Captured, String> {
    execute(Request { args, context, expected, timeout: Duration::from_secs(45), policy: OutputPolicy::Text }, None).await
}

#[cfg(all(test, target_os = "linux"))]
pub(super) fn resources_idle() -> bool {
    batch::resources_idle() && SLOTS.get().is_none_or(|slots| slots.available_permits() == 32)
        && RUNNING.get().is_none_or(|jobs| jobs.lock().unwrap().is_empty())
}

#[cfg(all(test, target_os = "linux"))]
pub(super) fn publish_cleanup_state(context: &str, state: &str) {
    let mut entry = activity_snapshot().into_iter().find(|entry| entry.context == context).unwrap();
    entry.state = state.to_string(); activity::publish(&entry, true);
}

/// `git` with prompts disabled and (on Windows) no console window.
fn git() -> tokio::process::Command {
    let binary = binary::current();
    let mut c = tokio::process::Command::new(&binary.program);
    if let Some(path) = binary.path_value(std::env::var_os("PATH")) { c.env("PATH", path); }
    c.env("GIT_TERMINAL_PROMPT", "0").env("GCM_INTERACTIVE", "Never")
        .env("GIT_ASKPASS", "").env("SSH_ASKPASS", "")
        .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes -oConnectTimeout=15")
        .env("GIT_OPTIONAL_LOCKS", "0").stdin(Stdio::null()).kill_on_drop(true);
    c.env("GIT_NO_REPLACE_OBJECTS", "1");
    locale::apply(&mut c);
    for variable in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR", "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_CONFIG_COUNT", "GIT_CONFIG_PARAMETERS", "GIT_CONFIG", "GIT_SHALLOW_FILE", "GIT_NAMESPACE", "GIT_EXTERNAL_DIFF", "GIT_DIFF_OPTS", "GIT_TRACE", "GIT_TRACE_CURL", "GIT_CURL_VERBOSE"] {
        c.env_remove(variable);
    }
    #[cfg(windows)]
    c.creation_flags(0x0800_0000);
    c
}

#[cfg(test)]
type FixtureSecrets = std::collections::BTreeMap<String, Result<Option<String>, String>>;
#[cfg(test)]
static FIXTURE_SECRETS: Mutex<Option<FixtureSecrets>> = Mutex::new(None);

#[cfg(test)]
fn fixture_secret(id: &str) -> Option<Result<Option<String>, String>> {
    FIXTURE_SECRETS.lock().unwrap().as_ref().map(|secrets| secrets.get(id).cloned().unwrap_or(Ok(None)))
}

#[cfg(test)]
fn bump<I: IntoIterator>(ids: I) where I::Item: AsRef<str> {
    for id in ids { crate::credentials::advance_revision(id.as_ref(), || ()); }
}

#[cfg(test)]
pub(crate) struct CredentialFixture { sources: Vec<String> }

#[cfg(test)]
impl CredentialFixture {
    pub(crate) fn new(secrets: FixtureSecrets) -> Self {
        let sources = SOURCES.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap().clone();
        for token in secrets.values().filter_map(|value| value.clone().ok().flatten()) { super::redaction::remember_secret(&token); }
        assert!(FIXTURE_SECRETS.lock().unwrap().replace(secrets.clone()).is_none());
        bump(secrets.keys());
        configure_sources(secrets.keys().cloned().collect());
        Self { sources }
    }

    pub(crate) fn replace(id: &str, token: &str) {
        FIXTURE_SECRETS.lock().unwrap().as_mut().unwrap().insert(id.into(), Ok(Some(token.into())));
        bump([id]);
    }
}

#[cfg(test)]
impl Drop for CredentialFixture {
    fn drop(&mut self) {
        let ids: Vec<String> = FIXTURE_SECRETS.lock().unwrap().take().map(|secrets| secrets.into_keys().collect()).unwrap_or_default();
        bump(&ids);
        configure_sources(self.sources.clone());
    }
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod runner_tests;

#[cfg(test)]
mod write_root_tests {
    use super::*;

    #[test]
    fn absolute_fetch_and_pull_roots_survive_an_unreadable_cwd() {
        let root = crate::test_support::tmp_root().join("write-root");
        let path = root.to_str().unwrap();
        for operation in ["fetch", "pull"] {
            let cwd = Err(std::io::Error::from(std::io::ErrorKind::NotFound));
            assert_eq!(
                write_root_from(&["-c", "alias.unrelated=pull", "-C", path, operation], cwd),
                Some(root.clone())
            );
        }
    }
}
