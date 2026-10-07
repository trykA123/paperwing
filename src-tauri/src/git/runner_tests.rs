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
