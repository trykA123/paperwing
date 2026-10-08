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

#[tokio::test]
async fn unicode_case_words_and_regex_ignore_the_process_locale() {
    let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("builtin-unicode-locales");
    let path = repo(
        &fixture,
        "a",
        &[("Ș.txt", "ș\nȘ\nșș\nșx\nxș\néș\nșé\n(ș)\n😀Ș\n".as_bytes())],
    );
    struct Locale;
    impl Drop for Locale {
        fn drop(&mut self) {
            *crate::git::CTYPE_OVERRIDE.lock().unwrap() = None;
        }
    }
    let _restore = Locale;
    for locale in ["C", "C.UTF-8", "skein-unavailable-locale"] {
        *crate::git::CTYPE_OVERRIDE.lock().unwrap() = Some(locale.into());
        for (pattern, mode) in [("ș", Mode::Fixed), ("[ș]", Mode::Basic)] {
            let mut query = request(&[&path], pattern);
            query.mode = mode;
            query.ignore_case = true;
            query.whole_word = true;
            let result = run(query, Engine::BuiltIn, Arc::default()).await;
            assert_eq!(result.0.failed, 0);
            assert_eq!(
                result.1.iter().map(|hit| hit.line).collect::<Vec<_>>(),
                [1, 2, 8, 9],
                "{locale}, {mode:?}"
            );
            assert_eq!(result.1[3].column, 3);
        }
    }
}

#[tokio::test]
async fn untracked_search_respects_nested_info_and_configured_global_excludes() {
    let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("builtin-untracked-ignores");
    let path = repo(
        &fixture,
        "a",
        &[
            ("tracked.txt", b"needle\n"),
            (".gitignore", b"ignored*\n"),
            ("nested/.gitignore", b"nested-ignore.txt\n"),
        ],
    );
    let root = Path::new(&path);
    for name in [
        "untracked.txt",
        "ignored.txt",
        "nested/visible.txt",
        "nested/nested-ignore.txt",
        "info-ignore.txt",
        "global-ignore.txt",
        "ignored-tracked.txt",
    ] {
        std::fs::write(root.join(name), b"needle\n").unwrap();
    }
    #[cfg(unix)]
    std::fs::write(root.join("100644 abc 0\tname.txt"), b"needle\n").unwrap();
    git(root, &["add", "-f", "ignored-tracked.txt"]);
    std::fs::write(root.join(".git/info/exclude"), b"info-ignore.txt\n").unwrap();
    let global = fixture.0.join("global-excludes");
    std::fs::write(&global, b"global-ignore.txt\n").unwrap();
    git(
        root,
        &["config", "core.excludesFile", global.to_str().unwrap()],
    );
    let mut query = request(&[&path], "needle");
    query.untracked = true;
    let expected = run(query.clone(), Engine::GitGrep, Arc::default()).await;
    crate::git::clear_activity();
    let actual = run(query.clone(), Engine::BuiltIn, Arc::default()).await;
    assert_eq!(actual.1, expected.1);
    assert_eq!(actual.0, expected.0);
    assert_eq!(crate::git::activity_snapshot().len(), 1);
    query.pathspecs = vec!["nested/*".into()];
    assert_eq!(
        run(query.clone(), Engine::BuiltIn, Arc::default()).await.1,
        run(query, Engine::GitGrep, Arc::default()).await.1
    );
}

#[tokio::test]
async fn settings_changes_apply_to_the_next_search_and_fallbacks_are_explicit() {
    let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("builtin-settings");
    let path = repo(&fixture, "a", &[("a.txt", b"foofoo\nd\n123\n")]);
    std::fs::write(Path::new(&path).join("untracked.txt"), b"foofoo\n").unwrap();
    for (value, expected_operation) in [("builtIn", "ls-files"), ("gitGrep", "grep")] {
        let settings: crate::settings::Settings = serde_json::from_value(serde_json::json!({"workspace":{"searchEngine":value,"searchFiles":"trackedAndUntracked"}})).unwrap();
        let mut query = request(&[&path], "foo");
        let engine = crate::search_service::configure_request(
            &mut query,
            settings.search_options().unwrap(),
        );
        crate::git::clear_activity();
        let result = run(query, engine, Arc::default()).await;
        assert_eq!(result.1.len(), 2);
        let calls = serde_json::to_value(crate::git::activity_snapshot()).unwrap();
        assert!(calls.as_array().unwrap().iter().any(|call| call["argv"]
            .as_array()
            .unwrap()
            .iter()
            .any(|arg| arg == expected_operation)));
    }
    for pattern in [r"\(foo\)\1", r"\d"] {
        let mut query = request(&[&path], pattern);
        query.mode = Mode::Basic;
        let expected = run(query.clone(), Engine::GitGrep, Arc::default()).await;
        let actual = run(query, Engine::BuiltIn, Arc::default()).await;
        assert_eq!(actual.1, expected.1);
        assert!(actual.2[0]
            .engine_note
            .as_ref()
            .unwrap()
            .contains("Git grep"));
    }
    if crate::search_grep::perl_supported().await {
        let mut query = request(&[&path], "foo+");
        query.mode = Mode::Perl;
        let result = run(query, Engine::BuiltIn, Arc::default()).await;
        assert_eq!(result.1.len(), 1);
        assert!(result.2[0].engine_note.as_ref().unwrap().contains("Perl"));
        assert_eq!(
            serde_json::to_value(&result.2[0]).unwrap()["engineNote"],
            result.2[0].engine_note.clone().unwrap()
        );
    }
}

#[tokio::test]
async fn git_attributes_keep_binary_text_out_of_builtin_results() {
    let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("builtin-attributes");
    for (name, attributes) in [
        ("root", ".gitattributes"),
        ("nested", "src/.gitattributes"),
        ("info", ".git/info/attributes"),
        ("configured", "custom.attributes"),
    ] {
        let path = repo(
            &fixture,
            name,
            &[("src/a.txt", b"needle\n"), ("b.txt", b"needle\n")],
        );
        let root = Path::new(&path);
        std::fs::write(root.join(attributes), b"*.txt binary\n").unwrap();
        if name == "configured" {
            git(
                root,
                &["config", "core.attributesFile", "custom.attributes"],
            );
        }
        let query = request(&[&path], "needle");
        let expected = run(query.clone(), Engine::GitGrep, Arc::default()).await;
        let actual = run(query, Engine::BuiltIn, Arc::default()).await;
        assert_eq!(actual.1, expected.1, "{name}");
        assert_eq!(actual.0, expected.0, "{name}");
        assert!(actual.2[0]
            .engine_note
            .as_deref()
            .unwrap()
            .contains("attributes"));
    }
}

#[tokio::test]
async fn index_only_and_common_metadata_attributes_require_git() {
    let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("builtin-linked-attributes");
    let path = repo(
        &fixture,
        "indexed",
        &[
            ("a.txt", b"needle\n"),
            (".gitattributes", b"*.txt binary\n"),
        ],
    );
    std::fs::remove_file(Path::new(&path).join(".gitattributes")).unwrap();
    let indexed = run(request(&[&path], "needle"), Engine::BuiltIn, Arc::default()).await;
    assert!(indexed.1.is_empty());
    assert!(indexed.2[0].engine_note.is_some());

    let source = repo(&fixture, "source", &[("a.txt", b"needle\n")]);
    let linked = fixture.0.join("linked");
    git(
        Path::new(&source),
        &["worktree", "add", "--detach", linked.to_str().unwrap()],
    );
    std::fs::write(
        Path::new(&source).join(".git/info/attributes"),
        b"*.txt binary\n",
    )
    .unwrap();
    assert!(crate::search_attributes::requires_git(
        &linked,
        &["a.txt".into()],
        &AtomicBool::new(false),
    ));
    let query = request(&[linked.to_str().unwrap()], "needle");
    let expected = run(query.clone(), Engine::GitGrep, Arc::default()).await;
    let actual = run(query, Engine::BuiltIn, Arc::default()).await;
    assert_eq!(actual.0, expected.0);
    assert_eq!(actual.2[0].state, expected.2[0].state);
    assert_eq!(actual.2[0].error, expected.2[0].error);
    assert_eq!(actual.2[0].state, State::Failed);
}

#[tokio::test]
async fn indexed_attributes_apply_to_filtered_builtin_queries() {
    let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("builtin-filtered-attributes");
    for (name, selected, attributes) in [
        ("root", "selected.txt", ".gitattributes"),
        (
            "nested",
            "nested space/selected.txt",
            "nested space/.gitattributes",
        ),
    ] {
        let path = repo(
            &fixture,
            name,
            &[
                (selected, b"needle\n"),
                (attributes, b"selected.txt binary\n"),
                ("outside.txt", b"needle\n"),
            ],
        );
        std::fs::remove_file(Path::new(&path).join(attributes)).unwrap();
        let mut query = request(&[&path], "needle");
        query.pathspecs = vec![selected.into()];
        let expected = run(query.clone(), Engine::GitGrep, Arc::default()).await;
        assert!(expected.1.is_empty());
        crate::git::clear_activity();
        let actual = run(query, Engine::BuiltIn, Arc::default()).await;
        assert_eq!(actual.1, expected.1, "{name}");
        assert_eq!(actual.0, expected.0, "{name}");
        assert!(actual.2[0]
            .engine_note
            .as_deref()
            .unwrap()
            .contains("attributes"));
        let activity: Vec<_> = crate::git::activity_snapshot()
            .into_iter()
            .map(|event| serde_json::to_value(event).unwrap())
            .collect();
        let count = |command: &str| {
            activity
                .iter()
                .filter(|event| {
                    event["argv"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|arg| arg == command)
                })
                .count()
        };
        assert_eq!(count("ls-files"), 1);
        assert_eq!(count("grep"), 1);
    }
}
