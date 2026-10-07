use super::*;
use crate::git::TEST_RUNNER_LOCK;
use crate::platform::Fixture;
use crate::search::Mode;
use crate::search::{ContextLine, RepoTarget, SearchRequest};
use crate::search_grep::{perl_supported, version_allows_hint};
use crate::search_rows::{build_matches, parse_row, window, Row};
use std::path::Path;
use std::sync::Mutex;

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
    git(&dir, &["config", "core.autocrlf", "false"]);
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

#[derive(Clone, Debug)]
struct RepoOutcome {
    repo: String,
    status: RepoStatus,
    matches: Vec<Match>,
}

fn job(concurrency: usize, cancel: Arc<AtomicBool>, send: Emit) -> Job {
    Job {
        id: 1,
        concurrency,
        cancel,
        send,
    }
}

fn recorder() -> (Log, Emit) {
    let log: Log = Arc::default();
    let sink = log.clone();
    (
        log,
        Arc::new(move |outbound| sink.lock().unwrap().push(outbound)),
    )
}

fn outcomes(log: &Log) -> Vec<RepoOutcome> {
    let mut found: Vec<RepoOutcome> = Vec::new();
    for outbound in log.lock().unwrap().iter() {
        match outbound {
            Outbound::Matches(chunk) => {
                match found.iter_mut().find(|item| item.repo == chunk.repo) {
                    Some(item) => item.matches.extend(chunk.matches.clone()),
                    None => found.push(RepoOutcome {
                        repo: chunk.repo.clone(),
                        status: RepoStatus::without_matches(State::Done, None),
                        matches: chunk.matches.clone(),
                    }),
                }
            }
            Outbound::Repo(payload) => {
                let item = match found.iter().position(|item| item.repo == payload.repo) {
                    Some(at) => &mut found[at],
                    None => {
                        let empty = RepoOutcome {
                            repo: payload.repo.clone(),
                            status: payload.status.clone(),
                            matches: Vec::new(),
                        };
                        found.push(empty);
                        found.last_mut().unwrap()
                    }
                };
                item.status = payload.status.clone();
            }
            Outbound::Done(_) => {}
        }
    }
    found
}

async fn run(request: SearchRequest, concurrency: usize) -> (Summary, Vec<RepoOutcome>) {
    let _guard = TEST_RUNNER_LOCK.lock().await;
    let (log, send) = recorder();
    let summary = run_job(
        request,
        job(concurrency, Arc::new(AtomicBool::new(false)), send),
    )
    .await
    .unwrap();
    (summary, outcomes(&log))
}

fn only(payloads: &[RepoOutcome]) -> &RepoOutcome {
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
async fn quotes_equals_and_crlf_lines_match_in_a_folder_with_spaces() {
    let fixture = Fixture::new("search-quotes");
    let path = repo(
        &fixture,
        "repo with spaces",
        &[("cfg/variant.ini", b"[build]\r\nsw_variant=\"Product_Variant_1\"\r\nother=1\r\n")],
    );
    for pattern in [
        "sw_variant=\"Product_Variant_1\"",
        "SW_VARIANT=\"product_variant_1\"",
        "Variant_1\"",
    ] {
        let mut options = request(&[&path], pattern);
        options.ignore_case = true;
        let (_, payloads) = run(options, 4).await;
        let found = only(&payloads);
        assert_eq!(found.matches.len(), 1, "{pattern}");
        assert_eq!((found.matches[0].path.as_str(), found.matches[0].line), ("cfg/variant.ini", 2));
    }
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
    options.repos[0].git_ref = Some("HEAD".into());
    let (_, committed) = run(options.clone(), 4).await;
    let head = &only(&committed).matches;
    assert_eq!((head.len(), head[0].path.as_str()), (1, "f.txt"));
    for bad in ["--output=x", "a..b"] {
        options.repos[0].git_ref = Some(bad.into());
        let (summary, payloads) = run(options.clone(), 4).await;
        assert_eq!(
            (summary.failed, only(&payloads).status.state),
            (1, State::Failed)
        );
    }
}

#[tokio::test]
async fn per_repo_cap_counts_matches_across_files_without_failing() {
    let fixture = Fixture::new("search-cap");
    let files: Vec<(String, &[u8])> = (0..5)
        .map(|n| (format!("f{n}.txt"), &b"hit\n"[..]))
        .collect();
    let borrowed: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(name, body)| (name.as_str(), *body))
        .collect();
    let path = repo(&fixture, "a", &borrowed);
    let mut options = request(&[&path], "hit");
    options.max_per_repo = Some(3);
    let (_, found) = run(options, 4).await;
    let found = only(&found);
    assert_eq!(
        (
            found.matches.len(),
            found.status.truncated,
            found.status.state
        ),
        (3, true, State::Done)
    );
}

#[tokio::test]
async fn per_repo_cap_counts_matches_inside_one_file() {
    let fixture = Fixture::new("search-cap-file");
    let path = repo(&fixture, "a", &[("f.txt", "hit\n".repeat(10).as_bytes())]);
    let mut options = request(&[&path], "hit");
    options.max_per_repo = Some(3);
    let found = run(options, 4).await.1;
    assert_eq!(
        (only(&found).matches.len(), only(&found).status.truncated),
        (3, true)
    );
}

#[tokio::test]
async fn a_huge_match_volume_stops_git_instead_of_failing() {
    let fixture = Fixture::new("search-volume");
    let body = "hit hit hit hit hit hit hit hit\n".repeat(400_000);
    let path = repo(&fixture, "a", &[("big.txt", body.as_bytes())]);
    let found = run(request(&[&path], "hit"), 4).await.1;
    let found = only(&found);
    assert_eq!(
        (
            found.status.state,
            found.status.truncated,
            found.matches.len()
        ),
        (State::Done, true, 200)
    );
}

#[tokio::test]
async fn overall_cap_stops_running_and_pending_repositories() {
    let fixture = Fixture::new("search-overall");
    let first = repo(&fixture, "a", &[("f.txt", b"hit\nhit\nhit\nhit\nhit\n")]);
    let second = repo(&fixture, "b", &[("f.txt", b"hit\n")]);
    let mut options = request(&[&first, &second], "hit");
    options.max_overall = Some(3);
    let (summary, found) = run(options, 1).await;
    assert!(summary.capped);
    assert_eq!(summary.matches, 3);
    assert!(
        found
            .iter()
            .find(|item| item.repo == first)
            .unwrap()
            .status
            .truncated
    );
    assert_eq!(
        found
            .iter()
            .find(|item| item.repo == second)
            .unwrap()
            .status
            .state,
        State::Skipped
    );
}

#[tokio::test]
async fn capped_is_false_when_results_exactly_fit() {
    let fixture = Fixture::new("search-exact");
    let path = repo(&fixture, "a", &[("f.txt", b"hit\nhit\nhit\n")]);
    let mut options = request(&[&path], "hit");
    options.max_overall = Some(3);
    options.max_per_repo = Some(3);
    let (summary, found) = run(options, 1).await;
    assert_eq!(
        (
            summary.capped,
            summary.matches,
            only(&found).status.truncated
        ),
        (false, 3, false)
    );
}

#[tokio::test]
async fn repositories_without_matches_and_non_repositories_are_isolated() {
    let fixture = Fixture::new("search-isolate");
    let hit = repo(&fixture, "hit", &[("f.txt", b"needle\n")]);
    let miss = repo(&fixture, "miss", &[("f.txt", b"other\n")]);
    let plain = fixture.0.join("plain");
    std::fs::create_dir(&plain).unwrap();
    let plain = plain.to_str().unwrap().to_string();
    let (summary, found) = run(request(&[&hit, &miss, &plain], "needle"), 4).await;
    let state = |path: &str| found.iter().find(|item| item.repo == path).unwrap();
    assert_eq!(state(&hit).status.matches, 1);
    assert_eq!(
        (state(&miss).status.state, state(&miss).status.error.clone()),
        (State::Done, None)
    );
    assert_eq!(state(&plain).status.state, State::Failed);
    assert_eq!((summary.repos, summary.failed, summary.matches), (3, 1, 1));
}

#[tokio::test]
async fn duplicate_repository_spellings_are_searched_once() {
    let fixture = Fixture::new("search-dedupe");
    let path = repo(&fixture, "a", &[("f.txt", b"hit\n")]);
    let spelled = format!("{path}/");
    let (summary, found) = run(request(&[&path, &spelled], "hit"), 4).await;
    assert_eq!((summary.repos, found.len(), summary.matches), (1, 1, 1));
}

#[tokio::test]
async fn cancel_marks_remaining_repositories_without_running_them() {
    let fixture = Fixture::new("search-cancel");
    let paths: Vec<String> = (0..3)
        .map(|n| repo(&fixture, &format!("r{n}"), &[("f.txt", b"hit\n")]))
        .collect();
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let _guard = TEST_RUNNER_LOCK.lock().await;
    let cancel = Arc::new(AtomicBool::new(false));
    let (log, record) = recorder();
    let flag = cancel.clone();
    let send: Emit = Arc::new(move |outbound| {
        flag.store(true, Ordering::Relaxed);
        record(outbound);
    });
    let summary = run_job(request(&refs, "hit"), job(1, cancel, send))
        .await
        .unwrap();
    let states: Vec<State> = outcomes(&log)
        .iter()
        .map(|item| item.status.state)
        .collect();
    assert_eq!(states, [State::Done, State::Cancelled, State::Cancelled]);
    assert!(summary.cancelled);
}

#[cfg(target_os = "linux")]
fn lingering(script: &str) -> Vec<String> {
    [
        "-c",
        &format!("alias.skein-linger=!{script}"),
        "skein-linger",
    ]
    .map(String::from)
    .to_vec()
}

#[cfg(target_os = "linux")]
async fn run_lingering(
    sink: crate::git::StdoutSink,
    cancel: Arc<AtomicBool>,
) -> Result<crate::git::Captured, String> {
    let argv = lingering("echo ready; sleep 30");
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let request = crate::git::Request {
        args: &args,
        context: "search-linger",
        timeout: std::time::Duration::from_secs(60),
        expected: &[0],
        policy: crate::git::OutputPolicy::Text,
    };
    crate::git::execute_streaming(request, cancel, sink).await
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn cancelling_after_spawn_kills_the_live_git_process() {
    let _guard = TEST_RUNNER_LOCK.lock().await;
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    let sink: crate::git::StdoutSink = Arc::new(move |_| {
        flag.store(true, Ordering::Relaxed);
        false
    });
    let result = run_lingering(sink, cancel).await;
    assert_eq!(result.err().as_deref(), Some("Git command cancelled"));
    assert!(crate::git::runner_idle());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_sink_stop_kills_the_live_git_process_and_succeeds() {
    let _guard = TEST_RUNNER_LOCK.lock().await;
    let sink: crate::git::StdoutSink = Arc::new(|_| true);
    let result = run_lingering(sink, Arc::new(AtomicBool::new(false))).await;
    assert!(result.is_ok());
    assert!(crate::git::runner_idle());
}

#[tokio::test]
async fn a_panicking_repository_still_reports_failed_and_the_job_finishes() {
    let fixture = Fixture::new("search-panic");
    let first = repo(&fixture, "a", &[("f.txt", b"hit\n")]);
    let second = repo(&fixture, "b", &[("f.txt", b"hit\n")]);
    let _guard = TEST_RUNNER_LOCK.lock().await;
    let (log, record) = recorder();
    let armed = Arc::new(AtomicBool::new(true));
    let victim = first.clone();
    let send: Emit = Arc::new(move |outbound| {
        if let Outbound::Matches(chunk) = &outbound {
            if chunk.repo == victim && armed.swap(false, Ordering::SeqCst) {
                panic!("send failed");
            }
        }
        record(outbound);
    });
    let summary = run_job(
        request(&[&first, &second], "hit"),
        job(1, Arc::default(), send),
    )
    .await
    .unwrap();
    let found = outcomes(&log);
    assert_eq!(
        found
            .iter()
            .find(|item| item.repo == first)
            .unwrap()
            .status
            .state,
        State::Failed
    );
    assert_eq!(
        found
            .iter()
            .find(|item| item.repo == second)
            .unwrap()
            .status
            .matches,
        1
    );
    assert_eq!(summary.failed, 1);
    assert!(log
        .lock()
        .unwrap()
        .iter()
        .any(|outbound| matches!(outbound, Outbound::Done(_))));
}

#[tokio::test]
async fn done_is_sent_even_when_the_job_is_aborted() {
    let fixture = Fixture::new("search-abort");
    let path = repo(&fixture, "a", &[("f.txt", b"hit\n")]);
    let _guard = TEST_RUNNER_LOCK.lock().await;
    let (log, record) = recorder();
    let (started, mut seen) = tokio::sync::mpsc::unbounded_channel();
    let send: Emit = Arc::new(move |outbound| {
        let _ = started.send(());
        record(outbound);
    });
    let running = tokio::spawn(run_job(
        request(&[&path], "hit"),
        job(1, Arc::default(), send),
    ));
    seen.recv().await.unwrap();
    running.abort();
    let _ = running.await;
    let done = log
        .lock()
        .unwrap()
        .iter()
        .filter(|outbound| matches!(outbound, Outbound::Done(_)))
        .count();
    assert!(done >= 1);
}

#[tokio::test]
async fn columns_are_utf16_and_long_lines_keep_the_match_in_view() {
    let fixture = Fixture::new("search-columns");
    let long = format!("{}needle{}\n", "é".repeat(1000), "x".repeat(1000));
    let path = repo(
        &fixture,
        "a",
        &[
            ("a.txt", "😀 needle\n".as_bytes()),
            ("b.txt", long.as_bytes()),
        ],
    );
    let (_, found) = run(request(&[&path], "needle"), 4).await;
    let found = &only(&found).matches;
    assert_eq!((found[0].path.as_str(), found[0].column), ("a.txt", 4));
    let units: Vec<u16> = found[1].text.encode_utf16().collect();
    let start = (found[1].column - 1) as usize;
    assert_eq!(String::from_utf16_lossy(&units[start..start + 6]), "needle");
    assert!(found[1].text.chars().count() <= 402);
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
    let (_, found) = run(options, 4).await;
    let found = &only(&found).matches[0];
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
async fn matches_are_emitted_in_chunks_of_two_hundred() {
    let fixture = Fixture::new("search-chunks");
    let path = repo(&fixture, "a", &[("f.txt", "hit\n".repeat(450).as_bytes())]);
    let _guard = TEST_RUNNER_LOCK.lock().await;
    let (log, send) = recorder();
    let mut options = request(&[&path], "hit");
    options.max_per_repo = Some(450);
    run_job(options, job(1, Arc::default(), send))
        .await
        .unwrap();
    let sizes: Vec<usize> = log
        .lock()
        .unwrap()
        .iter()
        .filter_map(|outbound| match outbound {
            Outbound::Matches(chunk) => Some(chunk.matches.len()),
            _ => None,
        })
        .collect();
    assert_eq!(sizes, [200, 200, 50]);
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
    let supported = {
        let _runner = crate::test_support::git_runner().await;
        perl_supported().await
    };
    if !supported {
        return;
    }
    let fixture = Fixture::new("search-perl");
    let path = repo(&fixture, "a", &[("f.txt", b"ab12\n")]);
    let mut options = request(&[&path], r"\d{2}");
    options.mode = Mode::Perl;
    assert_eq!(only(&run(options, 4).await.1).status.matches, 1);
}

#[test]
fn context_rows_keep_nul_bytes_and_numeric_text_is_not_a_column() {
    let nul = parse_row(b"p\x005\x00ab\x00cd", None, true).unwrap();
    assert!(matches!(nul, Row::Context { text, .. } if text == "ab\0cd"));
    let hit = parse_row(b"p\x005\x001\x00ab\x00cd", None, true).unwrap();
    assert!(matches!(hit, Row::Match(found) if found.text == "ab\0cd" && found.column == 1));
    let wide = parse_row(b"p\x005\x0012\x00more", None, true).unwrap();
    assert!(matches!(wide, Row::Context { text, .. } if text == "12\0more"));
}

#[test]
fn context_gathering_walks_outward_and_stops_at_other_matches() {
    let hit = |line| parse_row(format!("p\0{line}\x001\0x").as_bytes(), None, true).unwrap();
    let ctx = |line| parse_row(format!("p\0{line}\0c").as_bytes(), None, true).unwrap();
    let rows = vec![ctx(1), ctx(2), hit(3), ctx(4), ctx(5), hit(6), ctx(7)];
    let found = build_matches(rows, 2);
    let lines = |at: usize| {
        found[at]
            .context
            .iter()
            .map(|line| line.line)
            .collect::<Vec<_>>()
    };
    assert_eq!((lines(0), lines(1)), (vec![1, 2, 4, 5], vec![4, 5, 7]));
}

#[test]
fn trailing_carriage_returns_cannot_push_the_column_out_of_range() {
    let raw = format!("{}needle{}", "a".repeat(500), "\r".repeat(600));
    let (text, column) = window(raw.as_bytes(), 501);
    assert!(text.starts_with('…') || column > 0);
    let (short, short_column) = window(b"ab\r\r\r", 6);
    assert_eq!((short.as_str(), short_column), ("ab", 3));
    let (long, _) = window(
        format!("{}{}", "x".repeat(450), "\r".repeat(500)).as_bytes(),
        900,
    );
    assert!(!long.is_empty());
}

#[test]
fn windows_clip_around_the_match_with_a_valid_column() {
    let raw = format!("{}needle{}", "a".repeat(900), "b".repeat(900));
    let (text, column) = window(raw.as_bytes(), 901);
    let units: Vec<u16> = text.encode_utf16().collect();
    let start = (column - 1) as usize;
    assert_eq!(String::from_utf16_lossy(&units[start..start + 6]), "needle");
}

#[test]
fn git_versions_gate_the_per_file_hint() {
    assert!(version_allows_hint("git version 2.38.0"));
    assert!(version_allows_hint("git version 2.56.0.windows.1"));
    assert!(version_allows_hint("git version 3.0.0"));
    assert!(!version_allows_hint("git version 2.37.9"));
    assert!(!version_allows_hint("garbage"));
}

struct Utf8Locale;

impl Utf8Locale {
    fn enter() -> Option<Self> {
        let is_utf8 = |name: &str| name.to_ascii_lowercase().replace('-', "").contains("utf8");
        let inherited = ["LC_ALL", "LC_CTYPE", "LANG"].iter().find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()));
        let name = match inherited {
            Some(value) if is_utf8(&value) => value,
            _ => {
                let listed = std::process::Command::new("locale").arg("-a").output().ok()?;
                String::from_utf8_lossy(&listed.stdout).lines().find(|line| is_utf8(line))?.to_string()
            }
        };
        *crate::git::CTYPE_OVERRIDE.lock().unwrap() = Some(name.into());
        Some(Self)
    }
}

impl Drop for Utf8Locale {
    fn drop(&mut self) {
        *crate::git::CTYPE_OVERRIDE.lock().unwrap() = None;
    }
}

#[tokio::test]
async fn case_insensitive_search_matches_non_ascii_letters_in_a_utf8_locale() {
    let Some(_locale) = Utf8Locale::enter() else {
        eprintln!("skipped: no UTF-8 locale is installed on this machine");
        return;
    };
    let fixture = Fixture::new("search-utf8");
    let path = repo(&fixture, "a", &[("f.txt", "Ștefan\nCAFÉ\n".as_bytes())]);
    for (pattern, mode) in [("ștefan", Mode::Fixed), ("café", Mode::Basic)] {
        let mut options = request(&[&path], pattern);
        options.mode = mode;
        options.ignore_case = true;
        assert_eq!(only(&run(options, 4).await.1).status.matches, 1, "{pattern}");
    }
    let supported = {
        let _runner = crate::test_support::git_runner().await;
        perl_supported().await
    };
    if supported {
        let mut options = request(&[&path], "ș.efan");
        options.mode = Mode::Perl;
        options.ignore_case = true;
        assert_eq!(only(&run(options, 4).await.1).status.matches, 1);
    }
}
