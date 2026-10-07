use super::*;
use crate::platform::Fixture;
use crate::search::Mode;
use std::path::Path;
use std::sync::Mutex;

fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn repo(fixture: &Fixture, name: &str, files: &[(&str, &[u8])]) -> String {
    let root = fixture.0.join(name);
    std::fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "user.name", "admin"]);
    git(&root, &["config", "user.email", "admin@example.test"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    git(&root, &["config", "core.autocrlf", "false"]);
    for (relative, bytes) in files {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "fixture"]);
    root.to_str().unwrap().into()
}

fn request(paths: &[&str], pattern: &str) -> SearchRequest {
    SearchRequest {
        pattern: pattern.into(),
        repos: paths
            .iter()
            .map(|path| RepoTarget {
                path: (*path).into(),
                git_ref: None,
            })
            .collect(),
        ..SearchRequest::default()
    }
}

async fn run(
    request: SearchRequest,
    engine: Engine,
    cancel: Arc<AtomicBool>,
) -> (Summary, Vec<Match>, Vec<RepoStatus>) {
    let matches = Arc::new(Mutex::new(Vec::new()));
    let statuses = Arc::new(Mutex::new(Vec::new()));
    let (hits, repos) = (matches.clone(), statuses.clone());
    let send: Emit = Arc::new(move |outbound| match outbound {
        Outbound::Matches(chunk) => hits.lock().unwrap().extend(chunk.matches),
        Outbound::Repo(repo) => repos.lock().unwrap().push(repo.status),
        Outbound::Done(_) => {}
    });
    let summary = run_with_engine(
        request,
        Job {
            id: 47,
            concurrency: 1,
            cancel,
            send,
        },
        engine,
    )
    .await
    .unwrap();
    let hits = matches.lock().unwrap().clone();
    let repos = statuses.lock().unwrap().clone();
    (summary, hits, repos)
}

#[tokio::test]
async fn tracked_fixture_matches_git_grep_flags_paths_context_and_binary_policy() {
    let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("builtin-parity");
    let large = format!("{}needle{}\n", "é".repeat(1000), "x".repeat(1000));
    let path = repo(&fixture, "repo with spaces", &[
        ("a.txt", b"before\nneedle\nNeedle\nneedlework\na.c\nabc\na+c\naac\naaaa\na\na|b\nb\naaaab\n(after)\n"),
        ("crlf.txt", b"before\r\nneedle\r\nafter\r\n"),
        ("sp ace/Ș.txt", "😀 needle\n".as_bytes()),
        ("large.txt", large.as_bytes()),
        ("binary.dat", b"needle\0binary\n"),
        (".gitignore", b"ignored*\n"),
    ]);
    let root = Path::new(&path);
    std::fs::write(root.join("untracked.txt"), b"needle\n").unwrap();
    std::fs::write(root.join("ignored.txt"), b"needle\n").unwrap();
    std::fs::write(root.join("ignored-tracked.txt"), b"needle\n").unwrap();
    git(root, &["add", "-f", "ignored-tracked.txt"]);
    let commit = String::from_utf8(git(root, &["rev-parse", "HEAD"])).unwrap();
    git(
        root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{},submodule", commit.trim()),
        ],
    );
    std::fs::create_dir(root.join("submodule")).unwrap();
    std::fs::write(root.join("submodule/child.txt"), b"needle\n").unwrap();
    for (pattern, mode) in [
        ("needle", Mode::Fixed),
        ("a.c", Mode::Basic),
        ("a+c", Mode::Basic),
        (r"a\+c", Mode::Basic),
        (r"a\{2,4\}", Mode::Basic),
        (r"\(a\|b\)$", Mode::Basic),
    ] {
        for (ignore_case, whole_word) in [(false, false), (true, false), (true, true)] {
            let mut query = request(&[&path], pattern);
            query.mode = mode;
            query.ignore_case = ignore_case;
            query.whole_word = whole_word;
            query.context = 1;
            let expected = run(query.clone(), Engine::GitGrep, Arc::default()).await;
            crate::git::clear_activity();
            let actual = run(query, Engine::BuiltIn, Arc::default()).await;
            assert_eq!(
                actual.1, expected.1,
                "{pattern:?}, {mode:?}, {ignore_case}, {whole_word}"
            );
            assert_eq!(actual.0, expected.0);
            let activity = crate::git::activity_snapshot();
            assert_eq!(activity.len(), 1);
            assert!(serde_json::to_value(&activity[0]).unwrap()["argv"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "ls-files"));
        }
    }
    let mut query = request(&[&path], "needle");
    query.pathspecs = vec!["sp ace/*".into()];
    assert_eq!(
        run(query.clone(), Engine::BuiltIn, Arc::default()).await.1,
        run(query, Engine::GitGrep, Arc::default()).await.1
    );
}

#[tokio::test]
async fn caps_and_pre_cancel_have_the_same_outcome_in_both_engines() {
    let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("builtin-caps");
    let path = repo(
        &fixture,
        "a",
        &[("a.txt", b"hit\nhit\nhit\nhit\n"), ("b.txt", b"hit\n")],
    );
    let second = repo(&fixture, "b", &[("a.txt", b"hit\n")]);
    for (per_repo, overall) in [(3, 10), (10, 3), (4, 4), (5, 10)] {
        let mut query = request(&[&path, &second], "hit");
        query.max_per_repo = Some(per_repo);
        query.max_overall = Some(overall);
        let expected = run(query.clone(), Engine::GitGrep, Arc::default()).await;
        let actual = run(query, Engine::BuiltIn, Arc::default()).await;
        assert_eq!(actual.0, expected.0);
        assert_eq!(actual.1, expected.1);
        assert_eq!(
            actual
                .2
                .iter()
                .map(|status| (status.state, status.truncated))
                .collect::<Vec<_>>(),
            expected
                .2
                .iter()
                .map(|status| (status.state, status.truncated))
                .collect::<Vec<_>>()
        );
    }
    for engine in [Engine::BuiltIn, Engine::GitGrep] {
        crate::git::clear_activity();
        let result = run(
            request(&[&path], "hit"),
            engine,
            Arc::new(AtomicBool::new(true)),
        )
        .await;
        assert!(result.0.cancelled);
        assert_eq!(result.2[0].state, State::Cancelled);
        assert!(result.1.is_empty());
        assert!(crate::git::activity_snapshot().is_empty());
    }
}

#[tokio::test]
async fn bre_backreferences_preserve_git_matching() {
    let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("builtin-backreference");
    let path = repo(&fixture, "a", &[("a.txt", b"foofoo\nfoobar\n")]);
    let mut query = request(&[&path], r"\(foo\)\1");
    query.mode = Mode::Basic;
    assert_eq!(
        run(query.clone(), Engine::BuiltIn, Arc::default()).await.1,
        run(query, Engine::GitGrep, Arc::default()).await.1
    );
}

#[tokio::test]
async fn large_lines_and_late_nul_bytes_preserve_git_results() {
    let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("builtin-large-binary");
    let large = format!("hit\n{}hit\nhit\n", "x".repeat(3 * 1024 * 1024));
    let late_nul = format!("hit\n{}\nneedle\0tail\nneedle\n", "x".repeat(9000));
    let path = repo(
        &fixture,
        "a",
        &[
            ("large.txt", large.as_bytes()),
            ("late-nul.txt", late_nul.as_bytes()),
        ],
    );
    for pattern in ["hit", "needle"] {
        let query = request(&[&path], pattern);
        let expected = run(query.clone(), Engine::GitGrep, Arc::default()).await;
        let actual = run(query, Engine::BuiltIn, Arc::default()).await;
        assert_eq!(actual.1, expected.1);
        assert_eq!(actual.0, expected.0);
        assert_eq!(actual.2[0].truncated, expected.2[0].truncated);
    }
}
