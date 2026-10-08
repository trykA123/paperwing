use std::path::{Component, Path};

const IGNORED_DIRECTORIES: [&str; 4] = ["node_modules", "target", "dist", "build"];
const GIT_FILES: [&str; 3] = ["HEAD", "index", "packed-refs"];

pub(super) fn is_relevant(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let parts: Vec<&str> = relative
        .components()
        .filter_map(|part| match part {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    match parts.as_slice() {
        [] => false,
        [".git", rest @ ..] => is_git_signal(rest),
        parts => !parts.iter().any(|part| IGNORED_DIRECTORIES.contains(part)),
    }
}

fn is_git_signal(parts: &[&str]) -> bool {
    match parts {
        [name] => GIT_FILES.contains(name),
        ["refs", .., last] => !last.ends_with(".lock"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn relevant(path: &str) -> bool {
        is_relevant(Path::new("/work/repo"), Path::new(path))
    }

    #[test]
    fn working_tree_files_count() {
        assert!(relevant("/work/repo/src/main.rs"));
        assert!(relevant("/work/repo/README.md"));
    }

    #[test]
    fn heavy_directories_are_ignored_at_any_depth() {
        for name in ["node_modules", "target", "dist", "build"] {
            assert!(!relevant(&format!("/work/repo/{name}/a.js")));
            assert!(!relevant(&format!("/work/repo/app/{name}/deep/a.js")));
        }
    }

    #[test]
    fn git_signals_are_kept() {
        assert!(relevant("/work/repo/.git/HEAD"));
        assert!(relevant("/work/repo/.git/index"));
        assert!(relevant("/work/repo/.git/packed-refs"));
        assert!(relevant("/work/repo/.git/refs/heads/main"));
        assert!(relevant("/work/repo/.git/refs/tags/v1"));
    }

    #[test]
    fn git_internals_and_lock_files_are_ignored() {
        assert!(!relevant("/work/repo/.git/objects/ab/cdef"));
        assert!(!relevant("/work/repo/.git/lfs/tmp/x"));
        assert!(!relevant("/work/repo/.git/index.lock"));
        assert!(!relevant("/work/repo/.git/refs/heads/main.lock"));
        assert!(!relevant("/work/repo/.git/FETCH_HEAD"));
        assert!(!relevant("/work/repo/.git"));
    }

    #[test]
    fn paths_outside_the_root_and_the_root_itself_are_ignored() {
        assert!(!relevant("/work/other/src/a.rs"));
        assert!(!relevant("/work/repo"));
    }
}
