use serde::Serialize;
use std::cmp::Ordering;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock, atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering}};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

const OUTPUT_LIMIT: usize = 64 * 1024;
const CAPTURE_LIMIT: usize = 8 * 1024 * 1024;
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static ACTIVITY: OnceLock<Mutex<VecDeque<Activity>>> = OnceLock::new();
static APPLICATION: OnceLock<AppHandle> = OnceLock::new();
static SOURCES: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
type RunningJobs = Vec<(String, Arc<AtomicBool>)>;
static RUNNING: OnceLock<Mutex<RunningJobs>> = OnceLock::new();
static SLOTS: OnceLock<Semaphore> = OnceLock::new();
#[cfg(test)]
pub(crate) static TEST_RUNNER_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityOutput {
    sequence: u64,
    stream: String,
    text: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    id: String,
    context: String,
    argv: Vec<String>,
    sequence: u64,
    started_at: u128,
    elapsed_ms: u128,
    state: String,
    exit_code: Option<i32>,
    output: Vec<ActivityOutput>,
    truncated: bool,
    stdout_bytes: usize,
    stderr_bytes: usize,
}

pub fn attach(app: AppHandle) {
    let _ = APPLICATION.set(app);
}

pub fn configure_sources(ids: Vec<String>) {
    *SOURCES.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap() = ids;
}

fn configured_secrets() -> Vec<String> {
    SOURCES.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap().iter()
        .filter_map(|id| crate::settings::get_token(id)).filter(|value| !value.is_empty()).collect()
}

pub fn redact(text: &str, secrets: &[String]) -> String {
    let mut clean = text.to_string();
    let mut ordered = secrets.to_vec();
    ordered.sort_by_key(|value| std::cmp::Reverse(value.len()));
    for secret in ordered {
        clean = clean.replace(&secret, "[redacted]");
    }
    let mut authorities = String::new();
    let mut remaining = clean.as_str();
    while let Some(scheme) = remaining.find("://") {
        let start = scheme + 3;
        authorities.push_str(&remaining[..start]);
        remaining = &remaining[start..];
        let mut end = remaining.len();
        let mut cursor = 0;
        while let Some(boundary) = remaining[cursor..].find(['/', '?', '#']) {
            let index = cursor + boundary;
            if index > 0 && remaining.as_bytes()[index - 1] == b':' && remaining[index..].starts_with("//") {
                let prefix = &remaining[..index - 1];
                if let Some((_, next_scheme)) = prefix.rsplit_once(char::is_whitespace) {
                    if reqwest::Url::parse(&format!("{next_scheme}://invalid.test")).is_ok_and(|url| {
                        url.scheme().eq_ignore_ascii_case(next_scheme) && url.host_str() == Some("invalid.test")
                    }) {
                        end = prefix.len() - next_scheme.len();
                        break;
                    }
                }
                cursor = index + 2;
            } else {
                end = index;
                break;
            }
        }
        let authority = &remaining[..end];
        let value = authority.trim_end_matches(char::is_whitespace);
        let valid_authority = !value.chars().any(char::is_whitespace) && !value.contains("://")
            && reqwest::Url::parse(&format!("https://{value}")).is_ok_and(|url| {
                url.has_host() && url.username().is_empty() && url.password().is_none() && url.path() == "/"
                    && url.query().is_none() && url.fragment().is_none()
            });
        let redact_end = value.rfind('@').or_else(|| {
            (!value.is_empty() && !valid_authority).then_some(value.len())
        });
        if let Some(redact_end) = redact_end {
            authorities.push_str("[redacted]");
            authorities.extend(value[..redact_end].chars().filter(|character| character.is_whitespace()));
            authorities.push_str(&authority[redact_end..]);
        } else {
            authorities.push_str(authority);
        }
        remaining = &remaining[end..];
        let tail_end = remaining.find(char::is_whitespace).unwrap_or(remaining.len())
            .min(remaining.find("://").unwrap_or(remaining.len()));
        let tail = &remaining[..tail_end];
        if let Some(query) = tail.find(['?', '#']) {
            authorities.push_str(&tail[..query]);
            authorities.push_str("?[redacted]");
        } else {
            authorities.push_str(tail);
        }
        remaining = &remaining[tail_end..];
    }
    authorities.push_str(remaining);
    let mut result = String::new();
    for line in authorities.split_inclusive(['\r', '\n']) {
        let lower = line.to_ascii_lowercase();
        if let Some(index) = ["authorization:", "proxy-authorization:", "cookie:", "set-cookie:"]
            .iter().filter_map(|header| lower.find(header)).min() {
            result.push_str(&line[..index]);
            result.push_str("[redacted header]\n");
            continue;
        }
        result.push_str(line);
    }
    result
}

pub fn safe(text: &str) -> String {
    redact(text, &configured_secrets())
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

#[tauri::command]
pub fn activity_snapshot() -> Vec<Activity> {
    ACTIVITY.get_or_init(|| Mutex::new(VecDeque::new())).lock().unwrap().iter().cloned().collect()
}

#[tauri::command]
pub fn clear_activity() -> ClearedActivity {
    let jobs = RUNNING.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap();
    let mut entries = ACTIVITY.get_or_init(|| Mutex::new(VecDeque::new())).lock().unwrap();
    let retained = jobs.iter().filter(|(id, _)| !entries.iter().any(|entry| entry.id == *id && entry.state != "running"))
        .map(|(id, _)| id.clone()).collect();
    entries.retain(|entry| entry.state == "running");
    ClearedActivity { running: entries.iter().cloned().collect(), retained, through: NEXT_ID.load(AtomicOrdering::Relaxed).saturating_sub(1) }
}

#[derive(Serialize)]
pub struct ClearedActivity {
    running: Vec<Activity>,
    retained: Vec<String>,
    through: u64,
}

#[tauri::command]
pub fn cancel_activity(id: String) -> bool {
    let entries = RUNNING.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap();
    if let Some((_, flag)) = entries.iter().find(|(key, _)| key == &id) {
        flag.store(true, AtomicOrdering::Relaxed);
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
}

type Observer = Arc<dyn Fn(&str, &str) + Send + Sync>;
#[cfg(test)]
type ExitObserver = Arc<dyn Fn(&str, &str) -> Result<(), String> + Send + Sync>;

async fn drain(reader: impl AsyncRead + Unpin, stream: &'static str, sender: tokio::sync::mpsc::Sender<(String, String)>, secrets: Vec<String>, policy: OutputPolicy) -> Result<(Vec<u8>, usize, bool), String> {
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

async fn execute_inner(request: Request<'_>, observer: Option<Observer>, cancellation: Option<Arc<AtomicBool>>, input: Option<&[u8]>, #[cfg(test)] after_exit: Option<ExitObserver>) -> Result<Captured, String> {
    let _filesystem = filesystem_gate().read().await;
    let permit = SLOTS.get_or_init(|| Semaphore::new(32)).acquire();
    tokio::pin!(permit);
    let _permit = loop {
        tokio::select! {
            result = &mut permit => break result.map_err(|_| "Git runner unavailable")?,
            _ = tokio::time::sleep(Duration::from_millis(25)) => {
                if cancellation.as_ref().is_some_and(|flag| flag.load(AtomicOrdering::Relaxed)) { return Err("Git command cancelled".into()); }
            }
        }
    };
    if cancellation.as_ref().is_some_and(|flag| flag.load(AtomicOrdering::Relaxed)) { return Err("Git command cancelled".into()); }
    let secrets = configured_secrets();
    let start = Instant::now();
    let cancelled = Arc::new(AtomicBool::new(false));
    let id = {
        let mut jobs = RUNNING.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap();
        let id = format!("git-{}", NEXT_ID.fetch_add(1, AtomicOrdering::Relaxed));
        jobs.push((id.clone(), cancelled.clone()));
        id
    };
    let mut activity = Activity {
        id: id.clone(), context: redact(request.context, &secrets), argv: request.args.iter().map(|arg| redact(arg, &secrets)).collect(),
        sequence: 0, started_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(), elapsed_ms: 0,
        state: "running".into(), exit_code: None, output: Vec::new(), truncated: false, stdout_bytes: 0, stderr_bytes: 0,
    };
    publish(&activity);
    let result: Result<Captured, String> = async {
        let mut child = git().args(request.args).stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
            .map_err(|_| "Could not start Git".to_string())?;
        let mut stdin = child.stdin.take();
        let input = input.map(<[u8]>::to_vec);
        let mut input_task = tokio::spawn(async move {
            if let (Some(stdin), Some(input)) = (&mut stdin, input) {
                stdin.write_all(&input).await.map_err(|_| "Could not write Git input")?;
                stdin.shutdown().await.map_err(|_| "Could not close Git input")?;
            }
            Ok::<_, String>(())
        });
        let stdout = child.stdout.take().ok_or("Missing Git stdout")?;
        let stderr = child.stderr.take().ok_or("Missing Git stderr")?;
        let (sender, mut receiver) = tokio::sync::mpsc::channel(32);
        let mut stdout_task = tokio::spawn(drain(stdout, "stdout", sender.clone(), secrets.clone(), request.policy));
        let mut stderr_task = tokio::spawn(drain(stderr, "stderr", sender, secrets.clone(), request.policy));
        let deadline = tokio::time::sleep(request.timeout);
        tokio::pin!(deadline);
        let mut tick = tokio::time::interval(Duration::from_millis(25));
        let mut logged = 0;
        let mut stopped = None;
        let status = loop {
            tokio::select! {
                status = child.wait() => break status.map_err(|_| "Could not wait for Git".to_string())?,
                _ = &mut deadline => { stopped = Some("timedOut"); break terminate(&mut child).await?; },
                _ = tick.tick() => if cancelled.load(AtomicOrdering::Relaxed) || cancellation.as_ref().is_some_and(|flag| flag.load(AtomicOrdering::Relaxed)) { stopped = Some("cancelled"); break terminate(&mut child).await?; },
                Some((stream, text)) = receiver.recv() => record_output(&mut activity, &mut logged, stream, text, observer.as_ref(), start),
            }
        };
        {
            let mut jobs = RUNNING.get().unwrap().lock().unwrap();
            jobs.retain(|(key, _)| key != &id);
            if stopped.is_none() && (cancelled.load(AtomicOrdering::Relaxed) || cancellation.as_ref().is_some_and(|flag| flag.load(AtomicOrdering::Relaxed))) { stopped = Some("cancelled"); }
        }
        activity.exit_code = status.code();
        if let Some(state) = stopped { activity.state = state.into(); }
        #[cfg(test)]
        let exit_check = after_exit.map_or(Ok(()), |after_exit| after_exit("exit", &status.code().unwrap_or(-1).to_string()));
        let readers = async {
            #[cfg(test)]
            exit_check?;
            while let Some((stream, text)) = receiver.recv().await {
                record_output(&mut activity, &mut logged, stream, text, observer.as_ref(), start);
            }
            (&mut input_task).await.map_err(|_| "Git input task failed")??;
            Ok::<_, String>(((&mut stdout_task).await.map_err(|_| "Git stdout task failed")??, (&mut stderr_task).await.map_err(|_| "Git stderr task failed")??))
        };
        let drained = tokio::time::timeout(Duration::from_secs(2), readers).await;
        if drained.is_err() {
            input_task.abort();
            stdout_task.abort();
            stderr_task.abort();
        }
        let ((stdout, stdout_bytes, stdout_truncated), (stderr, stderr_bytes, stderr_truncated)) = drained.map_err(|_| "Git output drain timed out".to_string())??;
        activity.stdout_bytes = stdout_bytes;
        activity.stderr_bytes = stderr_bytes;
        activity.truncated |= stdout_truncated || stderr_truncated;
        if matches!(request.policy, OutputPolicy::Metadata) {
            record_output(&mut activity, &mut logged, "metadata".into(), format!("Content omitted: {stdout_bytes} stdout bytes, {stderr_bytes} stderr bytes"), None, start);
        }
        if let Some(state) = stopped {
            activity.state = state.into();
            return Err(if state == "cancelled" { "Git command cancelled" } else { "Git command timed out" }.into());
        }
        if stdout_bytes > CAPTURE_LIMIT || stderr_bytes > CAPTURE_LIMIT {
            return Err("Git output exceeded the capture limit".into());
        }
        activity.state = if status.code().is_some_and(|code| request.expected.contains(&code)) { "completed" } else { "failed" }.into();
        Ok(Captured { stdout, stderr, code: status.code() })
    }.await;
    if activity.state == "running" { activity.state = "failed".into(); }
    if let Err(error) = &result {
        activity.sequence += 1;
        activity.output.push(ActivityOutput { sequence: activity.sequence, stream: "runner".into(), text: redact(error, &secrets) });
    }
    activity.sequence += 1;
    activity.elapsed_ms = start.elapsed().as_millis();
    publish(&activity);
    RUNNING.get().unwrap().lock().unwrap().retain(|(key, _)| key != &id);
    result
}

async fn terminate(child: &mut tokio::process::Child) -> Result<std::process::ExitStatus, String> {
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

pub fn valid_url(u: &str) -> Result<(), String> {
    if u.is_empty() || u.starts_with('-') || u.chars().any(|c| c.is_control() || c.is_whitespace()) {
        Err("Invalid repository URL".into())
    } else {
        Ok(())
    }
}

pub fn valid_ref(name: &str) -> Result<(), String> {
    if name.is_empty() || name == "@" || name.starts_with('-') || name.ends_with('.') || name.contains("..") || name.contains("@{")
        || name.chars().any(|character| character.is_control() || character.is_whitespace() || "~^:?*[\\".contains(character))
        || name.split('/').any(|part| part.is_empty() || part.starts_with('.') || part.ends_with(".lock")) {
        return Err("Invalid ref name".into());
    }
    Ok(())
}

pub fn valid_path(path: &str, must_exist: bool) -> Result<(), String> {
    use std::path::{Component, Path, PathBuf};
    let path = Path::new(path);
    if !path.is_absolute() { return Err("Repository path must be absolute".into()); }
    let mut ancestor = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir | Component::CurDir => return Err("Traversal is not supported".into()),
            Component::Normal(part) => {
                let text = part.to_str().ok_or("Unsupported repository path")?;
                if text.ends_with(['.', ' ']) || text.contains(':') || text.chars().any(char::is_control) {
                    return Err("Unsupported repository path alias".into());
                }
                let stem = text.split('.').next().unwrap_or("").to_ascii_uppercase();
                if ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str()) || (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && stem.as_bytes()[3].is_ascii_digit()) {
                    return Err("Unsupported repository device path".into());
                }
            }
            Component::Prefix(prefix) => {
                #[cfg(windows)]
                if !matches!(prefix.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::UNC(_, _)) {
                    return Err("Unsupported repository device prefix".into());
                }
                let _ = prefix;
            }
            _ => {}
        }
        ancestor.push(component.as_os_str());
        match std::fs::symlink_metadata(&ancestor) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() { return Err("Linked repository paths are not supported".into()); }
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if metadata.file_attributes() & 0x400 != 0 { return Err("Reparse repository paths are not supported".into()); }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !must_exist => {}
            Err(_) => return Err("Repository path is unavailable".into()),
        }
    }
    Ok(())
}

pub fn valid_root(path: &str) -> Result<(), String> {
    valid_path(path, true)?;
    if !std::path::Path::new(path).is_dir() { return Err("Repository root must be a directory".into()); }
    let metadata = std::path::Path::new(path).join(".git");
    valid_path(metadata.to_str().ok_or("Unsupported repository path")?, true)?;
    if !metadata.is_dir() { return Err("Linked worktree metadata is not supported for these reads".into()); }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeRef {
    name: String,
    sha: String,
    current: bool,
    symbolic: String,
}

#[derive(Serialize)]
pub struct TreeRemote {
    name: String,
    urls: Vec<String>,
    refs: Vec<TreeRef>,
}

#[derive(Serialize)]
pub struct TreeStash {
    name: String,
    sha: String,
    subject: String,
}

#[derive(Serialize)]
pub struct TreeSubmodule {
    path: String,
    sha: String,
    url: Option<String>,
}

#[derive(Serialize, Default)]
pub struct RepositoryTree {
    branches: Vec<TreeRef>,
    remotes: Vec<TreeRemote>,
    tags: Vec<TreeRef>,
    stashes: Vec<TreeStash>,
    submodules: Vec<TreeSubmodule>,
}

async fn tree_output(path: &str, args: &[&str], expected: &[i32], policy: OutputPolicy) -> Result<Captured, String> {
    let mut argv = vec!["-C", path];
    argv.extend_from_slice(args);
    let output = execute(Request { args: &argv, context: &format!("Tree: {path}"), expected, policy, timeout: Duration::from_secs(45) }, None).await?;
    if !output.code.is_some_and(|code| expected.contains(&code)) {
        return Err(last_error(&String::from_utf8_lossy(&output.stderr)));
    }
    Ok(output)
}

#[tauri::command]
pub async fn repository_tree(path: String) -> Result<RepositoryTree, String> {
    valid_root(&path)?;
    let mut tree = RepositoryTree::default();
    let output = tree_output(&path, &["for-each-ref", "--format=%(refname)%09%(objectname)%09%(HEAD)%09%(symref)", "refs/heads", "refs/tags", "refs/remotes"], &[0], OutputPolicy::Text).await?;
    let mut remote_refs = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 4 { continue; }
        let make_ref = |name: &str| TreeRef { name: safe(name), sha: fields[1].into(), current: fields[2] == "*", symbolic: safe(fields[3]) };
        if let Some(name) = fields[0].strip_prefix("refs/heads/") { tree.branches.push(make_ref(name)); }
        else if let Some(name) = fields[0].strip_prefix("refs/tags/") { tree.tags.push(make_ref(name)); }
        else if let Some(name) = fields[0].strip_prefix("refs/remotes/") { remote_refs.push((name.to_string(), make_ref(name))); }
    }
    let output = tree_output(&path, &["remote"], &[0], OutputPolicy::Text).await?;
    for name in String::from_utf8_lossy(&output.stdout).lines() {
        valid_ref(name)?;
        let urls = tree_output(&path, &["remote", "get-url", "--all", name], &[0], OutputPolicy::Text).await?;
        let refs = remote_refs.iter().filter(|(reference, _)| reference.starts_with(&format!("{name}/"))).map(|(_, reference)| TreeRef {
            name: reference.name.clone(), sha: reference.sha.clone(), current: reference.current, symbolic: reference.symbolic.clone(),
        }).collect();
        tree.remotes.push(TreeRemote { name: safe(name), urls: safe(&String::from_utf8_lossy(&urls.stdout)).lines().map(str::to_string).collect(), refs });
    }
    let output = tree_output(&path, &["stash", "list", "--format=%gd%x09%H%x09%gs"], &[0], OutputPolicy::Text).await?;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let fields: Vec<_> = line.splitn(3, '\t').collect();
        if fields.len() == 3 { tree.stashes.push(TreeStash { name: fields[0].into(), sha: fields[1].into(), subject: safe(fields[2]) }); }
    }
    let output = tree_output(&path, &["ls-files", "--stage", "-z"], &[0], OutputPolicy::Metadata).await?;
    for entry in output.stdout.split(|byte| *byte == 0) {
        let text = String::from_utf8_lossy(entry);
        let Some((metadata, relative)) = text.split_once('\t') else { continue };
        let fields: Vec<_> = metadata.split(' ').collect();
        if fields.len() == 3 && fields[0] == "160000" && fields[2] == "0" {
            tree.submodules.push(TreeSubmodule { path: safe(relative), sha: fields[1].into(), url: None });
        }
    }
    let modules = std::path::Path::new(&path).join(".gitmodules");
    if modules.exists() {
        let module_path = modules.to_str().ok_or("Unsupported .gitmodules path")?;
        valid_path(module_path, true)?;
        if !modules.is_file() || std::fs::metadata(&modules).map_err(|_| ".gitmodules unavailable")?.len() > 256 * 1024 {
            return Err("Unsupported or oversized .gitmodules".into());
        }
        let output = tree_output(&path, &["config", "--no-includes", "--file", module_path, "--null", "--get-regexp", "^submodule\\..*\\.(path|url)$"], &[0, 1], OutputPolicy::Metadata).await?;
        let mut config = std::collections::BTreeMap::<String, (Option<String>, Option<String>)>::new();
        for entry in output.stdout.split(|byte| *byte == 0) {
            let text = String::from_utf8_lossy(entry);
            let Some((key, value)) = text.split_once('\n') else { continue };
            let Some((name, field)) = key.rsplit_once('.') else { continue };
            let pair = config.entry(name.into()).or_default();
            if field == "path" { pair.0 = Some(value.into()); }
            if field == "url" { pair.1 = Some(safe(value)); }
        }
        for (_, (relative, url)) in config {
            if let Some(submodule) = tree.submodules.iter_mut().find(|module| relative.as_deref().is_some_and(|relative| safe(relative) == module.path)) {
                submodule.url = url;
            }
        }
    }
    Ok(tree)
}

/// Picks the most useful line out of git's stderr, with a hint for common SSH failures.
pub fn last_error(stderr: &str) -> String {
    let clean = safe(stderr);
    let stderr = clean.as_str();
    let lines: Vec<&str> = stderr.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let has = |needle: &str| lines.iter().find(|l| l.contains(needle)).copied();
    if let Some(l) = has("Permission denied (publickey") {
        let host = l.split(": Permission").next().unwrap_or("the host");
        return format!("SSH key rejected by {host}. Check that your key is added under Settings → SSH keys on the server (test with: ssh -T {host})");
    }
    if has("Host key verification failed").is_some() {
        return "Unknown SSH host key. Connect once from a terminal (ssh -T git@host) and accept the fingerprint".into();
    }
    if let Some(l) = has("Could not resolve hostname").or_else(|| has("Connection timed out")).or_else(|| has("Connection refused")) {
        return format!("Network problem: {}", l.trim_start_matches("ssh: "));
    }
    if has("Repository not found").is_some() || has("does not appear to be a git repository").is_some() {
        return "Repository not found, or you have no access to it".into();
    }
    let line = lines
        .iter()
        .find(|l| l.starts_with("fatal:") || l.starts_with("error:"))
        .or(lines.last())
        .copied()
        .unwrap_or("git failed");
    line.trim_start_matches("fatal: ").trim_start_matches("error: ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn child_exit_rejects_late_cancellation_during_drain() {
        let _guard = TEST_RUNNER_LOCK.lock().await;
        let accepted = Arc::new(AtomicBool::new(false));
        let requested = accepted.clone();
        let after_exit: ExitObserver = Arc::new(move |_, code| {
            assert_eq!(code, "0");
            let entry = activity_snapshot().into_iter().find(|entry| entry.context == "runner-late-cancel").unwrap();
            requested.store(cancel_activity(entry.id), AtomicOrdering::Relaxed);
            Ok(())
        });
        let result = execute_inner(Request { args: &["--version"], context: "runner-late-cancel", expected: &[0], timeout: Duration::from_secs(45), policy: OutputPolicy::Text }, None, None, None, Some(after_exit)).await;
        assert!(!accepted.load(AtomicOrdering::Relaxed), "late cancellation was accepted after exit 0");
        assert_eq!(result.unwrap().code, Some(0));
        let entry = activity_snapshot().into_iter().find(|entry| entry.context == "runner-late-cancel").unwrap();
        assert_eq!(entry.state, "completed");
        assert_eq!(entry.exit_code, Some(0));
    }

    #[tokio::test]
    async fn stopped_outcome_survives_output_drain_failure() {
        let _guard = TEST_RUNNER_LOCK.lock().await;
        for state in ["cancelled", "timedOut"] {
            let context = format!("runner-drain-error-{state}");
            let observed_context = context.clone();
            let requested = Arc::new(AtomicBool::new(false));
            let observed = requested.clone();
            let observer: Observer = Arc::new(move |_, _| {
                if !observed.swap(true, AtomicOrdering::Relaxed) {
                    let entry = activity_snapshot().into_iter().find(|entry| entry.context == observed_context).unwrap();
                    assert!(cancel_activity(entry.id));
                }
            });
            let after_exit: ExitObserver = Arc::new(|_, _| Err("Injected Git output drain failure".into()));
            let timeout = if state == "timedOut" { Duration::ZERO } else { Duration::from_secs(45) };
            let result = execute_inner(Request { args: &["-c", "alias.paperwing-fixture-wait=!echo cancel-ready; while :; do :; done", "paperwing-fixture-wait"], context: &context, expected: &[0], timeout, policy: OutputPolicy::Text }, (state == "cancelled").then_some(observer), None, None, Some(after_exit)).await;
            assert_eq!(result.err().unwrap(), "Injected Git output drain failure");
            let entry = activity_snapshot().into_iter().find(|entry| entry.context == context).unwrap();
            assert_eq!(entry.state, state);
            assert!(entry.exit_code.is_some());
            if state == "cancelled" { assert!(requested.load(AtomicOrdering::Relaxed)); }
        }
    }

    #[tokio::test]
    async fn ambiguous_authorities_redact_before_record_and_word_splitting() {
        for separator in [" ", "\t", "\r", "\n", "\r\n"] {
            let input = format!("https://prefix.test{separator}https://probe-user:probe-password@invalid.test/repo");
            let expected = format!("https://prefix.test{separator}https://[redacted]@invalid.test/repo");
            assert_eq!(redact(&input, &[]), expected);
            assert_eq!(safe(&input), expected);
            let error = last_error(&input);
            assert!(!error.contains("probe-user") && !error.contains("probe-password"), "{error:?}");
            let benign = format!("https://prefix.test{separator}https://invalid.test:443/repo");
            assert_eq!(redact(&benign, &[]), benign);
            for authority in [
                format!("probe-user:probe-password{separator}extra"),
                format!("probe-user{separator}:probe-password"),
            ] {
                let input = format!("https://prefix.test{separator}https://{authority}@invalid.test/repo");
                let clean = redact(&input, &[]);
                assert!(!clean.contains("probe-user") && !clean.contains("probe-password"), "{clean:?}");
                assert!(clean.starts_with(&format!("https://prefix.test{separator}https://[redacted]")));
            }
            let input = format!("https://probe-user:probe-password{separator}extra@invalid.test/repo");
            let clean = redact(&input, &[]);
            assert!(!clean.contains("probe-user") && !clean.contains("probe-password"), "{clean:?}");
            assert!(clean.contains(separator));
            assert!(!last_error(&input).contains("probe-password"));
            let input = format!("https://probe-user{separator}:probe-password@invalid.test/repo");
            let clean = redact(&input, &[]);
            assert!(!clean.contains("probe-user") && !clean.contains("probe-password"), "{clean:?}");
            assert!(clean.contains(separator));
        }
        for (input, expected) in [
            ("https://prefix.testhttps://probe-user:probe-password@invalid.test/repo", "https://[redacted]@invalid.test/repo"),
            ("https://prefix.test/repohttps://probe-user:probe-password@invalid.test/repo", "https://prefix.test/repohttps://[redacted]@invalid.test/repo"),
            ("https://prefix.test/repo,https://probe-user:probe-password@invalid.test/repo", "https://prefix.test/repo,https://[redacted]@invalid.test/repo"),
            ("https://probe-user:probe-password@invalid.test/repo https://prefix.test", "https://[redacted]@invalid.test/repo https://prefix.test"),
            ("https://probe-user:probe-password@invalid.test/repo https://prefix.test https://probe-user:probe-password@invalid.test/repo", "https://[redacted]@invalid.test/repo https://prefix.test https://[redacted]@invalid.test/repo"),
            ("https://probe-user:probe-password", "https://[redacted]"),
            ("https://probe-user:probe-password@extra@invalid.test/repo", "https://[redacted]@invalid.test/repo"),
            ("https://prefix.test probe-user:probe-passwordhttps://probe-user:probe-password@invalid.test/repo", "https://[redacted] @invalid.test/repo"),
            ("https://[::1]:443/repo", "https://[::1]:443/repo"),
            ("https://\u{4f8b}.test/repo", "https://\u{4f8b}.test/repo"),
            ("https://prefix.test/repo,https://invalid.test/repo", "https://prefix.test/repo,https://invalid.test/repo"),
        ] {
            let clean = redact(input, &[]);
            assert_eq!(clean, expected);
            assert!(!clean.contains("probe-user") && !clean.contains("probe-password"), "{clean:?}");
            assert_eq!(redact(&clean, &[]), clean);
        }
        assert_eq!(redact("https://user:pass@host/repo", &[]), "https://[redacted]@host/repo");
        assert_eq!(redact("https://host:443/repo", &[]), "https://host:443/repo");
        let configured = "https://prefix.test https://configured-user:configured-password@invalid.test/repo?configured-token";
        let clean = redact(configured, &["configured-user".into(), "configured-password".into(), "configured-token".into()]);
        for secret in ["configured-user", "configured-password", "configured-token"] { assert!(!clean.contains(secret), "{clean:?}"); }
        let (mut writer, reader) = tokio::io::duplex(32);
        let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
        let task = tokio::spawn(drain(reader, "stderr", sender, vec!["configured-token".into()], OutputPolicy::Text));
        let writing = tokio::spawn(async move {
            for chunk in ["https://probe-user:probe-", "password extra@invalid.test/repo\r\nhttps://probe-user:", "probe-password\r", "\nextra@invalid.test/repo\nhttps://probe-user\r", "\n:probe-password@invalid.test/repo\n"] {
                writer.write_all(chunk.as_bytes()).await.unwrap();
            }
            for separator in [" ", "\t", "\r", "\n", "\r\n"] {
                for chunk in [format!("https://prefix.test{separator}htt"), "ps://probe-user:probe-".into(), "password@invalid.test/repo\n".into()] {
                    writer.write_all(chunk.as_bytes()).await.unwrap();
                }
                for chunk in [format!("https://prefix.test{separator}https://probe-user{separator}:probe-"), "password@invalid.test/repo\n".into()] {
                    writer.write_all(chunk.as_bytes()).await.unwrap();
                }
            }
            for chunk in ["https://prefix.test/repohttps://probe-user:probe-", "password@invalid.test/repo\nconfigu", "red-token\n"] {
                writer.write_all(chunk.as_bytes()).await.unwrap();
            }
        });
        let mut lines = Vec::new();
        while let Some((_, text)) = receiver.recv().await { lines.push(text); }
        writing.await.unwrap();
        task.await.unwrap().unwrap();
        let emitted = lines.join("\n");
        assert!(!emitted.contains("probe-user") && !emitted.contains("probe-password"), "{emitted}");
        assert!(!emitted.contains("configured-token"), "{emitted}");
        for separator in [" ", "\t", "\r", "\n", "\r\n"] {
            assert!(lines.contains(&format!("https://prefix.test{separator}https://[redacted]@invalid.test/repo")), "{lines:?}");
        }
        assert!(lines.contains(&"https://prefix.test/repohttps://[redacted]@invalid.test/repo".into()));
    }

    #[tokio::test]
    async fn concurrent_streams_redact_split_records_and_bound_content() {
        let (mut stdout_writer, stdout_reader) = tokio::io::duplex(64);
        let (mut stderr_writer, stderr_reader) = tokio::io::duplex(64);
        let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
        let stdout_task = tokio::spawn(drain(stdout_reader, "stdout", sender.clone(), vec![], OutputPolicy::Text));
        let stderr_task = tokio::spawn(drain(stderr_reader, "stderr", sender, vec![], OutputPolicy::Text));
        let stdout_writing = tokio::spawn(async move { stdout_writer.write_all(&"useful stdout\n".repeat(1000).into_bytes()).await.unwrap(); });
        let stderr_writing = tokio::spawn(async move { stderr_writer.write_all(&"useful stderr\r".repeat(1000).into_bytes()).await.unwrap(); });
        let mut counts = [0, 0];
        while let Some((stream, _)) = receiver.recv().await { counts[usize::from(stream == "stderr")] += 1; }
        stdout_writing.await.unwrap(); stderr_writing.await.unwrap();
        assert_eq!(counts, [1000, 1000]);
        assert_eq!(stdout_task.await.unwrap().unwrap().1, 14000);
        assert_eq!(stderr_task.await.unwrap().unwrap().1, 14000);
        let (mut writer, reader) = tokio::io::duplex(64);
        let (sender, mut receiver) = tokio::sync::mpsc::channel(32);
        let task = tokio::spawn(drain(reader, "stderr", sender, vec!["private-token".into()], OutputPolicy::Text));
        let writing = tokio::spawn(async move {
            for chunk in ["https://user:pass@host/repo?secret=yes pri", "vate-token\rAuthorization: Bearer hidden\nlast"] {
                writer.write_all(chunk.as_bytes()).await.unwrap();
            }
        });
        let mut lines = Vec::new();
        while let Some((stream, text)) = receiver.recv().await { assert_eq!(stream, "stderr"); lines.push(text); }
        writing.await.unwrap();
        let (_, size, truncated) = task.await.unwrap().unwrap();
        assert!(size > 0);
        assert!(!truncated);
        let text = lines.join("\n");
        for secret in ["user:pass", "secret=yes", "private-token", "hidden"] { assert!(!text.contains(secret), "{secret}"); }
        assert!(text.contains("last"));
        let (mut writer, reader) = tokio::io::duplex(128);
        let (sender, mut receiver) = tokio::sync::mpsc::channel(32);
        let task = tokio::spawn(drain(reader, "stdout", sender, vec![], OutputPolicy::Text));
        let writing = tokio::spawn(async move { writer.write_all(&vec![b'x'; 20000]).await.unwrap(); });
        assert_eq!(receiver.recv().await.unwrap().1, "[oversized output record omitted]");
        writing.await.unwrap();
        assert!(task.await.unwrap().unwrap().2);
    }

    #[tokio::test]
    async fn real_git_tracks_failure_clone_progress_timeout_and_clear() {
        let _guard = TEST_RUNNER_LOCK.lock().await;
        let root = std::env::temp_dir().join(format!("paperwing-runner-{}-{}", std::process::id(), NEXT_ID.fetch_add(1, AtomicOrdering::Relaxed)));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("source");
        let clone = root.join("clone");
        let source = source.to_str().unwrap();
        let clone = clone.to_str().unwrap();
        assert_eq!(buffered(&["init", "--initial-branch=main", source], "runner-test", &[0]).await.unwrap().code, Some(0));
        std::fs::write(std::path::Path::new(source).join("file.txt"), "fixture\n").unwrap();
        buffered(&["-C", source, "add", "file.txt"], "runner-test", &[0]).await.unwrap();
        assert_eq!(buffered(&["-C", source, "-c", "user.name=Fixture", "-c", "user.email=fixture@example.test", "commit", "-m", "fixture"], "runner-test", &[0]).await.unwrap().code, Some(0));
        let lines = Arc::new(Mutex::new(Vec::new()));
        let observed = lines.clone();
        let callback: Observer = Arc::new(move |stream, text| {
            observed.lock().unwrap().push((stream.to_string(), text.to_string()));
            let cleared = clear_activity();
            assert!(cleared.running.iter().any(|entry| entry.context == "runner-clone"));
        });
        let result = execute(Request { args: &["clone", "--no-local", "--progress", "--", source, clone], context: "runner-clone", expected: &[0], timeout: Duration::from_secs(45), policy: OutputPolicy::Text }, Some(callback)).await.unwrap();
        assert_eq!(result.code, Some(0));
        assert!(lines.lock().unwrap().iter().any(|(stream, text)| stream == "stderr" && text.contains("Receiving objects:")));
        let status = buffered(&["-C", clone, "status", "--branch"], "runner-status", &[0]).await.unwrap();
        assert!(String::from_utf8_lossy(&status.stdout).contains("main"));
        let failure = buffered(&["-C", clone, "not-a-command"], "runner-failure", &[0]).await.unwrap();
        assert_ne!(failure.code, Some(0));
        assert!(!failure.stderr.is_empty());
        buffered(&["-C", clone, "branch", "feature"], "runner-tree-fixture", &[0]).await.unwrap();
        buffered(&["-C", clone, "tag", "v1"], "runner-tree-fixture", &[0]).await.unwrap();
        std::fs::write(std::path::Path::new(clone).join("file.txt"), "stash fixture\n").unwrap();
        buffered(&["-C", clone, "-c", "user.name=Fixture", "-c", "user.email=fixture@example.test", "stash", "push", "-m", "fixture stash"], "runner-tree-fixture", &[0]).await.unwrap();
        let commit = buffered(&["-C", clone, "rev-parse", "HEAD"], "runner-tree-fixture", &[0]).await.unwrap();
        let commit = String::from_utf8_lossy(&commit.stdout).trim().to_string();
        buffered(&["-C", clone, "update-index", "--add", "--cacheinfo", &format!("160000,{commit},nested")], "runner-tree-fixture", &[0]).await.unwrap();
        buffered(&["-C", clone, "config", "remote.origin.url", "https://probe-user:probe-password extra@invalid.test/repo"], "runner-tree-fixture", &[0]).await.unwrap();
        buffered(&["-C", clone, "config", "remote.split.url", "https://probe-user\r\n:probe-password@invalid.test/repo"], "runner-tree-fixture", &[0]).await.unwrap();
        let modules = std::path::Path::new(clone).join(".gitmodules");
        std::fs::write(&modules, "[submodule \"nested\"]\npath = nested\nurl = \"https://probe-user:probe-password\\n extra@invalid.test/module?credential=private\"\n[include]\npath = unavailable-do-not-follow\n").unwrap();
        let observer_lines = Arc::new(Mutex::new(Vec::new()));
        for (name, separator) in [("z-multi-space", " "), ("z-multi-crlf", "\r\n")] {
            let input = format!("https://prefix.test{separator}https://probe-user:probe-password@invalid.test/repo");
            buffered(&["-C", clone, "config", &format!("remote.{name}.url"), &input], &input, &[0]).await.unwrap();
            buffered(&["-C", clone, "update-index", "--add", "--cacheinfo", &format!("160000,{commit},{name}")], "runner-tree-fixture", &[0]).await.unwrap();
            buffered(&["-C", clone, "config", "--no-includes", "--file", modules.to_str().unwrap(), &format!("submodule.{name}.path"), name], "runner-tree-fixture", &[0]).await.unwrap();
            buffered(&["-C", clone, "config", "--no-includes", "--file", modules.to_str().unwrap(), &format!("submodule.{name}.url"), &input], &input, &[0]).await.unwrap();
            let observed = observer_lines.clone();
            let observer: Observer = Arc::new(move |_, text| { observed.lock().unwrap().push(text.to_string()); });
            let error = execute(Request { args: &["-C", clone, &input], context: &input, expected: &[0], timeout: Duration::from_secs(45), policy: OutputPolicy::Text }, Some(observer)).await.unwrap();
            assert_ne!(error.code, Some(0));
            let message = last_error(&String::from_utf8_lossy(&error.stderr));
            assert!(!message.contains("probe-user") && !message.contains("probe-password"), "{message:?}");
        }
        let benign = "https://prefix.test https://invalid.test:443/repo";
        buffered(&["-C", clone, "config", "remote.z-benign.url", benign], "runner-tree-fixture", &[0]).await.unwrap();
        let index = std::fs::read(std::path::Path::new(clone).join(".git/index")).unwrap();
        let head = std::fs::read(std::path::Path::new(clone).join(".git/HEAD")).unwrap();
        let tree = repository_tree(clone.into()).await.unwrap();
        assert!(tree.branches.iter().any(|reference| reference.name == "main" && reference.current));
        assert!(tree.branches.iter().any(|reference| reference.name == "feature"));
        assert_eq!(tree.tags[0].name, "v1");
        assert_eq!(tree.remotes[0].name, "origin");
        assert!(!tree.remotes[0].refs.is_empty());
        assert!(!tree.remotes[0].urls[0].contains("probe-password"));
        assert_eq!(tree.stashes[0].name, "stash@{0}");
        assert_eq!(tree.submodules[0].path, "nested");
        let url = tree.submodules[0].url.as_ref().unwrap();
        assert!(!url.contains("password") && !url.contains("credential=private"));
        let serialized = serde_json::to_string(&tree).unwrap();
        assert!(!serialized.contains("probe-user") && !serialized.contains("probe-password"), "{serialized}");
        for (name, separator) in [("z-multi-space", " "), ("z-multi-crlf", "\r\n")] {
            let separator = if separator == "\r\n" { " \n" } else { separator };
            let expected = format!("https://prefix.test{separator}https://[redacted]@invalid.test/repo");
            let remote = tree.remotes.iter().find(|remote| remote.name == name).unwrap();
            assert_eq!(remote.urls, expected.lines().map(str::to_string).collect::<Vec<_>>());
            let module = tree.submodules.iter().find(|module| module.path == name).unwrap();
            assert_eq!(module.url.as_deref(), Some(expected.as_str()));
        }
        assert_eq!(tree.remotes.iter().find(|remote| remote.name == "z-benign").unwrap().urls, [benign]);
        {
            let observed = observer_lines.lock().unwrap();
            assert!(observed.len() >= 2);
            for text in observed.iter() {
                assert!(!text.contains("probe-user") && !text.contains("probe-password"), "{text:?}");
            }
        }
        let activities = serde_json::to_string(&activity_snapshot()).unwrap();
        assert!(!activities.contains("probe-user") && !activities.contains("probe-password"), "{activities}");
        assert_eq!(std::fs::read(std::path::Path::new(clone).join(".git/index")).unwrap(), index);
        assert_eq!(std::fs::read(std::path::Path::new(clone).join(".git/HEAD")).unwrap(), head);
        assert!(ls_remote(source).await.unwrap().0.iter().any(|(name, _)| name == "main"));
        assert!(activity_snapshot().iter().flat_map(|entry| &entry.output).all(|output| !output.text.contains("credential=private") && !output.text.contains("password")));
        assert!(valid_ref("main..unsafe").is_err());
        assert!(valid_ref("-option").is_err());
        assert!(valid_path(&root.join("../escape").to_string_lossy(), false).is_err());
        assert!(repository_tree(root.join("missing").to_string_lossy().into()).await.is_err());
        let timeout = execute(Request { args: &["status"], context: "runner-timeout", expected: &[0], timeout: Duration::ZERO, policy: OutputPolicy::Text }, None).await;
        assert!(timeout.is_err());
        let requested = Arc::new(AtomicBool::new(false));
        let observed = requested.clone();
        let cancel_callback: Observer = Arc::new(move |_, _| {
            if !observed.swap(true, AtomicOrdering::Relaxed) {
                let entry = activity_snapshot().into_iter().find(|entry| entry.context == "runner-cancel" && entry.state == "running").unwrap();
                assert!(cancel_activity(entry.id));
            }
        });
        let cancelled = execute(Request { args: &["-c", "alias.paperwing-fixture-wait=!echo cancel-ready; while :; do :; done", "paperwing-fixture-wait"], context: "runner-cancel", expected: &[0], timeout: Duration::from_secs(45), policy: OutputPolicy::Text }, Some(cancel_callback)).await;
        assert!(cancelled.is_err());
        assert!(requested.load(AtomicOrdering::Relaxed));
        let entries = activity_snapshot();
        assert_eq!(entries.iter().filter(|entry| entry.context == "runner-clone").count(), 1);
        assert!(entries.iter().any(|entry| entry.context == "runner-failure" && entry.state == "failed"));
        assert!(entries.iter().any(|entry| entry.context == "runner-timeout" && entry.state == "timedOut"));
        assert!(entries.iter().any(|entry| entry.context == "runner-cancel" && entry.state == "cancelled"));
        for entry in entries { assert!(entry.output.windows(2).all(|pair| pair[0].sequence < pair[1].sequence)); }
        assert!(clear_activity().running.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}

fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(c1), Some(c2)) if c1.is_ascii_digit() && c2.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(&c) = it.peek() {
                        if !c.is_ascii_digit() {
                            break;
                        }
                        s.push(c);
                        it.next();
                    }
                    s.trim_start_matches('0').to_string()
                };
                let (n1, n2) = (take(&mut x), take(&mut y));
                let o = n1.len().cmp(&n2.len()).then_with(|| n1.cmp(&n2));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(c1), Some(c2)) => {
                let o = c1.to_ascii_lowercase().cmp(&c2.to_ascii_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                x.next();
                y.next();
            }
        }
    }
}

async fn ls_remote(url: &str) -> Result<(Vec<(String, String)>, Vec<(String, String)>), String> {
    valid_url(url)?;
    let out = buffered(&["ls-remote", "--heads", "--tags", "--", url], "Remote references", &[0]).await?;
    if out.code != Some(0) {
        return Err(last_error(&String::from_utf8_lossy(&out.stderr)));
    }
    let mut branches: Vec<(String, String)> = Vec::new();
    let mut tags: Vec<(String, String)> = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Some((sha, r)) = line.split_once('\t') else { continue };
        if let Some(b) = r.strip_prefix("refs/heads/") {
            branches.push((b.to_string(), sha.to_string()));
        } else if let Some(t) = r.strip_prefix("refs/tags/") {
            // Annotated tags appear twice; the peeled `^{}` line carries the commit SHA.
            let (name, peeled) = t.strip_suffix("^{}").map_or((t, false), |n| (n, true));
            match tags.iter_mut().find(|(n, _)| n == name) {
                Some(e) if peeled => e.1 = sha.to_string(),
                Some(_) => {}
                None => tags.push((name.to_string(), sha.to_string())),
            }
        }
    }
    branches.sort_by(|a, b| natural_cmp(&a.0, &b.0));
    tags.sort_by(|a, b| natural_cmp(&b.0, &a.0));
    Ok((branches, tags))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefsResult {
    url: String,
    branches: Vec<String>,
    tags: Vec<String>,
    branch_shas: Vec<String>,
    tag_shas: Vec<String>,
    error: Option<String>,
}

#[tauri::command]
pub async fn get_refs_many(urls: Vec<String>) -> Vec<RefsResult> {
    let sem = Arc::new(Semaphore::new(8));
    let handles: Vec<_> = urls
        .into_iter()
        .map(|url| {
            let sem = sem.clone();
            tauri::async_runtime::spawn(async move {
                let _permit = sem.acquire_owned().await;
                match ls_remote(&url).await {
                    Ok((b, t)) => {
                        let (branches, branch_shas) = b.into_iter().unzip();
                        let (tags, tag_shas) = t.into_iter().unzip();
                        RefsResult { url, branches, tags, branch_shas, tag_shas, error: None }
                    }
                    Err(e) => RefsResult { url, branches: vec![], tags: vec![], branch_shas: vec![], tag_shas: vec![], error: Some(e) },
                }
            })
        })
        .collect();
    let mut out = Vec::with_capacity(handles.len());
    for h in handles {
        if let Ok(r) = h.await {
            out.push(r);
        }
    }
    out
}
