use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GitBinary {
    pub(super) program: PathBuf,
    pub(super) path_prefix: Vec<PathBuf>,
}

impl GitBinary {
    fn plain() -> Self {
        Self {
            program: PathBuf::from("git"),
            path_prefix: Vec::new(),
        }
    }

    pub(super) fn path_value(&self, current: Option<OsString>) -> Option<OsString> {
        if self.path_prefix.is_empty() {
            return None;
        }
        let rest = current.iter().flat_map(std::env::split_paths);
        std::env::join_paths(self.path_prefix.iter().cloned().chain(rest)).ok()
    }
}

const RUNTIME_DIRS: [&str; 3] = ["mingw64", "mingw32", "clangarm64"];

pub(super) fn from_exec_path(exec_path: &Path) -> GitBinary {
    let runtime = exec_path.ancestors().find(|dir| {
        dir.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| RUNTIME_DIRS.contains(&name))
    });
    let prefix = match runtime {
        Some(dir) => dir.parent(),
        None => exec_path.parent().and_then(Path::parent),
    };
    let mut candidates = vec![exec_path.join("git.exe")];
    if let Some(dir) = runtime {
        candidates.push(dir.join("bin").join("git.exe"));
    }
    if let Some(prefix) = prefix {
        candidates.push(prefix.join("bin").join("git.exe"));
        candidates.push(prefix.join("mingw64").join("bin").join("git.exe"));
    }
    let Some(program) = candidates.into_iter().find(|candidate| candidate.is_file()) else {
        return GitBinary::plain();
    };
    let path_prefix = prefix
        .map(|prefix| {
            [
                prefix.join("mingw64").join("bin"),
                prefix.join("usr").join("bin"),
            ]
            .into_iter()
            .filter(|dir| dir.is_dir())
            .collect()
        })
        .unwrap_or_default();
    GitBinary {
        program,
        path_prefix,
    }
}

#[cfg(windows)]
fn discover() -> GitBinary {
    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new("git")
        .arg("--exec-path")
        .creation_flags(0x0800_0000)
        .output();
    let Ok(output) = output else {
        return GitBinary::plain();
    };
    if !output.status.success() {
        return GitBinary::plain();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.trim();
    if line.is_empty() {
        GitBinary::plain()
    } else {
        from_exec_path(Path::new(line))
    }
}

#[cfg(not(windows))]
fn discover() -> GitBinary {
    GitBinary::plain()
}

#[cfg(test)]
static OVERRIDE: std::sync::Mutex<Option<GitBinary>> = std::sync::Mutex::new(None);

pub(super) fn current() -> GitBinary {
    #[cfg(test)]
    if let Some(binary) = OVERRIDE.lock().unwrap().clone() {
        return binary;
    }
    static BINARY: OnceLock<GitBinary> = OnceLock::new();
    BINARY.get_or_init(discover).clone()
}

#[cfg(test)]
pub(crate) struct BinaryOverride;

#[cfg(test)]
impl BinaryOverride {
    pub(crate) fn new(program: PathBuf) -> Self {
        let previous = OVERRIDE.lock().unwrap().replace(GitBinary {
            program,
            path_prefix: Vec::new(),
        });
        assert!(previous.is_none(), "git binary override is not reentrant");
        Self
    }
}

#[cfg(test)]
impl Drop for BinaryOverride {
    fn drop(&mut self) {
        *OVERRIDE.lock().unwrap() = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(name: &str, files: &[&str], dirs: &[&str]) -> PathBuf {
        let root = crate::test_support::tmp_root()
            .join("git-binary")
            .join(name);
        let _ = std::fs::remove_dir_all(&root);
        for dir in dirs {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        for file in files {
            std::fs::create_dir_all(root.join(file).parent().unwrap()).unwrap();
            std::fs::write(root.join(file), b"").unwrap();
        }
        root
    }

    #[test]
    fn prefers_the_exec_path_binary_and_prepends_runtime_dirs() {
        let root = layout(
            "standard",
            &[
                "Git/mingw64/libexec/git-core/git.exe",
                "Git/mingw64/bin/git.exe",
                "Git/cmd/git.exe",
            ],
            &["Git/usr/bin"],
        );
        let binary = from_exec_path(&root.join("Git/mingw64/libexec/git-core"));
        assert_eq!(
            binary.program,
            root.join("Git/mingw64/libexec/git-core/git.exe")
        );
        assert_eq!(
            binary.path_prefix,
            vec![root.join("Git/mingw64/bin"), root.join("Git/usr/bin")]
        );
    }

    #[test]
    fn falls_back_to_the_runtime_bin_binary() {
        let root = layout(
            "bin",
            &["Scoop/apps/git/current/mingw64/bin/git.exe"],
            &["Scoop/apps/git/current/usr/bin"],
        );
        let binary = from_exec_path(&root.join("Scoop/apps/git/current/mingw64/libexec/git-core"));
        assert_eq!(
            binary.program,
            root.join("Scoop/apps/git/current/mingw64/bin/git.exe")
        );
        assert_eq!(binary.path_prefix.len(), 2);
    }

    #[test]
    fn falls_back_to_the_prefix_bin_binary_without_a_runtime_dir() {
        let root = layout("prefix", &["Portable/bin/git.exe"], &[]);
        let binary = from_exec_path(&root.join("Portable/libexec/git-core"));
        assert_eq!(binary.program, root.join("Portable/bin/git.exe"));
        assert!(binary.path_prefix.is_empty());
    }

    #[test]
    fn falls_back_to_plain_git_when_no_binary_exists() {
        let root = layout("none", &[], &["Git/mingw64/libexec/git-core"]);
        assert_eq!(
            from_exec_path(&root.join("Git/mingw64/libexec/git-core")),
            GitBinary::plain()
        );
    }

    #[test]
    fn path_value_puts_the_prefix_first() {
        let binary = GitBinary {
            program: "git".into(),
            path_prefix: vec!["/a".into(), "/b".into()],
        };
        let value = binary
            .path_value(Some(std::env::join_paths(["/c", "/d"]).unwrap()))
            .unwrap();
        let parts: Vec<_> = std::env::split_paths(&value).collect();
        assert_eq!(parts, ["/a", "/b", "/c", "/d"].map(PathBuf::from));
        assert!(GitBinary::plain().path_value(Some("x".into())).is_none());
    }
}
