use super::*;
use crate::platform::Fixture;
use crate::search::{ContextLine, RepoTarget, SearchRequest};
use std::path::Path;

fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn repo(fixture: &Fixture, name: &str, files: &[(&str, &[u8])]) -> String {
    let dir = fixture.0.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.name", "Test"]);
    git(&dir, &["config", "user.email", "t@example.test"]);
    git(&dir, &["config", "commit.gpgsign", "false"]);
    for (path, content) in files {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, content).unwrap();
    }
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "init"]);
    dir.to_str().unwrap().to_string()
}

fn request(paths: &[&str], pattern: &str) -> SearchRequest {
    SearchRequest {
        repos: paths
            .iter()
            .map(|path| RepoTarget {
                path: path.to_string(),
                git_ref: None,
            })
            .collect(),
        pattern: pattern.into(),
        ..SearchRequest::default()
    }
}

type Log = Arc<Mutex<Vec<Outbound>>>;

async fn run(request: SearchRequest, concurrency: usize) -> (Summary, Vec<RepoPayload>) {
    let log: Log = Arc::default();
    let sink = log.clone();
    let send: Arc<dyn Fn(Outbound) + Send + Sync> =
        Arc::new(move |outbound| sink.lock().unwrap().push(outbound));
    let summary = run_job(
        1,
        request,
        concurrency,
        Arc::new(AtomicBool::new(false)),
        send,
    )
    .await
    .unwrap();
    (summary, repos_of(&log))
}

fn repos_of(log: &Log) -> Vec<RepoPayload> {
    log.lock()
        .unwrap()
        .iter()
        .filter_map(|outbound| match outbound {
            Outbound::Repo(payload) => Some(payload.clone()),
            Outbound::Done(_) => None,
        })
        .collect()
}

fn only(payloads: &[RepoPayload]) -> &RepoPayload {
    assert_eq!(payloads.len(), 1);
    &payloads[0]
}

#[tokio::test]
async fn fixed_strings_ignore_regex_syntax_and_regex_mode_uses_it() {
    let fixture = Fixture::new("search-mode");
    let path = repo(&fixture, "a", &[("f.txt", b"a.c\nabc\n")]);
    let mut fixed = request(&[&path], "a.c");
    let (_, payloads) = run(fixed.clone(), 4).await;
    assert_eq!(only(&payloads).matches.len(), 1);
    fixed.mode = Mode::Basic;
    let (_, payloads) = run(fixed, 4).await;
    assert_eq!(only(&payloads).matches.len(), 2);
}

#[tokio::test]
async fn patterns_starting_with_a_dash_are_data() {
    let fixture = Fixture::new("search-dash");
    let path = repo(&fixture, "a", &[("f.txt", b"--version here\n-x\n")]);
    let (_, payloads) = run(request(&[&path], "--version"), 4).await;
    let found = only(&payloads);
    assert_eq!(
        (
            found.matches.len(),
            found.matches[0].line,
            found.matches[0].column
        ),
        (1, 1, 1)
    );
}

#[tokio::test]
async fn case_and_whole_word_flags_apply() {
    let fixture = Fixture::new("search-case");
    let path = repo(&fixture, "a", &[("f.txt", b"Foo\nfoo\nfoobar\n")]);
    let mut options = request(&[&path], "foo");
    let (_, payloads) = run(options.clone(), 4).await;
    assert_eq!(only(&payloads).matches.len(), 2);
    options.ignore_case = true;
    let (_, payloads) = run(options.clone(), 4).await;
    assert_eq!(only(&payloads).matches.len(), 3);
    options.whole_word = true;
    let (_, payloads) = run(options, 4).await;
    assert_eq!(only(&payloads).matches.len(), 2);
}

#[tokio::test]
async fn pathspecs_filter_and_binary_files_are_skipped() {
    let fixture = Fixture::new("search-paths");
    let path = repo(
        &fixture,
        "a",
        &[
            ("src/a.rs", b"needle\n"),
            ("docs/a.md", b"needle\n"),
            ("bin.dat", b"needle\0\x01\x02"),
        ],
    );
    let (_, payloads) = run(request(&[&path], "needle"), 4).await;
    let paths: Vec<_> = only(&payloads)
        .matches
        .iter()
        .map(|found| found.path.clone())
        .collect();
    assert_eq!(paths, ["docs/a.md", "src/a.rs"]);
    let mut filtered = request(&[&path], "needle");
    filtered.pathspecs = vec!["src/*".into()];
    let (_, payloads) = run(filtered, 4).await;
    assert_eq!(only(&payloads).matches[0].path, "src/a.rs");
}

#[tokio::test]
async fn bad_pathspecs_patterns_and_limits_are_rejected() {
    let mut bad = request(&["/x"], "p");
    bad.pathspecs = vec!["-rf".into()];
    assert!(plan(&bad).is_err());
    bad.pathspecs = vec![":(top)x".into()];
    assert!(plan(&bad).is_err());
    bad.pathspecs.clear();
    bad.context = 4;
    assert!(plan(&bad).is_err());
    assert!(plan(&request(&["/x"], "")).is_err());
    assert!(plan(&request(&["/x"], "a\nb")).is_err());
    assert!(plan(&request(&[], "a")).is_err());
}

#[tokio::test]
async fn a_ref_target_searches_committed_content_and_rejects_bad_refs() {
    let fixture = Fixture::new("search-ref");
    let path = repo(&fixture, "a", &[("f.txt", b"committed\n")]);
    git(Path::new(&path), &["branch", "old"]);
    std::fs::write(Path::new(&path).join("f.txt"), b"edited\n").unwrap();
    let mut options = request(&[&path], "committed");
    options.repos[0].git_ref = Some("old".into());
    let (_, payloads) = run(options.clone(), 4).await;
    let found = &only(&payloads).matches;
    assert_eq!((found.len(), found[0].path.as_str()), (1, "f.txt"));
    let (_, working) = run(request(&[&path], "committed"), 4).await;
    assert_eq!(only(&working).status.matches, 0);
    for bad in ["--output=x", "HEAD", "a..b"] {
        options.repos[0].git_ref = Some(bad.into());
        let (summary, payloads) = run(options.clone(), 4).await;
        assert_eq!(
            (summary.failed, only(&payloads).status.state),
            (1, State::Failed)
        );
    }
}

#[tokio::test]
async fn per_repo_cap_truncates_even_inside_one_file() {
    let fixture = Fixture::new("search-cap");
    let path = repo(&fixture, "a", &[("f.txt", "hit\n".repeat(10).as_bytes())]);
    let mut options = request(&[&path], "hit");
    options.max_per_repo = Some(3);
    let (_, payloads) = run(options, 4).await;
    let found = only(&payloads);
    assert_eq!(
        (
            found.matches.len(),
            found.status.matches,
            found.status.truncated
        ),
        (3, 3, true)
    );
}

#[tokio::test]
async fn overall_cap_stops_later_repositories() {
    let fixture = Fixture::new("search-overall");
    let first = repo(&fixture, "a", &[("f.txt", b"hit\nhit\nhit\n")]);
    let second = repo(&fixture, "b", &[("f.txt", b"hit\n")]);
    let mut options = request(&[&first, &second], "hit");
    options.max_overall = Some(3);
    let (summary, payloads) = run(options, 1).await;
    assert!(summary.capped);
    assert_eq!(summary.matches, 3);
    let skipped = payloads
        .iter()
        .find(|payload| payload.repo == second)
        .unwrap();
    assert_eq!(skipped.status.state, State::Skipped);
}

#[tokio::test]
async fn repositories_without_matches_and_non_repositories_are_isolated() {
    let fixture = Fixture::new("search-isolate");
    let hit = repo(&fixture, "hit", &[("f.txt", b"needle\n")]);
    let miss = repo(&fixture, "miss", &[("f.txt", b"other\n")]);
    let plain = fixture.0.join("plain");
    std::fs::create_dir(&plain).unwrap();
    let plain = plain.to_str().unwrap().to_string();
    let (summary, payloads) = run(request(&[&hit, &miss, &plain], "needle"), 4).await;
    let state = |path: &str| {
        payloads
            .iter()
            .find(|payload| payload.repo == path)
            .unwrap()
    };
    assert_eq!(state(&hit).status.matches, 1);
    assert_eq!(
        (state(&miss).status.state, state(&miss).status.error.clone()),
        (State::Done, None)
    );
    assert_eq!(state(&plain).status.state, State::Failed);
    assert_eq!((summary.repos, summary.failed, summary.matches), (3, 1, 1));
}

#[tokio::test]
async fn cancel_marks_remaining_repositories_without_running_them() {
    let fixture = Fixture::new("search-cancel");
    let paths: Vec<String> = (0..3)
        .map(|n| repo(&fixture, &format!("r{n}"), &[("f.txt", b"hit\n")]))
        .collect();
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let cancel = Arc::new(AtomicBool::new(false));
    let log: Log = Arc::default();
    let (sink, flag) = (log.clone(), cancel.clone());
    let send: Arc<dyn Fn(Outbound) + Send + Sync> = Arc::new(move |outbound| {
        flag.store(true, Ordering::Relaxed);
        sink.lock().unwrap().push(outbound);
    });
    let summary = run_job(1, request(&refs, "hit"), 1, cancel, send)
        .await
        .unwrap();
    let states: Vec<State> = repos_of(&log)
        .iter()
        .map(|payload| payload.status.state)
        .collect();
    assert_eq!(states, [State::Done, State::Cancelled, State::Cancelled]);
    assert!(summary.cancelled);
}

#[tokio::test]
async fn a_cancelled_flag_stops_a_running_git_process() {
    let fixture = Fixture::new("search-kill");
    let path = repo(&fixture, "a", &[("f.txt", b"hit\n")]);
    let cancel = Arc::new(AtomicBool::new(true));
    let options = request(&[&path], "hit");
    let plan = plan(&options).unwrap();
    let result = crate::search::search_repo(&options.repos[0], &plan, 10, cancel).await;
    assert_eq!(result.status.state, State::Cancelled);
}

#[tokio::test]
async fn unicode_and_spaced_paths_columns_and_context_parse() {
    let fixture = Fixture::new("search-unicode");
    let path = repo(
        &fixture,
        "a",
        &[("sp ace/é ü.txt", "one\nżółć needle\nthree\n".as_bytes())],
    );
    let mut options = request(&[&path], "needle");
    options.context = 1;
    let (_, payloads) = run(options, 4).await;
    let found = &only(&payloads).matches[0];
    assert_eq!(
        (found.path.as_str(), found.line, found.column),
        ("sp ace/é ü.txt", 2, 6)
    );
    assert_eq!(found.text, "żółć needle");
    let lines: Vec<(u32, &str)> = found
        .context
        .iter()
        .map(|ContextLine { line, text }| (*line, text.as_str()))
        .collect();
    assert_eq!(lines, [(1, "one"), (3, "three")]);
}

#[tokio::test]
async fn long_lines_are_truncated() {
    let fixture = Fixture::new("search-long");
    let path = repo(
        &fixture,
        "a",
        &[("f.txt", format!("needle{}\n", "x".repeat(2000)).as_bytes())],
    );
    let (_, payloads) = run(request(&[&path], "needle"), 4).await;
    assert_eq!(only(&payloads).matches[0].text.chars().count(), 401);
}

#[tokio::test]
async fn untracked_files_need_the_flag_and_a_working_tree_target() {
    let fixture = Fixture::new("search-untracked");
    let path = repo(&fixture, "a", &[("f.txt", b"x\n")]);
    std::fs::write(Path::new(&path).join("new.txt"), b"needle\n").unwrap();
    let mut options = request(&[&path], "needle");
    assert_eq!(only(&run(options.clone(), 4).await.1).status.matches, 0);
    options.untracked = true;
    assert_eq!(only(&run(options.clone(), 4).await.1).status.matches, 1);
    options.repos[0].git_ref = Some("main".into());
    assert_eq!(only(&run(options, 4).await.1).status.state, State::Failed);
}

#[tokio::test]
async fn perl_mode_matches_when_supported() {
    if !perl_supported().await {
        return;
    }
    let fixture = Fixture::new("search-perl");
    let path = repo(&fixture, "a", &[("f.txt", b"ab12\n")]);
    let mut options = request(&[&path], r"\d{2}");
    options.mode = Mode::Perl;
    assert_eq!(only(&run(options, 4).await.1).status.matches, 1);
}

#[test]
fn service_limits_and_cancels_searches() {
    let service = Service::default();
    let started: Vec<_> = (0..MAX_RUNNING)
        .map(|_| service.register().unwrap())
        .collect();
    assert!(service.register().is_err());
    assert!(service.cancel(started[0].0));
    assert!(started[0].1.load(Ordering::Relaxed));
    assert!(!service.cancel(999));
}
