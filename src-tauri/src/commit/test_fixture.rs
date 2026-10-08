use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

pub(crate) struct Fixture {
    pub base: PathBuf,
    pub root: PathBuf,
}
impl Fixture {
    pub fn with_root_name(name: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let base = crate::test_support::tmp_root().join(format!(
            "partial-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let root = base.join(name);
        std::fs::create_dir_all(&root).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let fixture = Self { base, root };
        fixture.git(&["init", "-q", "-b", "main"]);
        fixture.git(&["config", "core.autocrlf", "false"]);
        fixture.git(&["config", "user.name", "admin"]);
        fixture.git(&["config", "user.email", "admin@example.test"]);
        fixture.git(&["config", "core.hooksPath", ""]);
        fixture.git(&["config", "commit.gpgSign", "false"]);
        fixture
    }
    pub fn new() -> Self {
        Self::with_root_name("repo with spaces")
    }
    pub fn path(&self) -> String {
        self.root.to_str().unwrap().to_string()
    }
    pub fn git(&self, args: &[&str]) -> Vec<u8> {
        let output = Command::new("git")
            .env_remove("GIT_INDEX_FILE")
            .arg("-C")
            .arg(&self.root)
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
    pub fn write(&self, file: &str, bytes: &[u8]) {
        std::fs::write(self.root.join(file), bytes).unwrap();
    }
    pub fn commit(&self, file: &str, bytes: &[u8]) {
        self.write(file, bytes);
        self.git(&["add", "--", file]);
        self.git(&["commit", "-qm", "base"]);
    }

    pub fn git_input(&self, args: &[&str], input: &[u8]) -> Vec<u8> {
        let mut child = Command::new("git")
            .env_remove("GIT_INDEX_FILE")
            .arg("-C")
            .arg(&self.root)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }
    pub fn apply(&self, patch: &[u8], reverse: bool) {
        let mut command = Command::new("git");
        command
            .env_remove("GIT_INDEX_FILE")
            .arg("-C")
            .arg(&self.root)
            .args(["apply", "--cached", "--whitespace=nowarn"]);
        if reverse {
            command.arg("--reverse");
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(patch).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(patch)
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.base).unwrap();
    }
}

pub(crate) fn two_hunks() -> (Vec<u8>, Vec<u8>) {
    let before = (0..24)
        .map(|index| format!("line {index}\r\n"))
        .collect::<String>();
    let after = before
        .replace("line 2\r\n", "first change\r\n")
        .replace("line 20\r\n", "second change\r\n");
    (before.into_bytes(), after.into_bytes())
}
