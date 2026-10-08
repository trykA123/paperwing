use super::filter::{is_relevant, is_watchable_dir, Root};
use super::{Signal, Sink};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, RwLock};
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

enum Message {
    Changed(Arc<Root>),
    Everything,
    NewDirectory(Arc<Root>, PathBuf),
    Stop,
}

type Roots = Arc<RwLock<Vec<Arc<Root>>>>;
type Shared = Arc<Mutex<Box<dyn Backend>>>;

pub(super) struct Hub {
    backend: Shared,
    roots: Roots,
    sender: Sender<Message>,
    thread: Option<JoinHandle<()>>,
}

fn own_walk() -> bool {
    cfg!(target_os = "linux")
}

impl Hub {
    pub(super) fn new(factory: &Factory, timing: Timing, sink: Sink) -> Result<Self, String> {
        let roots: Roots = Roots::default();
        let (sender, receiver) = channel();
        let handler = handler(roots.clone(), sender.clone(), sink.clone());
        let backend: Shared = Arc::new(Mutex::new(factory(handler)?));
        let worker = Worker {
            backend: backend.clone(),
            roots: roots.clone(),
            sink,
            timing,
        };
        let thread = std::thread::Builder::new()
            .name("skein-watch".into())
            .spawn(move || worker.run(&receiver))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            backend,
            roots,
            sender,
            thread: Some(thread),
        })
    }

    pub(super) fn add(&self, label: &str) -> Result<(), String> {
        let root = Arc::new(Root::new(label, git_dirs(Path::new(label))));
        attach(&mut **lock(&self.backend), &root)?;
        self.roots
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(root);
        Ok(())
    }

    pub(super) fn remove(&self, label: &str) {
        let mut roots = self
            .roots
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(index) = roots.iter().position(|root| root.label == label) {
            let root = roots.remove(index);
            detach(&mut **lock(&self.backend), &root);
        }
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.roots
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
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

fn lock(backend: &Shared) -> std::sync::MutexGuard<'_, Box<dyn Backend>> {
    backend
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
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
    let may_add_directory = own_walk()
        && matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(notify::event::ModifyKind::Name(_))
        );
    for path in &event.paths {
        for root in roots.iter() {
            if is_relevant(root, path) {
                let _ = sender.send(Message::Changed(root.clone()));
            }
            if may_add_directory && is_watchable_dir(root, path) && is_real_directory(path) {
                let _ = sender.send(Message::NewDirectory(root.clone(), path.clone()));
            }
        }
    }
}

fn is_real_directory(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir())
}

fn git_dirs(root: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(root.join(".git")) else {
        return Vec::new();
    };
    let Some(target) = text.trim().strip_prefix("gitdir:") else {
        return Vec::new();
    };
    let gitdir = root.join(target.trim());
    let Ok(gitdir) = gitdir.canonicalize() else {
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

fn attach(backend: &mut dyn Backend, root: &Root) -> Result<(), String> {
    let mut added: Vec<PathBuf> = Vec::new();
    let result = attach_into(backend, root, &mut added);
    if result.is_err() {
        added.iter().for_each(|path| backend.unwatch(path));
        return result;
    }
    *root
        .dirs
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = added;
    Ok(())
}

fn attach_into(
    backend: &mut dyn Backend,
    root: &Root,
    added: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let describe = |path: &Path, reason: String| format!("{}: {reason}", path.display());
    if !own_walk() {
        backend
            .watch(&root.path, true)
            .map_err(|reason| describe(&root.path, reason))?;
        added.push(root.path.clone());
        for dir in &root.git_dirs {
            backend
                .watch(dir, false)
                .map_err(|reason| describe(dir, reason))?;
            added.push(dir.clone());
            let refs = dir.join("refs");
            if refs.is_dir() && backend.watch(&refs, true).is_ok() {
                added.push(refs);
            }
        }
        return Ok(());
    }
    let starts = std::iter::once(root.path.clone()).chain(root.git_dirs.iter().cloned());
    for start in starts {
        for dir in collect_directories(root, &start)
            .map_err(|error| describe(&start, error.to_string()))?
        {
            backend
                .watch(&dir, false)
                .map_err(|reason| describe(&dir, reason))?;
            added.push(dir);
        }
    }
    Ok(())
}

fn detach(backend: &mut dyn Backend, root: &Root) {
    let dirs = std::mem::take(
        &mut *root
            .dirs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
    );
    dirs.iter().for_each(|path| backend.unwatch(path));
}

struct Worker {
    backend: Shared,
    roots: Roots,
    sink: Sink,
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
                    (self.sink)(Signal::Changed(root.label.clone()));
                }
            }
            if now >= next_check {
                next_check = now + self.timing.health;
                self.check_health(&mut pending);
            }
        }
    }

    fn snapshot(&self) -> Vec<Arc<Root>> {
        self.roots
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn collect(&self, message: Message, pending: &mut Vec<Arc<Root>>) {
        match message {
            Message::Changed(root) => remember(pending, &root),
            Message::Everything => self
                .snapshot()
                .iter()
                .for_each(|root| remember(pending, root)),
            Message::NewDirectory(root, path) => {
                self.watch_new_directory(&root, &path);
                remember(pending, &root);
            }
            Message::Stop => {}
        }
    }

    fn watch_new_directory(&self, root: &Root, path: &Path) {
        let Ok(found) = collect_directories(root, path) else {
            return;
        };
        let mut backend = lock(&self.backend);
        let mut dirs = root
            .dirs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for dir in found {
            if !dirs.contains(&dir) && backend.watch(&dir, false).is_ok() {
                dirs.push(dir);
            }
        }
    }

    fn check_health(&self, pending: &mut Vec<Arc<Root>>) {
        for root in self.snapshot() {
            let present = root.path.is_dir();
            let lost = root.lost.load(Ordering::Relaxed);
            if !present && !lost {
                root.lost.store(true, Ordering::Relaxed);
                detach(&mut **lock(&self.backend), &root);
                (self.sink)(Signal::Lost {
                    path: root.label.clone(),
                    reason: "the folder is gone".into(),
                });
            } else if present && lost && attach(&mut **lock(&self.backend), &root).is_ok() {
                root.lost.store(false, Ordering::Relaxed);
                remember(pending, &root);
            }
        }
    }
}

fn remember(pending: &mut Vec<Arc<Root>>, root: &Arc<Root>) {
    if !pending.iter().any(|known| Arc::ptr_eq(known, root)) {
        pending.push(root.clone());
    }
}
