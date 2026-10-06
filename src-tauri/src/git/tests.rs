use super::*;
use tokio::io::AsyncWriteExt;
use std::cmp::Ordering;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering as AtomicOrdering}};
use std::time::Duration;

#[test]
fn natural_reference_order_preserves_numeric_and_case_ties() {
    let mut branches = vec!["release/10", "Release/02", "release/2", "release/1", "main"];
    branches.sort_by(|a, b| natural_cmp(a, b));
    assert_eq!(branches, ["main", "release/1", "Release/02", "release/2", "release/10"]);
    let mut tags = vec!["v1.9", "v2.0", "v1.10", "v01.10"];
    tags.sort_by(|a, b| natural_cmp(b, a));
    assert_eq!(tags, ["v2.0", "v1.10", "v01.10", "v1.9"]);
    assert_eq!(natural_cmp("v0002", "V2"), Ordering::Equal);
    assert_eq!(natural_cmp("release/", "release/1"), Ordering::Less);
}

#[tokio::test]
async fn child_exit_rejects_late_cancellation_during_drain() {
    let _guard = TEST_RUNNER_LOCK.lock().await;
    let accepted = Arc::new(AtomicBool::new(false));
    let requested = accepted.clone();
    let after_exit: ExitObserver = Arc::new(move |_, code| {
        assert_eq!(code, "0");
        let entry = activity_snapshot().into_iter().find(|entry| entry.context == "runner-late-cancel").unwrap();
        requested.store(cancel_activity(entry.id), AtomicOrdering::Relaxed);
        Ok(())
    });
    let result = execute_inner(Request { args: &["--version"], context: "runner-late-cancel", expected: &[0], timeout: Duration::from_secs(45), policy: OutputPolicy::Text }, None, None, None, Some(after_exit)).await;
    assert!(!accepted.load(AtomicOrdering::Relaxed), "late cancellation was accepted after exit 0");
    assert_eq!(result.unwrap().code, Some(0));
    let entry = activity_snapshot().into_iter().find(|entry| entry.context == "runner-late-cancel").unwrap();
    assert_eq!(entry.state, "completed");
    assert_eq!(entry.exit_code, Some(0));
}

#[tokio::test]
async fn stopped_outcome_survives_output_drain_failure() {
    let _guard = TEST_RUNNER_LOCK.lock().await;
    for state in ["cancelled", "timedOut"] {
        let context = format!("runner-drain-error-{state}");
        let observed_context = context.clone();
        let requested = Arc::new(AtomicBool::new(false));
        let observed = requested.clone();
        let observer: Observer = Arc::new(move |_, _| {
            if !observed.swap(true, AtomicOrdering::Relaxed) {
                let entry = activity_snapshot().into_iter().find(|entry| entry.context == observed_context).unwrap();
                assert!(cancel_activity(entry.id));
            }
        });
        let after_exit: ExitObserver = Arc::new(|_, _| Err("Injected Git output drain failure".into()));
        let timeout = if state == "timedOut" { Duration::ZERO } else { Duration::from_secs(45) };
        let result = execute_inner(Request { args: &["-c", "alias.skein-fixture-wait=!echo cancel-ready; sleep 2", "skein-fixture-wait"], context: &context, expected: &[0], timeout, policy: OutputPolicy::Text }, (state == "cancelled").then_some(observer), None, None, Some(after_exit)).await;
        assert_eq!(result.err().unwrap(), "Injected Git output drain failure");
        let entry = activity_snapshot().into_iter().find(|entry| entry.context == context).unwrap();
        assert_eq!(entry.state, state);
        #[cfg(windows)]
        assert!(entry.exit_code.is_some());
        #[cfg(unix)]
        assert!(entry.exit_code.is_none());
        if state == "cancelled" { assert!(requested.load(AtomicOrdering::Relaxed)); }
    }
}

#[tokio::test]
async fn ambiguous_authorities_redact_before_record_and_word_splitting() {
    for separator in [" ", "\t", "\r", "\n", "\r\n"] {
        let input = format!("https://prefix.test{separator}https://probe-user:probe-password@invalid.test/repo");
        let expected = format!("https://prefix.test{separator}https://[redacted]@invalid.test/repo");
        assert_eq!(redact(&input, &[]), expected);
        assert_eq!(safe(&input), expected);
        let error = last_error(&input);
        assert!(!error.contains("probe-user") && !error.contains("probe-password"), "{error:?}");
        let benign = format!("https://prefix.test{separator}https://invalid.test:443/repo");
        assert_eq!(redact(&benign, &[]), benign);
        for authority in [
            format!("probe-user:probe-password{separator}extra"),
            format!("probe-user{separator}:probe-password"),
        ] {
            let input = format!("https://prefix.test{separator}https://{authority}@invalid.test/repo");
            let clean = redact(&input, &[]);
            assert!(!clean.contains("probe-user") && !clean.contains("probe-password"), "{clean:?}");
            assert!(clean.starts_with(&format!("https://prefix.test{separator}https://[redacted]")));
        }
        let input = format!("https://probe-user:probe-password{separator}extra@invalid.test/repo");
        let clean = redact(&input, &[]);
        assert!(!clean.contains("probe-user") && !clean.contains("probe-password"), "{clean:?}");
        assert!(clean.contains(separator));
        assert!(!last_error(&input).contains("probe-password"));
        let input = format!("https://probe-user{separator}:probe-password@invalid.test/repo");
        let clean = redact(&input, &[]);
        assert!(!clean.contains("probe-user") && !clean.contains("probe-password"), "{clean:?}");
        assert!(clean.contains(separator));
    }
    for (input, expected) in [
        ("https://prefix.testhttps://probe-user:probe-password@invalid.test/repo", "https://[redacted]@invalid.test/repo"),
        ("https://prefix.test/repohttps://probe-user:probe-password@invalid.test/repo", "https://prefix.test/repohttps://[redacted]@invalid.test/repo"),
        ("https://prefix.test/repo,https://probe-user:probe-password@invalid.test/repo", "https://prefix.test/repo,https://[redacted]@invalid.test/repo"),
        ("https://probe-user:probe-password@invalid.test/repo https://prefix.test", "https://[redacted]@invalid.test/repo https://prefix.test"),
        ("https://probe-user:probe-password@invalid.test/repo https://prefix.test https://probe-user:probe-password@invalid.test/repo", "https://[redacted]@invalid.test/repo https://prefix.test https://[redacted]@invalid.test/repo"),
        ("https://probe-user:probe-password", "https://[redacted]"),
        ("https://probe-user:probe-password@extra@invalid.test/repo", "https://[redacted]@invalid.test/repo"),
        ("https://prefix.test probe-user:probe-passwordhttps://probe-user:probe-password@invalid.test/repo", "https://[redacted] @invalid.test/repo"),
        ("https://[::1]:443/repo", "https://[::1]:443/repo"),
        ("https://\u{4f8b}.test/repo", "https://\u{4f8b}.test/repo"),
        ("https://prefix.test/repo,https://invalid.test/repo", "https://prefix.test/repo,https://invalid.test/repo"),
    ] {
        let clean = redact(input, &[]);
        assert_eq!(clean, expected);
        assert!(!clean.contains("probe-user") && !clean.contains("probe-password"), "{clean:?}");
        assert_eq!(redact(&clean, &[]), clean);
    }
    assert_eq!(redact("https://user:pass@host/repo", &[]), "https://[redacted]@host/repo");
    assert_eq!(redact("https://host:443/repo", &[]), "https://host:443/repo");
    let configured = "https://prefix.test https://configured-user:configured-password@invalid.test/repo?configured-token";
    let clean = redact(configured, &["configured-user".into(), "configured-password".into(), "configured-token".into()]);
    for secret in ["configured-user", "configured-password", "configured-token"] { assert!(!clean.contains(secret), "{clean:?}"); }
    let (mut writer, reader) = tokio::io::duplex(32);
    let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
    let task = tokio::spawn(drain(reader, "stderr", sender, vec!["configured-token".into()], OutputPolicy::Text));
    let writing = tokio::spawn(async move {
        for chunk in ["https://probe-user:probe-", "password extra@invalid.test/repo\r\nhttps://probe-user:", "probe-password\r", "\nextra@invalid.test/repo\nhttps://probe-user\r", "\n:probe-password@invalid.test/repo\n"] {
            writer.write_all(chunk.as_bytes()).await.unwrap();
        }
        for separator in [" ", "\t", "\r", "\n", "\r\n"] {
            for chunk in [format!("https://prefix.test{separator}htt"), "ps://probe-user:probe-".into(), "password@invalid.test/repo\n".into()] {
                writer.write_all(chunk.as_bytes()).await.unwrap();
            }
            for chunk in [format!("https://prefix.test{separator}https://probe-user{separator}:probe-"), "password@invalid.test/repo\n".into()] {
                writer.write_all(chunk.as_bytes()).await.unwrap();
            }
        }
        for chunk in ["https://prefix.test/repohttps://probe-user:probe-", "password@invalid.test/repo\nconfigu", "red-token\n"] {
            writer.write_all(chunk.as_bytes()).await.unwrap();
        }
    });
    let mut lines = Vec::new();
    while let Some((_, text)) = receiver.recv().await { lines.push(text); }
    writing.await.unwrap();
    task.await.unwrap().unwrap();
    let emitted = lines.join("\n");
    assert!(!emitted.contains("probe-user") && !emitted.contains("probe-password"), "{emitted}");
    assert!(!emitted.contains("configured-token"), "{emitted}");
    for separator in [" ", "\t", "\r", "\n", "\r\n"] {
        assert!(lines.contains(&format!("https://prefix.test{separator}https://[redacted]@invalid.test/repo")), "{lines:?}");
    }
    assert!(lines.contains(&"https://prefix.test/repohttps://[redacted]@invalid.test/repo".into()));
}

#[tokio::test]
async fn concurrent_streams_redact_split_records_and_bound_content() {
    let (mut stdout_writer, stdout_reader) = tokio::io::duplex(64);
    let (mut stderr_writer, stderr_reader) = tokio::io::duplex(64);
    let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
    let stdout_task = tokio::spawn(drain(stdout_reader, "stdout", sender.clone(), vec![], OutputPolicy::Text));
    let stderr_task = tokio::spawn(drain(stderr_reader, "stderr", sender, vec![], OutputPolicy::Text));
    let stdout_writing = tokio::spawn(async move { stdout_writer.write_all(&"useful stdout\n".repeat(1000).into_bytes()).await.unwrap(); });
    let stderr_writing = tokio::spawn(async move { stderr_writer.write_all(&"useful stderr\r".repeat(1000).into_bytes()).await.unwrap(); });
    let mut counts = [0, 0];
    while let Some((stream, _)) = receiver.recv().await { counts[usize::from(stream == "stderr")] += 1; }
    stdout_writing.await.unwrap(); stderr_writing.await.unwrap();
    assert_eq!(counts, [1000, 1000]);
    assert_eq!(stdout_task.await.unwrap().unwrap().1, 14000);
    assert_eq!(stderr_task.await.unwrap().unwrap().1, 14000);
    let (mut writer, reader) = tokio::io::duplex(64);
    let (sender, mut receiver) = tokio::sync::mpsc::channel(32);
    let task = tokio::spawn(drain(reader, "stderr", sender, vec!["private-token".into()], OutputPolicy::Text));
    let writing = tokio::spawn(async move {
        for chunk in ["https://user:pass@host/repo?secret=yes pri", "vate-token\rAuthorization: Bearer hidden\nlast"] {
            writer.write_all(chunk.as_bytes()).await.unwrap();
        }
    });
    let mut lines = Vec::new();
    while let Some((stream, text)) = receiver.recv().await { assert_eq!(stream, "stderr"); lines.push(text); }
    writing.await.unwrap();
    let (_, size, truncated) = task.await.unwrap().unwrap();
    assert!(size > 0);
    assert!(!truncated);
    let text = lines.join("\n");
    for secret in ["user:pass", "secret=yes", "private-token", "hidden"] { assert!(!text.contains(secret), "{secret}"); }
    assert!(text.contains("last"));
    let (mut writer, reader) = tokio::io::duplex(128);
    let (sender, mut receiver) = tokio::sync::mpsc::channel(32);
    let task = tokio::spawn(drain(reader, "stdout", sender, vec![], OutputPolicy::Text));
    let writing = tokio::spawn(async move { writer.write_all(&vec![b'x'; 20000]).await.unwrap(); });
    assert_eq!(receiver.recv().await.unwrap().1, "[oversized output record omitted]");
    writing.await.unwrap();
    assert!(task.await.unwrap().unwrap().2);
}

#[tokio::test]
async fn real_git_tracks_failure_clone_progress_timeout_and_clear() {
    let _guard = TEST_RUNNER_LOCK.lock().await;
    let root = std::env::temp_dir().join(format!("skein-runner-{}-{}", std::process::id(), NEXT_ID.fetch_add(1, AtomicOrdering::Relaxed)));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("source");
    let clone = root.join("clone");
    let source = source.to_str().unwrap();
    let clone = clone.to_str().unwrap();
    assert_eq!(buffered(&["init", "--initial-branch=main", source], "runner-test", &[0]).await.unwrap().code, Some(0));
    std::fs::write(std::path::Path::new(source).join("file.txt"), "fixture\n").unwrap();
    buffered(&["-C", source, "add", "file.txt"], "runner-test", &[0]).await.unwrap();
    assert_eq!(buffered(&["-C", source, "-c", "user.name=Fixture", "-c", "user.email=fixture@example.test", "commit", "-m", "fixture"], "runner-test", &[0]).await.unwrap().code, Some(0));
    let lines = Arc::new(Mutex::new(Vec::new()));
    let observed = lines.clone();
    let callback: Observer = Arc::new(move |stream, text| {
        observed.lock().unwrap().push((stream.to_string(), text.to_string()));
        let cleared = clear_activity();
        assert!(cleared.running.iter().any(|entry| entry.context == "runner-clone"));
    });
    let result = execute(Request { args: &["clone", "--no-local", "--progress", "--", source, clone], context: "runner-clone", expected: &[0], timeout: Duration::from_secs(45), policy: OutputPolicy::Text }, Some(callback)).await.unwrap();
    assert_eq!(activity_snapshot().iter().filter(|entry| entry.context == "runner-clone").count(), 1);
    assert_eq!(result.code, Some(0));
    assert!(lines.lock().unwrap().iter().any(|(stream, text)| stream == "stderr" && text.contains("Receiving objects:")));
    let status = buffered(&["-C", clone, "status", "--branch"], "runner-status", &[0]).await.unwrap();
    assert!(String::from_utf8_lossy(&status.stdout).contains("main"));
    let failure = buffered(&["-C", clone, "not-a-command"], "runner-failure", &[0]).await.unwrap();
    assert_ne!(failure.code, Some(0));
    assert!(!failure.stderr.is_empty());
    assert!(activity_snapshot().iter().any(|entry| entry.context == "runner-failure" && entry.state == "failed"));
    buffered(&["-C", clone, "branch", "feature"], "runner-tree-fixture", &[0]).await.unwrap();
    buffered(&["-C", clone, "tag", "v1"], "runner-tree-fixture", &[0]).await.unwrap();
    std::fs::write(std::path::Path::new(clone).join("file.txt"), "stash fixture\n").unwrap();
    buffered(&["-C", clone, "-c", "user.name=Fixture", "-c", "user.email=fixture@example.test", "stash", "push", "-m", "fixture stash"], "runner-tree-fixture", &[0]).await.unwrap();
    let commit = buffered(&["-C", clone, "rev-parse", "HEAD"], "runner-tree-fixture", &[0]).await.unwrap();
    let commit = String::from_utf8_lossy(&commit.stdout).trim().to_string();
    buffered(&["-C", clone, "update-index", "--add", "--cacheinfo", &format!("160000,{commit},nested")], "runner-tree-fixture", &[0]).await.unwrap();
    buffered(&["-C", clone, "config", "remote.origin.url", "https://probe-user:probe-password extra@invalid.test/repo"], "runner-tree-fixture", &[0]).await.unwrap();
    buffered(&["-C", clone, "config", "remote.split.url", "https://probe-user\r\n:probe-password@invalid.test/repo"], "runner-tree-fixture", &[0]).await.unwrap();
    let modules = std::path::Path::new(clone).join(".gitmodules");
    std::fs::write(&modules, "[submodule \"nested\"]\npath = nested\nurl = \"https://probe-user:probe-password\\n extra@invalid.test/module?credential=private\"\n[include]\npath = unavailable-do-not-follow\n").unwrap();
    let observer_lines = Arc::new(Mutex::new(Vec::new()));
    for (name, separator) in [("z-multi-space", " "), ("z-multi-crlf", "\r\n")] {
        let input = format!("https://prefix.test{separator}https://probe-user:probe-password@invalid.test/repo");
        buffered(&["-C", clone, "config", &format!("remote.{name}.url"), &input], &input, &[0]).await.unwrap();
        buffered(&["-C", clone, "update-index", "--add", "--cacheinfo", &format!("160000,{commit},{name}")], "runner-tree-fixture", &[0]).await.unwrap();
        buffered(&["-C", clone, "config", "--no-includes", "--file", modules.to_str().unwrap(), &format!("submodule.{name}.path"), name], "runner-tree-fixture", &[0]).await.unwrap();
        buffered(&["-C", clone, "config", "--no-includes", "--file", modules.to_str().unwrap(), &format!("submodule.{name}.url"), &input], &input, &[0]).await.unwrap();
        let observed = observer_lines.clone();
        let observer: Observer = Arc::new(move |_, text| { observed.lock().unwrap().push(text.to_string()); });
        let error = execute(Request { args: &["-C", clone, &input], context: &input, expected: &[0], timeout: Duration::from_secs(45), policy: OutputPolicy::Text }, Some(observer)).await.unwrap();
        assert_ne!(error.code, Some(0));
        let message = last_error(&String::from_utf8_lossy(&error.stderr));
        assert!(!message.contains("probe-user") && !message.contains("probe-password"), "{message:?}");
    }
    let benign = "https://prefix.test https://invalid.test:443/repo";
    buffered(&["-C", clone, "config", "remote.z-benign.url", benign], "runner-tree-fixture", &[0]).await.unwrap();
    let index = std::fs::read(std::path::Path::new(clone).join(".git/index")).unwrap();
    let head = std::fs::read(std::path::Path::new(clone).join(".git/HEAD")).unwrap();
    let tree = repository_tree(clone.into()).await.unwrap();
    assert!(tree.branches.iter().any(|reference| reference.name == "main" && reference.current));
    assert!(tree.branches.iter().any(|reference| reference.name == "feature"));
    assert_eq!(tree.tags[0].name, "v1");
    assert_eq!(tree.remotes[0].name, "origin");
    assert!(!tree.remotes[0].refs.is_empty());
    assert!(!tree.remotes[0].urls[0].contains("probe-password"));
    assert_eq!(tree.stashes[0].name, "stash@{0}");
    assert_eq!(tree.submodules[0].path, "nested");
    let url = tree.submodules[0].url.as_ref().unwrap();
    assert!(!url.contains("password") && !url.contains("credential=private"));
    let serialized = serde_json::to_string(&tree).unwrap();
    assert!(!serialized.contains("probe-user") && !serialized.contains("probe-password"), "{serialized}");
    for (name, separator) in [("z-multi-space", " "), ("z-multi-crlf", "\r\n")] {
        let expected = format!("https://prefix.test{separator}https://[redacted]@invalid.test/repo");
        let remote = tree.remotes.iter().find(|remote| remote.name == name).unwrap();
        assert_eq!(remote.urls, expected.lines().map(str::to_string).collect::<Vec<_>>());
        let module = tree.submodules.iter().find(|module| module.path == name).unwrap();
        assert_eq!(module.url.as_deref().map(|url| url.replace('\r', "")), Some(expected.replace('\r', "")));
    }
    assert_eq!(tree.remotes.iter().find(|remote| remote.name == "z-benign").unwrap().urls, [benign]);
    {
        let observed = observer_lines.lock().unwrap();
        assert!(observed.len() >= 2);
        for text in observed.iter() {
            assert!(!text.contains("probe-user") && !text.contains("probe-password"), "{text:?}");
        }
    }
    let activities = serde_json::to_string(&activity_snapshot()).unwrap();
    assert!(!activities.contains("probe-user") && !activities.contains("probe-password"), "{activities}");
    assert_eq!(std::fs::read(std::path::Path::new(clone).join(".git/index")).unwrap(), index);
    assert_eq!(std::fs::read(std::path::Path::new(clone).join(".git/HEAD")).unwrap(), head);
    assert!(ls_remote(source).await.unwrap().0.iter().any(|(name, _)| name == "main"));
    assert!(activity_snapshot().iter().flat_map(|entry| &entry.output).all(|output| !output.text.contains("credential=private") && !output.text.contains("password")));
    assert!(valid_ref("main..unsafe").is_err());
    assert!(valid_ref("-option").is_err());
    assert!(valid_path(&root.join("../escape").to_string_lossy(), false).is_err());
    assert!(repository_tree(root.join("missing").to_string_lossy().into()).await.is_err());
    let timeout = execute(Request { args: &["-c", "alias.skein-fixture-timeout=!sleep 2", "skein-fixture-timeout"], context: "runner-timeout", expected: &[0], timeout: Duration::ZERO, policy: OutputPolicy::Text }, None).await;
    assert!(timeout.is_err());
    assert!(activity_snapshot().iter().any(|entry| entry.context == "runner-timeout" && entry.state == "timedOut"));
    let requested = Arc::new(AtomicBool::new(false));
    let observed = requested.clone();
    let cancel_callback: Observer = Arc::new(move |_, _| {
        if !observed.swap(true, AtomicOrdering::Relaxed) {
            let entry = activity_snapshot().into_iter().find(|entry| entry.context == "runner-cancel" && entry.state == "running").unwrap();
            assert!(cancel_activity(entry.id));
        }
    });
    let cancelled = execute(Request { args: &["-c", "alias.skein-fixture-wait=!echo cancel-ready; sleep 2", "skein-fixture-wait"], context: "runner-cancel", expected: &[0], timeout: Duration::from_secs(45), policy: OutputPolicy::Text }, Some(cancel_callback)).await;
    assert!(cancelled.is_err());
    assert!(requested.load(AtomicOrdering::Relaxed));
    assert!(activity_snapshot().iter().any(|entry| entry.context == "runner-cancel" && entry.state == "cancelled"));
    let entries = activity_snapshot();
    assert!(entries.iter().filter(|entry| entry.context == "runner-clone").count() <= 1);
    for entry in entries { assert!(entry.output.windows(2).all(|pair| pair[0].sequence < pair[1].sequence)); }
    assert!(clear_activity().running.iter().all(|entry| !entry.context.starts_with("runner-")));
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn inaccessible_credentials_allow_quiet_git_without_observer_or_diagnostics() {
    let _serial = TEST_RUNNER_LOCK.lock().await;
    for reason in ["locked", "unavailable"] {
        let _credentials = CredentialFixture::new(std::collections::BTreeMap::from([
            ("a-readable-owner".into(), Ok(Some("known-synthetic-token".into()))),
            ("api-owner".into(), Err(reason.into())),
            ("plain-manual".into(), Ok(None)),
        ]));
        let observed = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let records = observed.clone();
        let alias = "alias.skein-quiet-fixture=!printf 'unknown-secret stdout\\n'; printf 'unknown-secret stderr\\n' >&2; exit 7";
        let output = execute(Request { args: &["-c", alias, "skein-quiet-fixture"], context: "unknown-secret context",
            expected: &[0], timeout: Duration::from_secs(45), policy: OutputPolicy::Text },
            Some(Arc::new(move |_, text| records.lock().unwrap().push(text.into())))).await.unwrap();
        assert_eq!(output.code, Some(7));
        assert_eq!(output.stdout, b"unknown-secret stdout\n");
        assert!(output.stderr.is_empty());
        assert!(observed.lock().unwrap().is_empty());
        assert_eq!(output.safe("unknown-secret"), "Git output omitted because the credential store is inaccessible.");
        assert!(!output.last_error().contains("unknown-secret"));
        let activity = activity_snapshot().into_iter().last().unwrap();
        assert_eq!(activity.context, "Git operation");
        assert!(activity.argv.is_empty());
        assert!(activity.output.is_empty());
        assert_eq!(activity.state, "failed");
        assert!(!serde_json::to_string(&activity).unwrap().contains("unknown-secret"));
    }
}

#[tokio::test]
async fn operation_redaction_survives_token_replacement_and_preserves_raw_bytes() {
    let _serial = TEST_RUNNER_LOCK.lock().await;
    let _credentials = CredentialFixture::new(std::collections::BTreeMap::from([
        ("managed-manual".into(), Ok(Some("synthetic-old-token".into()))),
    ]));
    let observed = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let records = observed.clone();
    let alias = "alias.skein-capture-fixture=!printf 'synthetic-old-token stdout\\n'; printf 'synthetic-old-token stderr\\n' >&2";
    let output = execute(Request { args: &["-c", alias, "skein-capture-fixture"], context: "synthetic-old-token context",
        expected: &[0], timeout: Duration::from_secs(45), policy: OutputPolicy::Text },
        Some(Arc::new(move |_, text| {
            CredentialFixture::replace("managed-manual", "synthetic-new-token");
            records.lock().unwrap().push(text.into());
        }))).await.unwrap();
    assert_eq!(output.stdout, b"synthetic-old-token stdout\n");
    assert!(!output.safe(&String::from_utf8_lossy(&output.stdout)).contains("synthetic-old-token"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("synthetic-old-token"));
    assert!(observed.lock().unwrap().iter().all(|text| !text.contains("synthetic-old-token")));
    let activity = activity_snapshot().into_iter().last().unwrap();
    assert!(!serde_json::to_string(&activity).unwrap().contains("synthetic-old-token"));
    let bytes = b"synthetic-new-token\0blob\xff";
    let alias = "alias.skein-byte-fixture=!printf 'synthetic-new-token\\0blob\\377'";
    let output = execute(Request { args: &["-c", alias, "skein-byte-fixture"], context: "byte capture",
        expected: &[0], timeout: Duration::from_secs(45), policy: OutputPolicy::Metadata }, None).await.unwrap();
    assert_eq!(output.stdout, bytes);
}

struct DisplayFixture(std::path::PathBuf);

impl Drop for DisplayFixture {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}

#[tokio::test]
async fn displayed_git_metadata_redacts_managed_manual_tokens_and_keeps_action_names() {
    let _serial = TEST_RUNNER_LOCK.lock().await;
    let _credentials = CredentialFixture::new(std::collections::BTreeMap::from([
        ("managed-manual".into(), Ok(Some("synthetic-display-token".into()))),
    ]));
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("skein-display-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let fixture = DisplayFixture(dir);
    let dir = &fixture.0;
    let path = dir.to_str().unwrap();
    let run = |args: &[&str]| {
        let output = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
        assert!(output.status.success(), "fixture Git failed");
    };
    run(&["init", "-q", "-b", "synthetic-display-token-branch"]);
    run(&["config", "user.name", "synthetic-display-token author"]);
    run(&["config", "user.email", "fixture@example.test"]);
    run(&["config", "commit.gpgsign", "false"]);
    run(&["config", "core.hooksPath", ""]);
    std::fs::write(dir.join("file.txt"), b"synthetic-display-token file\n").unwrap();
    run(&["add", "file.txt"]);
    run(&["commit", "-qm", "synthetic-display-token subject"]);
    run(&["tag", "synthetic-display-token-tag"]);
    run(&["config", "remote.origin.url", "https://invalid.test/synthetic-display-token"]);
    std::fs::write(dir.join("file.txt"), b"synthetic-display-token changed\n").unwrap();
    run(&["stash", "push", "-qm", "synthetic-display-token stash"]);
    let tree = repository_tree(path.into()).await.unwrap();
    assert_eq!(tree.branches[0].name, "synthetic-display-token-branch");
    assert_eq!(tree.branches[0].label, "[redacted]-branch");
    assert_eq!(tree.tags[0].name, "synthetic-display-token-tag");
    assert!(!tree.stashes[0].subject.contains("synthetic-display-token"));
    assert!(!tree.remotes[0].urls[0].contains("synthetic-display-token"));
    let refs = get_refs_many(vec![path.into()]).await;
    let refs = serde_json::to_value(refs).unwrap();
    assert_eq!(refs[0]["branches"][0], "synthetic-display-token-branch");
    assert_eq!(refs[0]["branchLabels"][0], "[redacted]-branch");
    assert_eq!(refs[0]["tagLabels"][0], "[redacted]-tag");
    let local = serde_json::to_value(crate::local::local_status(vec![path.into()]).await).unwrap();
    assert_eq!(local[0]["branch"], "synthetic-display-token-branch");
    assert_eq!(local[0]["branchLabel"], "[redacted]-branch");
    let duplicate = crate::commit::create_branch(path.into(), "synthetic-display-token-branch".into(), None, false).await.unwrap_err();
    assert!(!duplicate.contains("synthetic-display-token"));
    let missing = crate::commit::delete_branch(path.into(), "synthetic-display-token-missing".into(), false).await.unwrap_err();
    assert!(!missing.contains("synthetic-display-token"));
    let missing = crate::commit::create_branch(path.into(), "candidate".into(), Some("synthetic-display-token-missing".into()), false).await.unwrap_err();
    assert!(!missing.contains("synthetic-display-token"));
    let changes = serde_json::to_value(crate::commit::repo_changes(path.into()).await.unwrap()).unwrap();
    assert_eq!(changes["branch"], "[redacted]-branch");
    assert!(!changes["author"].as_str().unwrap().contains("synthetic-display-token"));
    std::fs::write(dir.join("file.txt"), b"synthetic-display-token next\n").unwrap();
    run(&["add", "file.txt"]);
    let commit = serde_json::to_value(crate::commit::commit_staged(path.into(), "synthetic-display-token next subject".into()).await.unwrap()).unwrap();
    assert_eq!(commit["subject"], "[redacted] next subject");
    let content = serde_json::to_value(crate::commit::change_content(path.into(), "file.txt".into(), None, "staged".into()).await.unwrap()).unwrap();
    assert_eq!(content["original"], "synthetic-display-token next\n");
    drop(_credentials);
    let _credentials = CredentialFixture::new(std::collections::BTreeMap::from([("api-owner".into(), Err("locked".into()))]));
    let content = serde_json::to_value(crate::commit::change_content(path.into(), "new-file.txt".into(), None, "unstaged".into()).await.unwrap()).unwrap();
    assert_eq!(content["original"], "");
    let tree = repository_tree(path.into()).await.unwrap();
    assert_eq!(tree.branches[0].name, "synthetic-display-token-branch");
    assert!(tree.branches[0].label.contains("omitted"));
    assert!(tree.stashes[0].subject.contains("omitted"));
}

#[cfg(windows)]
#[test]
fn valid_path_ignores_the_process_working_directory_on_the_same_drive() {
    const CHILD: &str = "SKEIN_VALID_PATH_CHILD";
    if let Ok(path) = std::env::var(CHILD) {
        assert_eq!(valid_path(&path, true), Ok(()), "drive-relative cwd leaked into validation");
        return;
    }
    let fixture = crate::platform::Fixture::new("valid-path-cwd");
    let repo = fixture.0.join("repo");
    let linked = fixture.0.join("linked-cwd");
    std::fs::create_dir(&repo).unwrap();
    std::fs::create_dir(fixture.0.join("target")).unwrap();
    let made = std::process::Command::new("cmd").arg("/c").arg("mklink").arg("/J").arg(&linked).arg(fixture.0.join("target")).output().unwrap();
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stdout));
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "git::tests::valid_path_ignores_the_process_working_directory_on_the_same_drive", "--nocapture"])
        .env(CHILD, &repo).current_dir(&linked).output().unwrap();
    assert!(child.status.success(), "{}{}", String::from_utf8_lossy(&child.stdout), String::from_utf8_lossy(&child.stderr));
}

#[tokio::test]
async fn redaction_secrets_are_read_once_per_credential_revision() {
    use std::sync::atomic::Ordering;
    let _serial = TEST_RUNNER_LOCK.lock().await;
    let _credentials = CredentialFixture::new(std::collections::BTreeMap::from([
        ("cache-owner-a".into(), Ok(Some("synthetic-cache-a".into()))),
        ("cache-owner-b".into(), Ok(Some("synthetic-cache-b".into()))),
    ]));
    let run = || async {
        execute(Request { args: &["--version"], context: "cache", expected: &[0],
            timeout: Duration::from_secs(45), policy: OutputPolicy::Text }, None).await.unwrap()
    };
    let before = super::runner::SECRET_READS.load(Ordering::SeqCst);
    for _ in 0..5 { run().await; }
    assert_eq!(super::runner::SECRET_READS.load(Ordering::SeqCst) - before, 2);
    CredentialFixture::replace("cache-owner-a", "synthetic-cache-a2");
    for _ in 0..5 { run().await; }
    assert_eq!(super::runner::SECRET_READS.load(Ordering::SeqCst) - before, 4);
}
