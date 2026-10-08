use super::session::{Backend, Factory, Handler};
use super::*;
use crate::platform::Fixture;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::sync::OnceLock;

const WINDOW: Duration = Duration::from_millis(120);
const WAIT: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_millis(500);

fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn service(factory: Factory) -> (Service, Receiver<Signal>) {
    let (sender, receiver) = channel();
    let sender = Mutex::new(sender);
    let sink: Sink = Arc::new(move |signal| {
        let _ = sender.lock().unwrap().send(signal);
    });
    (Service::with_parts(sink, factory, WINDOW), receiver)
}

fn repository(fixture: &Fixture, name: &str) -> String {
    let root = fixture.0.join(name);
    for directory in [
        ".git/objects",
        ".git/refs/heads",
        "src",
        "node_modules",
        "target",
    ] {
        std::fs::create_dir_all(root.join(directory)).unwrap();
    }
    root.to_str().unwrap().to_string()
}

fn write(root: &str, relative: &str, text: &str) {
    std::fs::write(Path::new(root).join(relative), text).unwrap();
}

fn drain(receiver: &Receiver<Signal>, quiet: Duration) -> Vec<Signal> {
    let mut seen = Vec::new();
    loop {
        match receiver.recv_timeout(quiet) {
            Ok(signal) => seen.push(signal),
            Err(RecvTimeoutError::Timeout) => return seen,
            Err(RecvTimeoutError::Disconnected) => panic!("sink closed"),
        }
    }
}

#[cfg(target_os = "linux")]
fn inotify_descriptors() -> usize {
    std::fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(|entry| std::fs::read_link(entry.ok()?.path()).ok())
        .filter(|target| target.to_string_lossy().contains("inotify"))
        .count()
}

#[test]
fn a_burst_of_writes_becomes_one_change() {
    let _guard = serial();
    let fixture = Fixture::new("watch-burst");
    let root = repository(&fixture, "repo");
    let (service, receiver) = service(session::recommended());
    service.watch("set", std::slice::from_ref(&root)).unwrap();
    for number in 0..25 {
        write(&root, "src/a.txt", &number.to_string());
        write(&root, &format!("src/f{number}.txt"), "x");
    }
    let first = receiver.recv_timeout(WAIT).unwrap();
    assert_eq!(first, Signal::Changed(root.clone()));
    assert_eq!(drain(&receiver, QUIET), Vec::<Signal>::new());
    service.stop_all();
}

#[test]
fn ignored_paths_stay_quiet_and_git_signals_get_through() {
    let _guard = serial();
    let fixture = Fixture::new("watch-ignore");
    let root = repository(&fixture, "repo");
    let (service, receiver) = service(session::recommended());
    service.watch("set", std::slice::from_ref(&root)).unwrap();
    write(&root, "node_modules/a.js", "x");
    write(&root, "target/a.o", "x");
    write(&root, ".git/objects/blob", "x");
    write(&root, ".git/index.lock", "x");
    assert_eq!(drain(&receiver, QUIET), Vec::<Signal>::new());
    write(&root, ".git/HEAD", "ref: refs/heads/main\n");
    assert_eq!(
        receiver.recv_timeout(WAIT).unwrap(),
        Signal::Changed(root.clone())
    );
    assert_eq!(drain(&receiver, QUIET), Vec::<Signal>::new());
    write(&root, ".git/refs/heads/main", "0\n");
    assert_eq!(receiver.recv_timeout(WAIT).unwrap(), Signal::Changed(root));
    service.stop_all();
}

#[test]
fn only_the_repository_that_changed_is_reported() {
    let _guard = serial();
    let fixture = Fixture::new("watch-two");
    let (first, second) = (repository(&fixture, "one"), repository(&fixture, "two"));
    let (service, receiver) = service(session::recommended());
    service.watch("set", &[first, second.clone()]).unwrap();
    write(&second, "src/a.txt", "x");
    assert_eq!(
        receiver.recv_timeout(WAIT).unwrap(),
        Signal::Changed(second)
    );
    assert_eq!(drain(&receiver, QUIET), Vec::<Signal>::new());
    service.stop_all();
}

#[cfg(target_os = "linux")]
#[test]
fn stopping_a_watch_releases_its_descriptors() {
    let _guard = serial();
    let fixture = Fixture::new("watch-fds");
    let root = repository(&fixture, "repo");
    let (service, _receiver) = service(session::recommended());
    let before = inotify_descriptors();
    service.watch("set", &[root]).unwrap();
    assert!(inotify_descriptors() > before);
    service.unwatch("set");
    let deadline = std::time::Instant::now() + WAIT;
    while inotify_descriptors() > before && std::time::Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert_eq!(inotify_descriptors(), before);
}

#[test]
fn closing_sets_and_the_app_brings_the_watcher_count_to_zero() {
    let _guard = serial();
    let fixture = Fixture::new("watch-count");
    let (one, two) = (repository(&fixture, "one"), repository(&fixture, "two"));
    let (service, _receiver) = service(session::recommended());
    service.watch("a", std::slice::from_ref(&one)).unwrap();
    service.watch("b", std::slice::from_ref(&two)).unwrap();
    assert_eq!(service.count(), 2);
    service.watch("a", &[one, two]).unwrap();
    assert_eq!(service.count(), 2);
    service.unwatch("a");
    assert_eq!(service.count(), 1);
    service.watch("b", &[]).unwrap();
    assert_eq!(service.count(), 0);
    service
        .watch("a", &[repository(&fixture, "three")])
        .unwrap();
    service.stop_all();
    assert_eq!(service.count(), 0);
}

#[test]
fn too_many_roots_is_refused_and_leaves_nothing_watched() {
    let _guard = serial();
    let (service, _receiver) = service(session::recommended());
    let roots: Vec<String> = (0..=MAX_ROOTS).map(|n| format!("/missing/{n}")).collect();
    let error = service.watch("set", &roots).unwrap_err();
    assert_eq!(
        error,
        WatchError::TooManyRoots {
            count: MAX_ROOTS + 1,
            limit: MAX_ROOTS
        }
    );
    assert_eq!(service.count(), 0);
}

struct Silent;

impl Backend for Silent {
    fn watch(&mut self, _: &Path) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn a_watcher_that_cannot_start_reports_and_holds_nothing() {
    let _guard = serial();
    let factory: Factory = Arc::new(|_| Err("no inotify instances left".to_string()));
    let (service, _receiver) = service(factory);
    let error = service
        .watch("set", &["/work/repo".to_string()])
        .unwrap_err();
    assert_eq!(
        error,
        WatchError::Start("no inotify instances left".to_string())
    );
    assert_eq!(service.count(), 0);
}

#[test]
fn a_watcher_error_after_start_is_reported_once() {
    let _guard = serial();
    let slot: Arc<Mutex<Option<Handler>>> = Arc::default();
    let held = slot.clone();
    let factory: Factory = Arc::new(move |handler| {
        *held.lock().unwrap() = Some(handler);
        Ok(Box::new(Silent) as Box<dyn Backend>)
    });
    let (service, receiver) = service(factory);
    service.watch("set", &["/work/repo".to_string()]).unwrap();
    let mut handler = slot.lock().unwrap().take().unwrap();
    for _ in 0..3 {
        handler(Err(notify::Error::generic("overflow")));
    }
    let seen = drain(&receiver, QUIET);
    assert_eq!(seen.len(), 1);
    assert!(matches!(&seen[0], Signal::Failed { set, .. } if set == "set"));
    service.stop_all();
}
