use serde::Serialize;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock, atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering}};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::task::JoinSet;

use super::{redact, redaction::Redaction};

#[cfg(target_os = "linux")]
#[path = "linux_job.rs"]
pub(super) mod linux_job;
#[cfg(target_os = "linux")]
type Child = linux_job::Job;
#[cfg(not(target_os = "linux"))]
type Child = tokio::process::Child;

const OUTPUT_LIMIT: usize = 64 * 1024;
const CAPTURE_LIMIT: usize = 8 * 1024 * 1024;
pub(super) static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static ACTIVITY: OnceLock<Mutex<VecDeque<Activity>>> = OnceLock::new();
static APPLICATION: OnceLock<AppHandle> = OnceLock::new();
static SOURCES: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
struct RunningJob { id: String, cancelled: Arc<AtomicBool>, accepts_cancel: bool }
type RunningJobs = Vec<RunningJob>;
static RUNNING: OnceLock<Mutex<RunningJobs>> = OnceLock::new();
static SLOTS: OnceLock<Semaphore> = OnceLock::new();
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
    let _ = APPLICATION.set(app);
}

pub fn configure_sources(ids: Vec<String>) {
    *SOURCES.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap() = ids;
}

async fn read_secret(source_id: String) -> Result<Option<String>, String> {
    #[cfg(test)]
    if let Some(result) = fixture_secret(&source_id) { return result; }
    crate::credentials::read(source_id).await
}

pub(super) fn configured_secrets() -> Result<Vec<String>, String> {
    let ids = SOURCES.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap().clone();
    let mut secrets = Vec::new();
    for id in ids {
        #[cfg(test)]
        let token = fixture_secret(&id).unwrap_or_else(|| crate::settings::get_token(&id));
        #[cfg(not(test))]
        let token = crate::settings::get_token(&id);
        if let Some(token) = token? {
            if !token.is_empty() { secrets.push(token); }
        }
    }
    Ok(secrets)
}

fn publish(activity: &Activity) {
    let mut entries = ACTIVITY.get_or_init(|| Mutex::new(VecDeque::new())).lock().unwrap();
    if let Some(entry) = entries.iter_mut().find(|entry| entry.id == activity.id) {
        *entry = activity.clone();
    } else {
        while entries.len() >= 64 {
            let Some(index) = entries.iter().position(|entry| entry.state != "running") else { break };
            entries.remove(index);
        }
        entries.push_back(activity.clone());
    }
    drop(entries);
    if let Some(app) = APPLICATION.get() {
        let _ = app.emit("git-activity", activity);
    }
}

pub fn activity_snapshot() -> Vec<Activity> {
    ACTIVITY.get_or_init(|| Mutex::new(VecDeque::new())).lock().unwrap().iter().cloned().collect()
}

pub fn clear_activity() -> ClearedActivity {
    let jobs = RUNNING.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap();
    let mut entries = ACTIVITY.get_or_init(|| Mutex::new(VecDeque::new())).lock().unwrap();
    let retained = jobs.iter().map(|job| job.id.clone()).collect();
    entries.retain(|entry| entry.state == "running" || jobs.iter().any(|job| job.id == entry.id));
    ClearedActivity { running: entries.iter().cloned().collect(), retained, through: NEXT_ID.load(AtomicOrdering::Relaxed).saturating_sub(1) }
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
    raw_stderr: Vec<u8>,
}

impl Captured {
    pub fn safe(&self, text: &str) -> String { self.redaction.safe(text) }

    pub(crate) fn stderr_contains(&self, text: &str) -> bool {
        String::from_utf8_lossy(&self.raw_stderr).contains(text)
    }

    pub fn last_error(&self) -> String {
        if self.redaction.quiet() { return self.safe(""); }
        super::last_error(&String::from_utf8_lossy(&self.stderr))
    }
}

pub(super) type Observer = Arc<dyn Fn(&str, &str) + Send + Sync>;
#[cfg(test)]
pub(super) type ExitObserver = Arc<dyn Fn(&str, &str) -> Result<(), String> + Send + Sync>;

pub type StdoutSink = Arc<dyn Fn(&[u8]) -> bool + Send + Sync>;
type SinkHook = (StdoutSink, Arc<AtomicBool>);

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
        if let Some((sink, stop)) = &sink {
            if sink(&buffer[..count]) { stop.store(true, AtomicOrdering::Relaxed); break; }
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
    #[cfg(target_os = "linux")]
    {
        let args: Vec<_> = request.args.iter().map(|arg| arg.to_string()).collect();
        let context = request.context.to_string();
        let expected = request.expected.to_vec();
        let timeout = request.timeout;
        let policy = request.policy;
        let input = input.map(<[u8]>::to_vec);
        let owner = Arc::new(AtomicBool::new(false));
        let _call = CancelOnDrop(owner.clone());
        tokio::spawn(async move {
            let args: Vec<_> = args.iter().map(String::as_str).collect();
            run_inner(Request { args: &args, context: &context, expected: &expected, timeout, policy }, observer,
                cancellation, input.as_deref(), Some(owner), sink, #[cfg(test)] after_exit).await
        }).await.map_err(|_| "Git runner task failed".to_string())?
    }
    #[cfg(not(target_os = "linux"))]
    run_inner(request, observer, cancellation, input, None, sink, #[cfg(test)] after_exit).await
}

#[cfg(target_os = "linux")]
struct CancelOnDrop(Arc<AtomicBool>);

#[cfg(target_os = "linux")]
impl Drop for CancelOnDrop {
    fn drop(&mut self) { self.0.store(true, AtomicOrdering::Relaxed); }
}

fn is_cancelled(external: &Option<Arc<AtomicBool>>, owner: &Option<Arc<AtomicBool>>) -> bool {
    external.as_ref().is_some_and(|flag| flag.load(AtomicOrdering::Relaxed))
        || owner.as_ref().is_some_and(|flag| flag.load(AtomicOrdering::Relaxed))
}

#[cfg(target_os = "linux")]
struct Registration(String);

#[cfg(target_os = "linux")]
impl Drop for Registration {
    fn drop(&mut self) {
        RUNNING.get().unwrap().lock().unwrap().retain(|job| job.id != self.0);
        let mut incomplete = ACTIVITY.get().and_then(|entries| entries.lock().ok()
            .and_then(|entries| entries.iter().find(|entry| entry.id == self.0 && entry.state == "running").cloned()));
        if let Some(activity) = &mut incomplete {
            activity.state = "failed".into();
            activity.sequence += 1;
            activity.output.push(ActivityOutput { sequence: activity.sequence, stream: "runner".into(),
                text: "Git runner stopped before finalizing its job.".into() });
            publish(activity);
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

async fn run_inner(request: Request<'_>, observer: Option<Observer>, cancellation: Option<Arc<AtomicBool>>, input: Option<&[u8]>, owner: Option<Arc<AtomicBool>>, sink: Option<StdoutSink>, #[cfg(test)] after_exit: Option<ExitObserver>) -> Result<Captured, String> {
    #[cfg(feature = "benchmark")]
    let operation = crate::benchmark::operation(request.args);
    #[cfg(feature = "benchmark")]
    crate::benchmark::command(operation);
    #[cfg(feature = "benchmark")]
    let queue = crate::benchmark::Span::new("git.queue", operation);
    #[cfg(target_os = "linux")]
    let _filesystem = loop {
        tokio::select! {
            guard = filesystem_gate().read() => break guard,
            _ = tokio::time::sleep(Duration::from_millis(25)) => if is_cancelled(&cancellation, &owner) { return Err("Git command cancelled".into()); },
        }
    };
    #[cfg(not(target_os = "linux"))]
    let _filesystem = filesystem_gate().read().await;
    let permit = SLOTS.get_or_init(|| Semaphore::new(32)).acquire();
    tokio::pin!(permit);
    let _permit = loop {
        tokio::select! {
            result = &mut permit => break result.map_err(|_| "Git runner unavailable")?,
            _ = tokio::time::sleep(Duration::from_millis(25)) => {
                if is_cancelled(&cancellation, &owner) { return Err("Git command cancelled".into()); }
            }
        }
    };
    if is_cancelled(&cancellation, &owner) { return Err("Git command cancelled".into()); }
    #[cfg(feature = "benchmark")]
    drop(queue);
    #[cfg(feature = "benchmark")]
    let _process = crate::benchmark::Span::new("git.process", operation);
    let ids = SOURCES.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap().clone();
    let mut secrets = Vec::new();
    let mut quiet = false;
    for source_id in ids {
        if is_cancelled(&cancellation, &owner) { return Err("Git command cancelled".into()); }
        match read_secret(source_id).await {
            Ok(Some(token)) if !token.is_empty() => secrets.push(token),
            Ok(_) => (),
            Err(_) => { quiet = true; break; }
        }
    }
    let redaction = if quiet { Redaction::Quiet } else { Redaction::Ready(secrets) };
    let observer = if quiet { None } else { observer };
    if is_cancelled(&cancellation, &owner) { return Err("Git command cancelled".into()); }
    let start = Instant::now();
    let cancelled = Arc::new(AtomicBool::new(false));
    let id = {
        let mut jobs = RUNNING.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap();
        let id = format!("git-{}", NEXT_ID.fetch_add(1, AtomicOrdering::Relaxed));
        jobs.push(RunningJob { id: id.clone(), cancelled: cancelled.clone(), accepts_cancel: true });
        id
    };
    let mut activity = Activity {
        id: id.clone(), context: if quiet { "Git operation".into() } else { redaction.safe(request.context) },
        argv: if quiet { Vec::new() } else { request.args.iter().map(|arg| redaction.safe(arg)).collect() },
        sequence: 0, started_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(), elapsed_ms: 0,
        state: "running".into(), exit_code: None, output: Vec::new(), truncated: false, stdout_bytes: 0, stderr_bytes: 0,
    };
    publish(&activity);
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
        let hook = sink.map(|sink| (sink, sunk.clone()));
        let mut streams = Streams::new(stdin, stdout, stderr, input, redaction.secrets(), if quiet { OutputPolicy::Metadata } else { request.policy }, hook);
        let deadline = tokio::time::sleep(request.timeout);
        tokio::pin!(deadline);
        let mut tick = tokio::time::interval(Duration::from_millis(25));
        let mut logged = 0;
        let mut stopped = None;
        let status = loop {
            tokio::select! {
                status = child.wait() => break status.map_err(|_| "Could not wait for Git".to_string())?,
                _ = &mut deadline => { stopped = Some("timedOut"); break terminate(&mut child).await?; },
                _ = tick.tick() => if sunk.load(AtomicOrdering::Relaxed) { stopped = Some("sunk"); break terminate(&mut child).await?; } else if cancelled.load(AtomicOrdering::Relaxed) || is_cancelled(&cancellation, &owner) { stopped = Some("cancelled"); break terminate(&mut child).await?; },
                Some((stream, text)) = streams.receiver.recv() => record_output(&mut activity, &mut logged, stream, text, observer.as_ref(), start),
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
            code: status.code(), redaction: redaction.clone(), raw_stderr: stderr })
    }.await.map_err(|error: String| if quiet { error } else { redaction.safe(&error) });
    if activity.state == "running" { activity.state = "failed".into(); }
    if let Err(error) = &result {
        activity.sequence += 1;
        activity.output.push(ActivityOutput { sequence: activity.sequence, stream: "runner".into(), text: if quiet { error.clone() } else { redaction.safe(error) } });
    }
    activity.sequence += 1;
    activity.elapsed_ms = start.elapsed().as_millis();
    publish(&activity);
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

fn record_output(activity: &mut Activity, logged: &mut usize, stream: String, text: String, observer: Option<&Observer>, start: Instant) {
    if let Some(observer) = observer { observer(&stream, &text); }
    if *logged + text.len() > OUTPUT_LIMIT || activity.output.len() >= 512 {
        activity.truncated = true;
        return;
    }
    *logged += text.len();
    activity.sequence += 1;
    activity.elapsed_ms = start.elapsed().as_millis();
    activity.output.push(ActivityOutput { sequence: activity.sequence, stream, text });
    publish(activity);
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
    SLOTS.get().is_none_or(|slots| slots.available_permits() == 32)
        && RUNNING.get().is_none_or(|jobs| jobs.lock().unwrap().is_empty())
}

#[cfg(all(test, target_os = "linux"))]
pub(super) fn publish_cleanup_state(context: &str, state: &str) {
    let mut entry = activity_snapshot().into_iter().find(|entry| entry.context == context).unwrap();
    entry.state = state.to_string(); publish(&entry);
}

/// `git` with prompts disabled and (on Windows) no console window.
fn git() -> tokio::process::Command {
    let mut c = tokio::process::Command::new("git");
    c.env("GIT_TERMINAL_PROMPT", "0").env("GCM_INTERACTIVE", "Never")
        .env("GIT_ASKPASS", "").env("SSH_ASKPASS", "")
        .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes -oConnectTimeout=15")
        .env("GIT_OPTIONAL_LOCKS", "0").stdin(Stdio::null()).kill_on_drop(true);
    c.env("GIT_NO_REPLACE_OBJECTS", "1");
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
pub(crate) struct CredentialFixture { sources: Vec<String> }

#[cfg(test)]
impl CredentialFixture {
    pub(crate) fn new(secrets: FixtureSecrets) -> Self {
        let sources = SOURCES.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap().clone();
        assert!(FIXTURE_SECRETS.lock().unwrap().replace(secrets.clone()).is_none());
        configure_sources(secrets.keys().cloned().collect());
        Self { sources }
    }

    pub(crate) fn replace(id: &str, token: &str) {
        FIXTURE_SECRETS.lock().unwrap().as_mut().unwrap().insert(id.into(), Ok(Some(token.into())));
    }
}

#[cfg(test)]
impl Drop for CredentialFixture {
    fn drop(&mut self) {
        *FIXTURE_SECRETS.lock().unwrap() = None;
        configure_sources(self.sources.clone());
    }
}
