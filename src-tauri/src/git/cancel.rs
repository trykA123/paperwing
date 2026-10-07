use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;

const WATCH_INTERVAL: Duration = Duration::from_millis(25);
const CANCELLED: &str = "Git command cancelled";

pub(super) struct Stop {
    owner: AtomicBool,
    wake: Notify,
}

impl Stop {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            owner: AtomicBool::new(false),
            wake: Notify::new(),
        })
    }

    pub(super) fn request(&self) {
        self.owner.store(true, Ordering::Relaxed);
        self.wake.notify_one();
    }

    pub(super) fn wake(&self) {
        self.wake.notify_one();
    }

    pub(super) fn owner_requested(&self) -> bool {
        self.owner.load(Ordering::Relaxed)
    }
}

struct Watch {
    id: u64,
    external: Arc<AtomicBool>,
    stop: Arc<Stop>,
}

#[derive(Default)]
struct Watcher {
    watches: Vec<Watch>,
    next: u64,
    running: bool,
}

static WATCHER: Mutex<Watcher> = Mutex::new(Watcher {
    watches: Vec::new(),
    next: 0,
    running: false,
});

pub(super) struct WatchGuard(Option<u64>);

impl Drop for WatchGuard {
    fn drop(&mut self) {
        if let Some(id) = self.0 {
            WATCHER
                .lock()
                .unwrap()
                .watches
                .retain(|watch| watch.id != id);
        }
    }
}

pub(super) fn watch(external: Option<&Arc<AtomicBool>>, stop: &Arc<Stop>) -> WatchGuard {
    let Some(external) = external else {
        return WatchGuard(None);
    };
    let mut watcher = WATCHER.lock().unwrap();
    let id = watcher.next;
    watcher.next += 1;
    watcher.watches.push(Watch {
        id,
        external: external.clone(),
        stop: stop.clone(),
    });
    if !watcher.running {
        watcher.running = true;
        std::thread::spawn(poll_watches);
    }
    WatchGuard(Some(id))
}

fn poll_watches() {
    loop {
        std::thread::sleep(WATCH_INTERVAL);
        let mut watcher = WATCHER.lock().unwrap();
        watcher.watches.retain(|watch| {
            let requested = watch.external.load(Ordering::Relaxed);
            if requested {
                watch.stop.wake();
            }
            !requested
        });
        if watcher.watches.is_empty() {
            watcher.running = false;
            return;
        }
    }
}

pub(super) fn is_cancelled(external: &Option<Arc<AtomicBool>>, stop: &Stop) -> bool {
    stop.owner_requested()
        || external
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
}

pub(super) async fn until(stop: &Stop, requested: impl Fn() -> bool) {
    loop {
        if requested() {
            return;
        }
        stop.wake.notified().await;
    }
}

pub(super) async fn admitted<T>(
    wait: impl Future<Output = T>,
    external: &Option<Arc<AtomicBool>>,
    stop: &Stop,
) -> Result<T, String> {
    tokio::select! {
        value = wait => Ok(value),
        _ = until(stop, || is_cancelled(external, stop)) => Err(CANCELLED.into()),
    }
}

#[cfg(test)]
#[path = "cancel_tests.rs"]
mod tests;
