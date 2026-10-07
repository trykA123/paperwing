use super::{filesystem_gate, git, Child};
use crate::git::batch_repository;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, OnceLock, Weak,
};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot, watch, Semaphore};

const READER_LIMIT: usize = 12;
static READERS: Semaphore = Semaphore::const_new(READER_LIMIT);
static OWNERS: OnceLock<Mutex<Vec<Weak<Owner>>>> = OnceLock::new();
const IDLE: Duration = Duration::from_secs(30);

#[cfg(test)]
pub(super) fn resources_idle() -> bool {
    READERS.available_permits() == READER_LIMIT
}

struct RunState {
    cancelled: Arc<AtomicBool>,
    closed: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
}

struct Request {
    object: String,
    reply: oneshot::Sender<Result<Vec<u8>, String>>,
}

#[derive(Clone)]
struct Handle {
    repository: batch_repository::Repository,
    requests: mpsc::Sender<Request>,
    finished: watch::Receiver<bool>,
    failed: Arc<AtomicBool>,
}

struct Owner {
    root: PathBuf,
    cancelled: Arc<AtomicBool>,
    closed: Arc<AtomicBool>,
    handle: Mutex<Option<Handle>>,
    start: Semaphore,
    restarted: AtomicBool,
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Relaxed);
    }
}

#[derive(Clone)]
pub(crate) struct BatchReader(Arc<Owner>);

impl BatchReader {
    pub(crate) async fn shares_directory(&self, root: PathBuf) -> bool {
        let left = self.0.root.clone();
        tokio::task::spawn_blocking(move || {
            batch_repository::resolve(&left)
                .ok()
                .zip(batch_repository::resolve(&root).ok())
                .is_some_and(|(left, right)| left.directory == right.directory)
        })
        .await
        .unwrap_or(false)
    }

    #[cfg(test)]
    pub(crate) fn is_started(&self) -> bool {
        self.0.handle.lock().is_ok_and(|handle| {
            handle
                .as_ref()
                .is_some_and(|handle| !*handle.finished.borrow())
        })
    }

    #[cfg(test)]
    pub(crate) async fn wait_idle(&self) {
        let handle = self.0.handle.lock().unwrap().clone().unwrap();
        wait_finished(handle.finished).await;
    }

    pub(crate) fn new(root: PathBuf, cancelled: Arc<AtomicBool>) -> Self {
        let owner = Arc::new(Owner {
            root,
            cancelled,
            closed: Arc::new(AtomicBool::new(false)),
            handle: Mutex::new(None),
            start: Semaphore::new(1),
            restarted: AtomicBool::new(false),
        });
        if let Ok(mut owners) = OWNERS.get_or_init(|| Mutex::new(Vec::new())).lock() {
            owners.retain(|owner| owner.strong_count() > 0);
            owners.push(Arc::downgrade(&owner));
        }
        Self(owner)
    }

    pub(crate) async fn close_root(root: &Path) -> Result<(), String> {
        let root = root.to_path_buf();
        let owners: Vec<_> = OWNERS
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .map_err(|_| "Batch reader registry unavailable")?
            .iter()
            .filter_map(Weak::upgrade)
            .collect();
        let readers: Vec<_> = tokio::task::spawn_blocking(move || {
            let root = crate::paths::plain(root.canonicalize().unwrap_or(root));
            owners
                .into_iter()
                .filter(|owner| {
                    let path = crate::paths::plain(
                        owner
                            .root
                            .canonicalize()
                            .unwrap_or_else(|_| owner.root.clone()),
                    );
                    path.starts_with(&root)
                        || root.starts_with(&path)
                        || owner.handle.lock().is_ok_and(|handle| {
                            handle.as_ref().is_some_and(|handle| {
                                handle.repository.directory.starts_with(&root)
                            })
                        })
                })
                .map(Self)
                .collect()
        })
        .await
        .map_err(|_| "Batch root validation failed")?;
        futures_util::future::join_all(readers.iter().map(Self::close)).await;
        Ok(())
    }

    pub(crate) async fn read(&self, object: &str) -> Result<Vec<u8>, String> {
        if !matches!(object.len(), 40 | 64) || !object.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("Invalid batch blob identity".into());
        }
        for attempt in 0..2 {
            let handle = self.handle().await?;
            let (reply, result) = oneshot::channel();
            let sent = handle
                .requests
                .send(Request {
                    object: object.into(),
                    reply,
                })
                .await;
            let result = if sent.is_ok() {
                result
                    .await
                    .unwrap_or_else(|_| Err("Batch reader closed".into()))
            } else {
                Err("Batch reader closed".into())
            };
            if result.is_ok() || stopped(&self.0) || attempt == 1 {
                return result;
            }
            wait_finished(handle.finished.clone()).await;
        }
        Err("Batch reader closed".into())
    }

    async fn handle(&self) -> Result<Handle, String> {
        let root = self.0.root.clone();
        let repository = tokio::task::spawn_blocking(move || batch_repository::resolve(&root))
            .await
            .map_err(|_| "Batch root validation failed")??;
        let _starting = self
            .0
            .start
            .acquire()
            .await
            .map_err(|_| "Batch reader closed")?;
        if stopped(&self.0) {
            return Err("Comparison cancelled".into());
        }
        let previous = self
            .0
            .handle
            .lock()
            .map_err(|_| "Batch reader unavailable")?
            .clone();
        if let Some(handle) = previous {
            if handle.repository != repository {
                drop(_starting);
                self.close().await;
                return Err("Batch repository identity changed".into());
            }
            if *handle.finished.borrow()
                && handle.failed.load(Ordering::Relaxed)
                && self.0.restarted.swap(true, Ordering::Relaxed)
            {
                return Err("Batch reader restart exhausted".into());
            }
            if !*handle.finished.borrow() {
                return Ok(handle);
            }
        }
        let handle = start(&self.0, repository).await?;
        *self
            .0
            .handle
            .lock()
            .map_err(|_| "Batch reader unavailable")? = Some(handle.clone());
        Ok(handle)
    }

    pub(crate) async fn close(&self) {
        self.0.closed.store(true, Ordering::Relaxed);
        let _starting = self.0.start.acquire().await;
        let handle = self.0.handle.lock().ok().and_then(|handle| handle.clone());
        if let Some(handle) = handle {
            wait_finished(handle.finished).await;
        }
    }
}

async fn wait_finished(mut finished: watch::Receiver<bool>) {
    while !*finished.borrow_and_update() {
        if finished.changed().await.is_err() {
            break;
        }
    }
}

fn stopped(owner: &Owner) -> bool {
    owner.cancelled.load(Ordering::Relaxed) || owner.closed.load(Ordering::Relaxed)
}

async fn start(owner: &Owner, repository: batch_repository::Repository) -> Result<Handle, String> {
    let slot = READERS
        .try_acquire()
        .map_err(|_| "Batch reader capacity reached")?;
    let _filesystem = loop {
        tokio::select! {
            guard = filesystem_gate().read() => break guard,
            _ = tokio::time::sleep(Duration::from_millis(25)) => if stopped(owner) { return Err("Comparison cancelled".into()); }
        }
    };
    if stopped(owner) {
        return Err("Comparison cancelled".into());
    }
    let child = spawn(&owner.root, &repository).await?;
    let (requests, receiver) = mpsc::channel(1);
    let (done, finished) = watch::channel(false);
    let failed = Arc::new(AtomicBool::new(false));
    let state = RunState {
        failed: failed.clone(),
        cancelled: owner.cancelled.clone(),
        closed: owner.closed.clone(),
    };
    tokio::spawn(async move {
        serve(child, receiver, state).await;
        drop(slot);
        let _ = done.send(true);
    });
    Ok(Handle {
        repository,
        requests,
        finished,
        failed,
    })
}

async fn spawn(root: &Path, repository: &batch_repository::Repository) -> Result<Child, String> {
    let mut command = git();
    command
        .args([
            "--no-pager",
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=",
            "-c",
            "core.deltaBaseCacheLimit=1m",
            "-c",
            "core.packedGitWindowSize=1m",
            "-c",
            "core.packedGitLimit=8m",
        ])
        .arg("--git-dir")
        .arg(&repository.directory)
        .arg("--work-tree")
        .arg(root)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(feature = "benchmark")]
    crate::benchmark::command("cat-file");
    #[cfg(target_os = "linux")]
    let child = Child::spawn(&mut command, None)
        .await
        .map_err(|_| "Could not start batch reader")?;
    #[cfg(not(target_os = "linux"))]
    let child = command
        .spawn()
        .map_err(|_| "Could not start batch reader")?;
    Ok(child)
}

async fn serve(mut child: Child, mut receiver: mpsc::Receiver<Request>, state: RunState) {
    if let Some((mut stdin, stdout)) = child.stdin.take().zip(child.stdout.take()) {
        let mut stdout = BufReader::new(stdout);
        let mut tick = tokio::time::interval(Duration::from_millis(25));
        let mut last = tokio::time::Instant::now();
        loop {
            let request = tokio::select! {
                request = receiver.recv() => match request { Some(request) => request, None => break },
                _ = tick.tick() => {
                    if state.cancelled.load(Ordering::Relaxed) || state.closed.load(Ordering::Relaxed) || last.elapsed() >= IDLE { break; }
                    continue;
                }
            };
            let read = async {
                let _filesystem = filesystem_gate().read().await;
                read_blob(&mut stdin, &mut stdout, &request.object).await
            };
            tokio::pin!(read);
            let timeout = tokio::time::sleep(Duration::from_secs(45));
            tokio::pin!(timeout);
            let result = loop {
                tokio::select! {
                    result = &mut read => break result,
                    _ = &mut timeout => break Err("Batch blob read timed out".into()),
                    _ = tick.tick() => if state.cancelled.load(Ordering::Relaxed) || state.closed.load(Ordering::Relaxed) { break Err("Comparison cancelled".into()); }
                }
            };
            let failed = result.is_err();
            if failed {
                state.failed.store(true, Ordering::Relaxed);
            }
            let _ = request.reply.send(result);
            if failed {
                break;
            }
            last = tokio::time::Instant::now();
        }
        drop(stdin);
    }
    shutdown(&mut child).await;
}

async fn shutdown(child: &mut Child) {
    if matches!(
        tokio::time::timeout(Duration::from_millis(500), child.wait()).await,
        Ok(Ok(_))
    ) {
        return;
    }
    #[cfg(windows)]
    if child.try_wait().is_ok_and(|status| status.is_none()) {
        if let Some(pid) = child.id() {
            let mut command = tokio::process::Command::new("taskkill");
            command
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .creation_flags(0x0800_0000)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true);
            let _ = tokio::time::timeout(Duration::from_secs(2), command.status()).await;
        }
    }
    let _ = child.start_kill();
    let _ = child.wait().await;
}
async fn read_blob(
    stdin: &mut tokio::process::ChildStdin,
    stdout: &mut BufReader<tokio::process::ChildStdout>,
    object: &str,
) -> Result<Vec<u8>, String> {
    stdin
        .write_all(format!("{object}\n").as_bytes())
        .await
        .map_err(|_| "Could not request batch blob")?;
    stdin
        .flush()
        .await
        .map_err(|_| "Could not flush batch request")?;
    let mut header = Vec::new();
    (&mut *stdout)
        .take(256)
        .read_until(b'\n', &mut header)
        .await
        .map_err(|_| "Could not read batch header")?;
    let header = std::str::from_utf8(&header).map_err(|_| "Invalid batch header")?;
    let fields: Vec<_> = header.split_whitespace().collect();
    if !header.ends_with('\n') || fields.len() != 3 || fields[0] != object || fields[1] != "blob" {
        return Err("Unexpected batch blob".into());
    }
    let size: usize = fields[2].parse().map_err(|_| "Invalid batch size")?;
    if size > crate::paths::CONTENT_LIMIT {
        return Err("Content exceeds read limit".into());
    }
    let mut bytes = vec![0; size];
    stdout
        .read_exact(&mut bytes)
        .await
        .map_err(|_| "Could not read batch blob")?;
    if stdout
        .read_u8()
        .await
        .map_err(|_| "Missing batch delimiter")?
        != b'\n'
    {
        return Err("Invalid batch delimiter".into());
    }
    Ok(bytes)
}
