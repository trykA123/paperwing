use super::filter::{is_relevant, is_watchable_dir, Root};
use super::{Signal, Sink};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub(super) type Handler = Box<dyn FnMut(notify::Result<notify::Event>) + Send>;
pub(super) type Factory = Arc<dyn Fn(Handler) -> Result<Box<dyn Backend>, String> + Send + Sync>;

pub(super) trait Backend: Send {
    fn watch(&mut self, path: &Path, recursive: bool) -> Result<(), String>;
    fn unwatch(&mut self, path: &Path);
}

impl Backend for RecommendedWatcher {
    fn watch(&mut self, path: &Path, recursive: bool) -> Result<(), String> {
        let mode = if recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        Watcher::watch(self, path, mode).map_err(|error| error.to_string())
    }

    fn unwatch(&mut self, path: &Path) {
        let _ = Watcher::unwatch(self, path);
    }
}

pub(super) fn recommended() -> Factory {
    Arc::new(|handler| {
        let config = notify::Config::default().with_follow_symlinks(false);
        RecommendedWatcher::new(handler, config)
            .map(|watcher| Box::new(watcher) as Box<dyn Backend>)
            .map_err(|error| error.to_string())
    })
}

pub(super) struct Timing {
    pub debounce: Duration,
    pub health: Duration,
}

pub(super) struct Spec {
    path: PathBuf,
    recursive: bool,
}

pub(super) struct Prepared {
    root: Arc<Root>,
    plan: Vec<Spec>,
}

enum Message {
    Changed(Arc<Root>),
    Everything,
    NewDirectory(Arc<Root>, PathBuf),
    Removed(PathBuf),
    Stop,
}

type Roots = Arc<RwLock<Vec<Arc<Root>>>>;

struct Core {
    backend: Box<dyn Backend>,
    counts: HashMap<PathBuf, usize>,
}

impl Core {
    fn acquire(&mut self, spec: &Spec) -> Result<(), String> {
        if let Some(count) = self.counts.get_mut(&spec.path) {
            *count += 1;
            return Ok(());
        }
        self.backend.watch(&spec.path, spec.recursive)?;
        self.counts.insert(spec.path.clone(), 1);
        Ok(())
    }

    fn release(&mut self, path: &Path) {
        let Some(count) = self.counts.get_mut(path) else {
            return;
        };
        *count -= 1;
        if *count == 0 {
            self.counts.remove(path);
            self.backend.unwatch(path);
        }
    }

    fn forget(&mut self, path: &Path) {
        if self.counts.remove(path).is_some() {
            self.backend.unwatch(path);
        }
    }

    fn rewatch(&mut self, path: &Path) {
        if self.counts.contains_key(path) {
            let _ = self.backend.watch(path, false);
        }
    }
}

struct Shared {
    core: Mutex<Core>,
    roots: Roots,
    structure: Mutex<()>,
    sink: Sink,
}

pub(super) struct Hub {
    shared: Arc<Shared>,
    sender: Sender<Message>,
    thread: Option<JoinHandle<()>>,
}

fn own_walk() -> bool {
    cfg!(target_os = "linux")
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Shared {
    fn snapshot(&self) -> Vec<Arc<Root>> {
        self.roots
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn is_member(&self, root: &Arc<Root>) -> bool {
        self.snapshot().iter().any(|known| Arc::ptr_eq(known, root))
    }

    fn acquire_all(&self, root: &Root, plan: &[Spec]) -> Result<(), String> {
        let mut core = locked(&self.core);
        let mut dirs = locked(&root.dirs);
        for spec in plan {
            if let Err(reason) = core.acquire(spec) {
                if spec.path != root.path && !spec.path.exists() {
                    continue;
                }
                dirs.drain(..).for_each(|path| core.release(&path));
                return Err(format!("{}: {reason}", spec.path.display()));
            }
            dirs.push(spec.path.clone());
        }
        *locked(&root.identity) = identity(&root.path);
        Ok(())
    }

    fn detach(&self, root: &Root) {
        let dirs = std::mem::take(&mut *locked(&root.dirs));
        let mut core = locked(&self.core);
        dirs.iter().for_each(|path| core.release(path));
    }
}

pub(super) fn prepare(label: &str) -> Result<Prepared, String> {
    let root = Arc::new(Root::new(label, git_dirs(Path::new(label))));
    let plan = plan(&root)?;
    Ok(Prepared { root, plan })
}

impl Hub {
    pub(super) fn new(factory: &Factory, timing: Timing, sink: Sink) -> Result<Self, String> {
        let roots: Roots = Roots::default();
        let (sender, receiver) = channel();
        let handler = handler(roots.clone(), sender.clone(), sink.clone());
        let shared = Arc::new(Shared {
            core: Mutex::new(Core {
                backend: factory(handler)?,
                counts: HashMap::new(),
            }),
            roots,
            structure: Mutex::new(()),
            sink,
        });
        let worker = Worker {
            shared: shared.clone(),
            timing,
        };
        let thread = std::thread::Builder::new()
            .name("skein-watch".into())
            .spawn(move || worker.run(&receiver))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            shared,
            sender,
            thread: Some(thread),
        })
    }

    pub(super) fn commit(&self, prepared: Prepared) -> Result<(), String> {
        let _structure = locked(&self.shared.structure);
        self.shared.acquire_all(&prepared.root, &prepared.plan)?;
        self.shared
            .roots
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(prepared.root);
        Ok(())
    }

    pub(super) fn remove(&self, label: &str) {
        let _structure = locked(&self.shared.structure);
        let taken = {
            let mut roots = self
                .shared
                .roots
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            roots
                .iter()
                .position(|root| root.label == label)
                .map(|index| roots.remove(index))
        };
        if let Some(root) = taken {
            self.shared.detach(&root);
        }
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.shared.snapshot().len()
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        let _ = self.sender.send(Message::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn handler(roots: Roots, sender: Sender<Message>, sink: Sink) -> Handler {
    let reported = AtomicBool::new(false);
    Box::new(move |result| match result {
        Ok(event) if matches!(event.kind, EventKind::Access(_)) => {}
        Ok(event) if event.need_rescan() || event.paths.is_empty() => {
            let _ = sender.send(Message::Everything);
        }
        Ok(event) => route(&roots, &sender, &event),
        Err(error) if !reported.swap(true, Ordering::Relaxed) => sink(Signal::Failed {
            reason: error.to_string(),
        }),
        Err(_) => {}
    })
}

fn route(roots: &Roots, sender: &Sender<Message>, event: &notify::Event) {
    let roots = roots
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let created = matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(notify::event::ModifyKind::Name(_))
    );
    let vanished = matches!(
        event.kind,
        EventKind::Remove(_) | EventKind::Modify(notify::event::ModifyKind::Name(_))
    );
    for path in &event.paths {
        for root in roots.iter() {
            if is_relevant(root, path) {
                let _ = sender.send(Message::Changed(root.clone()));
            }
            if own_walk() && created && is_watchable_dir(root, path) && is_real_directory(path) {
                let _ = sender.send(Message::NewDirectory(root.clone(), path.clone()));
            }
        }
        if own_walk() && vanished && roots.iter().any(|root| is_watchable_dir(root, path)) {
            let _ = sender.send(Message::Removed(path.clone()));
        }
    }
}

fn is_real_directory(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir())
}

#[cfg(unix)]
fn identity(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path)
        .ok()
        .map(|meta| (meta.dev(), meta.ino()))
}

#[cfg(windows)]
fn identity(path: &Path) -> Option<(u64, u64)> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    let handle = std::fs::OpenOptions::new()
        .access_mode(0)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .ok()?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the handle is open for the whole call and `info` is a valid out pointer.
    if unsafe { GetFileInformationByHandle(handle.as_raw_handle().cast(), &mut info) } == 0 {
        return None;
    }
    let index = u64::from(info.nFileIndexHigh) << 32 | u64::from(info.nFileIndexLow);
    Some((u64::from(info.dwVolumeSerialNumber), index))
}

#[cfg(not(any(unix, windows)))]
fn identity(_: &Path) -> Option<(u64, u64)> {
    None
}

fn git_dirs(root: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(root.join(".git")) else {
        return Vec::new();
    };
    let Some(target) = text.trim().strip_prefix("gitdir:") else {
        return Vec::new();
    };
    let Ok(gitdir) = root.join(target.trim()).canonicalize() else {
        return Vec::new();
    };
    let common = std::fs::read_to_string(gitdir.join("commondir"))
        .ok()
        .and_then(|text| gitdir.join(text.trim()).canonicalize().ok());
    std::iter::once(gitdir.clone())
        .chain(common.filter(|dir| *dir != gitdir))
        .collect()
}

fn collect_directories(root: &Root, start: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut found = vec![start.to_path_buf()];
    let mut next = 0;
    while next < found.len() {
        let current = found[next].clone();
        next += 1;
        let entries = match std::fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(error) if next == 1 => return Err(error),
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) && is_watchable_dir(root, &path) {
                found.push(path);
            }
        }
    }
    Ok(found)
}

fn plan(root: &Root) -> Result<Vec<Spec>, String> {
    let describe = |path: &Path, reason: String| format!("{}: {reason}", path.display());
    let mut specs = vec![];
    if !own_walk() {
        specs.push(Spec {
            path: root.path.clone(),
            recursive: true,
        });
        for dir in &root.git_dirs {
            specs.push(Spec {
                path: dir.clone(),
                recursive: false,
            });
            let refs = dir.join("refs");
            if refs.is_dir() {
                specs.push(Spec {
                    path: refs,
                    recursive: true,
                });
            }
        }
        return Ok(specs);
    }
    let mut seen = HashSet::new();
    let starts = std::iter::once(root.path.clone()).chain(root.git_dirs.iter().cloned());
    for start in starts {
        let found = collect_directories(root, &start)
            .map_err(|error| describe(&start, error.to_string()))?;
        for path in found.into_iter().filter(|path| seen.insert(path.clone())) {
            specs.push(Spec {
                path,
                recursive: false,
            });
        }
    }
    Ok(specs)
}

struct Worker {
    shared: Arc<Shared>,
    timing: Timing,
}

impl Worker {
    fn run(&self, receiver: &Receiver<Message>) {
        let mut pending: Vec<Arc<Root>> = Vec::new();
        let mut deadline: Option<Instant> = None;
        let mut next_check = Instant::now() + self.timing.health;
        loop {
            let wake = deadline.map_or(next_check, |due| due.min(next_check));
            match receiver.recv_timeout(wake.saturating_duration_since(Instant::now())) {
                Ok(Message::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Ok(message) => self.collect(message, &mut pending),
                Err(RecvTimeoutError::Timeout) => {}
            }
            let now = Instant::now();
            if !pending.is_empty() && deadline.is_none() {
                deadline = Some(now + self.timing.debounce);
            }
            if deadline.is_some_and(|due| now >= due) {
                deadline = None;
                for root in pending.drain(..) {
                    (self.shared.sink)(Signal::Changed(root.label.clone()));
                }
            }
            if now >= next_check {
                next_check = now + self.timing.health;
                self.check_health(&mut pending);
            }
        }
    }

    fn collect(&self, message: Message, pending: &mut Vec<Arc<Root>>) {
        match message {
            Message::Changed(root) => remember(pending, &root),
            Message::Everything => self
                .shared
                .snapshot()
                .iter()
                .for_each(|root| remember(pending, root)),
            Message::NewDirectory(root, path) => {
                self.watch_new_directory(&root, &path);
                remember(pending, &root);
            }
            Message::Removed(path) => self.prune(&path),
            Message::Stop => {}
        }
    }

    fn watch_new_directory(&self, root: &Arc<Root>, path: &Path) {
        let _structure = locked(&self.shared.structure);
        if !self.shared.is_member(root) {
            return;
        }
        let Ok(found) = collect_directories(root, path) else {
            return;
        };
        let mut core = locked(&self.shared.core);
        let mut dirs = locked(&root.dirs);
        for dir in found {
            if dirs.contains(&dir) {
                core.rewatch(&dir);
                continue;
            }
            let spec = Spec {
                path: dir.clone(),
                recursive: false,
            };
            if core.acquire(&spec).is_ok() {
                dirs.push(dir);
            }
        }
    }

    fn prune(&self, path: &Path) {
        let _structure = locked(&self.shared.structure);
        let mut core = locked(&self.shared.core);
        for root in self.shared.snapshot() {
            let mut dirs = locked(&root.dirs);
            dirs.retain(|dir| {
                let gone = dir.starts_with(path) && !dir.exists();
                if gone {
                    core.forget(dir);
                }
                !gone
            });
        }
    }

    fn check_health(&self, pending: &mut Vec<Arc<Root>>) {
        for root in self.shared.snapshot() {
            let _structure = locked(&self.shared.structure);
            if !self.shared.is_member(&root) {
                continue;
            }
            let lost = root.lost.load(Ordering::Relaxed);
            if !root.path.is_dir() {
                if !lost {
                    self.lose(&root, "the folder is gone");
                }
                continue;
            }
            let replaced = identity(&root.path) != *locked(&root.identity);
            if !lost && !replaced {
                continue;
            }
            self.shared.detach(&root);
            let attached = plan(&root).and_then(|plan| self.shared.acquire_all(&root, &plan));
            match attached {
                Ok(()) => {
                    root.lost.store(false, Ordering::Relaxed);
                    remember(pending, &root);
                }
                Err(reason) if !lost => {
                    self.lose(&root, &format!("it could not be watched again ({reason})"))
                }
                Err(_) => {}
            }
        }
    }

    fn lose(&self, root: &Root, reason: &str) {
        root.lost.store(true, Ordering::Relaxed);
        self.shared.detach(root);
        (self.shared.sink)(Signal::Lost {
            path: root.label.clone(),
            reason: reason.into(),
        });
    }
}

fn remember(pending: &mut Vec<Arc<Root>>, root: &Arc<Root>) {
    if !pending.iter().any(|known| Arc::ptr_eq(known, root)) {
        pending.push(root.clone());
    }
}
