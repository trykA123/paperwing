use super::*;

#[cfg(unix)]
use std::path::{Path, PathBuf};

#[cfg(unix)]
fn shim(name: &str) -> (PathBuf, PathBuf) {
    let root = crate::test_support::tmp_root()
        .join("runner-shim")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let log = root.join("spawns.log");
    let program = root.join("git-shim");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\necho \"$*\" >> {}\nexec git \"$@\"\n",
            log.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    (program, log)
}

#[cfg(unix)]
fn lines(log: &Path) -> usize {
    std::fs::read_to_string(log)
        .map(|text| text.lines().count())
        .unwrap_or(0)
}

#[cfg(unix)]
#[tokio::test]
async fn one_call_starts_exactly_one_git_child() {
    let _runner = TEST_RUNNER_LOCK.lock().await;
    let (program, log) = shim("one-child");
    let _override = binary::BinaryOverride::new(program);
    let output = buffered(&["--version"], "one-child", &[0]).await.unwrap();
    assert_eq!(output.code, Some(0));
    assert_eq!(lines(&log), 1);
}

#[tokio::test]
async fn safe_redacts_without_reading_the_keyring() {
    let _runner = TEST_RUNNER_LOCK.lock().await;
    let fixture = CredentialFixture::new(std::collections::BTreeMap::from([(
        "safe-owner".to_string(),
        Ok(Some("synthetic-safe-token".to_string())),
    )]));
    buffered(&["--version"], "safe-warm", &[0]).await.unwrap();
    drop(fixture);
    configure_sources(vec!["safe-owner".into()]);
    let _held = crate::credentials::hold_slot();
    assert_eq!(super::super::safe("a synthetic-safe-token b"), "a [redacted] b");
    configure_sources(Vec::new());
}

#[cfg(unix)]
#[tokio::test]
async fn a_500_line_command_emits_at_most_three_deltas_that_rebuild_every_line() {
    let _runner = TEST_RUNNER_LOCK.lock().await;
    let context = "delta-500-lines";
    let request = Request { args: &["-c", "alias.skein-lines=!seq 1 500", "skein-lines"], context, expected: &[0], timeout: Duration::from_secs(45), policy: OutputPolicy::Text };
    execute(request, None).await.unwrap();
    let id = activity_snapshot().into_iter().find(|entry| entry.context == context).unwrap().id;
    let events: Vec<_> = activity::TEST_EVENTS.lock().unwrap().iter().filter(|event| event["id"] == id.as_str()).cloned().collect();
    assert!(events.len() <= 3, "{} events", events.len());
    let mut lines: Vec<String> = Vec::new();
    for event in &events {
        assert_eq!(event["from"], lines.len());
        lines.extend(event["lines"].as_array().unwrap().iter().map(|line| line["text"].as_str().unwrap().to_string()));
    }
    assert_eq!(lines, (1..=500).map(|number| number.to_string()).collect::<Vec<_>>());
    assert_eq!(events.last().unwrap()["state"], "completed");
}

#[cfg(unix)]
#[tokio::test]
async fn a_throttled_line_is_flushed_while_the_command_is_still_running() {
    let _runner = TEST_RUNNER_LOCK.lock().await;
    let context = "delta-flush-timer";
    let request = Request { args: &["-c", "alias.skein-slow=!echo first; sleep 1; echo second", "skein-slow"], context, expected: &[0], timeout: Duration::from_secs(45), policy: OutputPolicy::Text };
    execute(request, None).await.unwrap();
    let id = activity_snapshot().into_iter().find(|entry| entry.context == context).unwrap().id;
    let events: Vec<_> = activity::TEST_EVENTS.lock().unwrap().iter().filter(|event| event["id"] == id.as_str()).cloned().collect();
    let running_first = events.iter().any(|event| event["state"] == "running" && event["lines"].as_array().unwrap().iter().any(|line| line["text"] == "first"));
    assert!(running_first, "first line only arrived with the final event");
}
