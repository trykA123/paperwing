use super::filter::is_relevant;
use super::{Signal, Sink};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub(super) type Handler = Box<dyn FnMut(notify::Result<notify::Event>) + Send>;
pub(super) type Factory = Arc<dyn Fn(Handler) -> Result<Box<dyn Backend>, String> + Send + Sync>;

pub(super) trait Backend: Send {
    fn watch(&mut self, path: &Path) -> Result<(), String>;
}

impl Backend for RecommendedWatcher {
    fn watch(&mut self, path: &Path) -> Result<(), String> {
        Watcher::watch(self, path, RecursiveMode::Recursive).map_err(|error| error.to_string())
    }
}

pub(super) fn recommended() -> Factory {
    Arc::new(|handler| {
        RecommendedWatcher::new(handler, notify::Config::default())
            .map(|watcher| Box::new(watcher) as Box<dyn Backend>)
            .map_err(|error| error.to_string())
    })
}

pub(super) struct Root {
    pub path: PathBuf,
    pub label: String,
}

pub(super) struct Session {
    backend: Option<Box<dyn Backend>>,
    stop: Sender<Option<usize>>,
    thread: Option<JoinHandle<()>>,
}

pub(super) struct Options {
    pub set: String,
    pub debounce: Duration,
    pub sink: Sink,
}

impl Session {
    pub(super) fn start(
        roots: Vec<Root>,
        options: Options,
        factory: &Factory,
    ) -> Result<Self, String> {
        let roots: Arc<[Root]> = roots.into();
        let (sender, receiver) = channel::<Option<usize>>();
        let handler = handler(roots.clone(), sender.clone(), &options);
        let mut backend = factory(handler)?;
        for root in roots.iter() {
            backend
                .watch(&root.path)
                .map_err(|reason| format!("{}: {reason}", root.path.display()))?;
        }
        let Options { debounce, sink, .. } = options;
        let thread = std::thread::Builder::new()
            .name("skein-watch".into())
            .spawn(move || debounce_loop(&receiver, debounce, &roots, &sink))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            backend: Some(backend),
            stop: sender,
            thread: Some(thread),
        })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.backend.take();
        let _ = self.stop.send(None);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn handler(roots: Arc<[Root]>, sender: Sender<Option<usize>>, options: &Options) -> Handler {
    let reported = AtomicBool::new(false);
    let set = options.set.clone();
    let sink = options.sink.clone();
    Box::new(move |result| match result {
        Ok(event) if matches!(event.kind, EventKind::Access(_)) => {}
        Ok(event) => {
            for path in &event.paths {
                for (index, root) in roots.iter().enumerate() {
                    if is_relevant(&root.path, path) {
                        let _ = sender.send(Some(index));
                    }
                }
            }
        }
        Err(error) if !reported.swap(true, Ordering::Relaxed) => sink(Signal::Failed {
            set: set.clone(),
            reason: error.to_string(),
        }),
        Err(_) => {}
    })
}

fn debounce_loop(
    receiver: &std::sync::mpsc::Receiver<Option<usize>>,
    debounce: Duration,
    roots: &[Root],
    sink: &Sink,
) {
    let mut pending = Vec::new();
    let mut deadline = Instant::now();
    loop {
        let message = if pending.is_empty() {
            receiver.recv().map_err(|_| RecvTimeoutError::Disconnected)
        } else {
            receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        };
        match message {
            Ok(Some(index)) => {
                if pending.is_empty() {
                    deadline = Instant::now() + debounce;
                }
                if !pending.contains(&index) {
                    pending.push(index);
                }
            }
            Ok(None) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => {
                for index in pending.drain(..) {
                    sink(Signal::Changed(roots[index].label.clone()));
                }
            }
        }
    }
}
