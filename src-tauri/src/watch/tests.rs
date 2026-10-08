use super::hub::{Backend, Factory, Handler};
use super::*;
use crate::platform::Fixture;
use notify::event::{EventKind, Flag};
use std::path::{Path, PathBuf};
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
    service_with(factory, Duration::from_secs(60))
}

fn service_with(factory: Factory, health: Duration) -> (Service, Receiver<Signal>) {
    let (sender, receiver) = channel();
    let sender = Mutex::new(sender);
    let sink: Sink = Arc::new(move |signal| {
        let _ = sender.lock().unwrap().send(signal);
    });
    (Service::with_parts(sink, factory, WINDOW, health), receiver)
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
    let (service, receiver) = service(hub::recommended());
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
    let (service, receiver) = service(hub::recommended());
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
    let (service, receiver) = service(hub::recommended());
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
    let (service, _receiver) = service(hub::recommended());
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
    let (service, _receiver) = service(hub::recommended());
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
    let (service, _receiver) = service(hub::recommended());
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
    fn watch(&mut self, _: &Path, _: bool) -> Result<(), String> {
        Ok(())
    }

    fn unwatch(&mut self, _: &Path) {}
}

type Calls = Arc<Mutex<Vec<(PathBuf, bool)>>>;

struct Recording(Calls);

impl Backend for Recording {
    fn watch(&mut self, path: &Path, recursive: bool) -> Result<(), String> {
        self.0.lock().unwrap().push((path.to_path_buf(), recursive));
        Ok(())
    }

    fn unwatch(&mut self, _: &Path) {}
}

fn injected() -> (Factory, Arc<Mutex<Option<Handler>>>) {
    let slot: Arc<Mutex<Option<Handler>>> = Arc::default();
    let held = slot.clone();
    let factory: Factory = Arc::new(move |handler| {
        *held.lock().unwrap() = Some(handler);
        Ok(Box::new(Silent) as Box<dyn Backend>)
    });
    (factory, slot)
}

#[test]
fn a_watcher_that_cannot_start_reports_and_holds_nothing() {
    let _guard = serial();
    let factory: Factory = Arc::new(|_| Err("no inotify instances left".to_string()));
    let (service, _receiver) = service(factory);
    let fixture = Fixture::new("watch-start");
    let root = repository(&fixture, "repo");
    let error = service.watch("set", &[root]).unwrap_err();
    assert_eq!(
        error,
        WatchError::Start("no inotify instances left".to_string())
    );
    assert_eq!(service.count(), 0);
}

#[test]
fn a_watcher_error_after_start_is_reported_once() {
    let _guard = serial();
    let (factory, slot) = injected();
    let (service, receiver) = service(factory);
    let fixture = Fixture::new("watch-error");
    service
        .watch("set", &[repository(&fixture, "repo")])
        .unwrap();
    let mut handler = slot.lock().unwrap().take().unwrap();
    for _ in 0..3 {
        handler(Err(notify::Error::generic("overflow")));
    }
    let seen = drain(&receiver, QUIET);
    assert_eq!(seen.len(), 1);
    assert!(matches!(&seen[0], Signal::Failed { .. }));
    service.stop_all();
}

#[test]
fn a_rescan_request_or_an_event_without_paths_marks_every_root_changed() {
    let _guard = serial();
    let fixture = Fixture::new("watch-rescan");
    let (first, second) = (repository(&fixture, "one"), repository(&fixture, "two"));
    let (factory, slot) = injected();
    let (service, receiver) = service(factory);
    service
        .watch("set", &[first.clone(), second.clone()])
        .unwrap();
    let mut handler = slot.lock().unwrap().take().unwrap();
    handler(Ok(
        notify::Event::new(EventKind::Other).set_flag(Flag::Rescan)
    ));
    let mut seen = drain(&receiver, QUIET);
    seen.sort_by_key(|signal| format!("{signal:?}"));
    assert_eq!(
        seen,
        vec![
            Signal::Changed(first.clone()),
            Signal::Changed(second.clone())
        ]
    );
    handler(Ok(notify::Event::new(EventKind::Any)));
    assert_eq!(drain(&receiver, QUIET).len(), 2);
    service.stop_all();
}

#[test]
fn one_bad_root_is_skipped_and_the_rest_are_watched() {
    let _guard = serial();
    let fixture = Fixture::new("watch-skip");
    let good = repository(&fixture, "good");
    let (service, _receiver) = service(hub::recommended());
    let missing = fixture.0.join("missing").to_str().unwrap().to_string();
    let report = service
        .watch("set", &[missing.clone(), good, "/".to_string()])
        .unwrap();
    assert_eq!(report.watched, 1);
    let skipped: Vec<&str> = report
        .skipped
        .iter()
        .map(|entry| entry.path.as_str())
        .collect();
    assert_eq!(skipped, vec![missing.as_str(), "/"]);
    assert_eq!(service.count(), 1);
    service.stop_all();
}

#[test]
fn the_home_folder_is_refused() {
    let fixture = Fixture::new("watch-home");
    let home = repository(&fixture, "home");
    let inside = repository(&fixture, "home/work");
    assert!(refusal_in(&home, Some(Path::new(&home))).is_some());
    assert_eq!(refusal_in(&inside, Some(Path::new(&home))), None);
}

#[test]
fn network_and_cloud_sync_locations_are_flagged_as_best_effort() {
    assert!(is_best_effort_location(Path::new(
        "/home/a/OneDrive - Acme/repo"
    )));
    assert!(is_best_effort_location(Path::new("/home/a/onedrive/repo")));
    assert!(!is_best_effort_location(Path::new("/home/a/work/repo")));
}

#[cfg(target_os = "linux")]
#[test]
fn ignored_directories_never_get_a_watch() {
    let _guard = serial();
    let fixture = Fixture::new("watch-walk");
    let root = repository(&fixture, "repo");
    std::fs::create_dir_all(Path::new(&root).join("web/node_modules/deep")).unwrap();
    std::fs::create_dir_all(Path::new(&root).join("src/inner")).unwrap();
    let calls: Calls = Arc::default();
    let seen = calls.clone();
    let factory: Factory =
        Arc::new(move |_| Ok(Box::new(Recording(seen.clone())) as Box<dyn Backend>));
    let (service, _receiver) = service(factory);
    service.watch("set", std::slice::from_ref(&root)).unwrap();
    let calls = calls.lock().unwrap();
    let watched: Vec<String> = calls
        .iter()
        .map(|(path, _)| path.to_string_lossy().into_owned())
        .collect();
    assert!(calls.iter().all(|(_, recursive)| !recursive));
    assert!(watched.iter().any(|path| path.ends_with("src/inner")));
    assert!(watched.iter().any(|path| path.ends_with(".git/refs/heads")));
    assert!(!watched.iter().any(|path| path.contains("node_modules")
        || path.contains("/target")
        || path.contains(".git/objects")));
    drop(calls);
    service.stop_all();
}

#[cfg(target_os = "linux")]
#[test]
fn a_directory_created_later_is_watched() {
    let _guard = serial();
    let fixture = Fixture::new("watch-new-dir");
    let root = repository(&fixture, "repo");
    let (service, receiver) = service(hub::recommended());
    service.watch("set", std::slice::from_ref(&root)).unwrap();
    std::fs::create_dir_all(Path::new(&root).join("src/fresh/deeper")).unwrap();
    assert_eq!(
        receiver.recv_timeout(WAIT).unwrap(),
        Signal::Changed(root.clone())
    );
    drain(&receiver, QUIET);
    write(&root, "src/fresh/deeper/a.txt", "x");
    assert_eq!(receiver.recv_timeout(WAIT).unwrap(), Signal::Changed(root));
    service.stop_all();
}

#[test]
fn a_linked_worktree_reports_its_gitdir_and_the_shared_refs() {
    let _guard = serial();
    let fixture = Fixture::new("watch-linked");
    let main = repository(&fixture, "main");
    let tree = fixture.0.join("wt");
    std::fs::create_dir_all(&tree).unwrap();
    let gitdir = Path::new(&main).join(".git/worktrees/wt");
    std::fs::create_dir_all(&gitdir).unwrap();
    std::fs::write(gitdir.join("commondir"), "../..\n").unwrap();
    std::fs::write(tree.join(".git"), format!("gitdir: {}\n", gitdir.display())).unwrap();
    let root = tree.to_str().unwrap().to_string();
    let (service, receiver) = service(hub::recommended());
    service.watch("set", std::slice::from_ref(&root)).unwrap();
    std::fs::write(gitdir.join("HEAD"), "ref: refs/heads/x\n").unwrap();
    assert_eq!(
        receiver.recv_timeout(WAIT).unwrap(),
        Signal::Changed(root.clone())
    );
    drain(&receiver, QUIET);
    write(&main, ".git/refs/heads/x", "0\n");
    assert_eq!(receiver.recv_timeout(WAIT).unwrap(), Signal::Changed(root));
    service.stop_all();
}

#[test]
fn a_repository_in_several_sets_has_one_watch() {
    let _guard = serial();
    let fixture = Fixture::new("watch-shared");
    let root = repository(&fixture, "repo");
    let other = repository(&fixture, "other");
    let (service, _receiver) = service(hub::recommended());
    service.watch("a", std::slice::from_ref(&root)).unwrap();
    service.watch("b", &[root.clone(), other]).unwrap();
    assert_eq!(service.root_count(), 2);
    service.unwatch("a");
    assert_eq!(service.root_count(), 2);
    service.watch("b", std::slice::from_ref(&root)).unwrap();
    assert_eq!(service.root_count(), 1);
    service.unwatch("b");
    assert_eq!((service.count(), service.root_count()), (0, 0));
}

#[test]
fn a_root_that_disappears_is_reported_once_and_rewatched_when_it_returns() {
    let _guard = serial();
    let fixture = Fixture::new("watch-health");
    let root = repository(&fixture, "repo");
    let (service, receiver) = service_with(hub::recommended(), Duration::from_millis(100));
    service.watch("set", std::slice::from_ref(&root)).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    let mut lost = 0;
    let deadline = std::time::Instant::now() + WAIT;
    while lost == 0 && std::time::Instant::now() < deadline {
        if let Ok(Signal::Lost { path, .. }) = receiver.recv_timeout(Duration::from_millis(200)) {
            assert_eq!(path, root);
            lost += 1;
        }
    }
    assert_eq!(lost, 1);
    assert!(!drain(&receiver, QUIET)
        .iter()
        .any(|signal| matches!(signal, Signal::Lost { .. })));
    repository(&fixture, "repo");
    let deadline = std::time::Instant::now() + WAIT;
    let mut back = false;
    while !back && std::time::Instant::now() < deadline {
        back = matches!(receiver.recv_timeout(Duration::from_millis(200)), Ok(Signal::Changed(path)) if path == root);
    }
    assert!(back);
    write(&root, "src/a.txt", "x");
    assert_eq!(receiver.recv_timeout(WAIT).unwrap(), Signal::Changed(root));
    service.stop_all();
}
