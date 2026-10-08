use std::path::{Component, Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

const IGNORED_DIRECTORIES: [&str; 4] = ["node_modules", "target", "dist", "build"];
const GIT_FILES: [&str; 3] = ["HEAD", "index", "packed-refs"];

pub(super) struct Root {
    pub path: PathBuf,
    pub label: String,
    pub git_dirs: Vec<PathBuf>,
    pub dirs: Mutex<Vec<PathBuf>>,
    pub lost: AtomicBool,
}

impl Root {
    pub(super) fn new(label: &str, git_dirs: Vec<PathBuf>) -> Self {
        Self {
            path: PathBuf::from(label),
            label: label.to_string(),
            git_dirs,
            dirs: Mutex::default(),
            lost: AtomicBool::new(false),
        }
    }
}

enum Place<'a> {
    Tree(Vec<&'a str>),
    Git(Vec<&'a str>),
}

fn names(relative: &Path) -> Vec<&str> {
    relative
        .components()
        .filter_map(|part| match part {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect()
}

fn locate<'a>(root: &Root, path: &'a Path) -> Option<Place<'a>> {
    if let Ok(relative) = path.strip_prefix(&root.path) {
        return Some(match names(relative).as_slice() {
            [".git", rest @ ..] => Place::Git(rest.to_vec()),
            parts => Place::Tree(parts.to_vec()),
        });
    }
    root.git_dirs
        .iter()
        .find_map(|dir| path.strip_prefix(dir).ok())
        .map(|relative| Place::Git(names(relative)))
}

fn has_ignored(parts: &[&str]) -> bool {
    parts.iter().any(|part| IGNORED_DIRECTORIES.contains(part))
}

pub(super) fn is_relevant(root: &Root, path: &Path) -> bool {
    match locate(root, path) {
        Some(Place::Tree(parts)) => !parts.is_empty() && !has_ignored(&parts),
        Some(Place::Git(rest)) => is_git_signal(&rest),
        None => false,
    }
}

pub(super) fn is_watchable_dir(root: &Root, path: &Path) -> bool {
    match locate(root, path) {
        Some(Place::Tree(parts)) => !has_ignored(&parts),
        Some(Place::Git(rest)) => rest.first().is_none_or(|first| *first == "refs"),
        None => false,
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
        is_relevant(&Root::new("/work/repo", Vec::new()), Path::new(path))
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
    fn linked_git_directories_signal_the_working_root() {
        let root = Root::new(
            "/work/wt",
            vec!["/main/.git/worktrees/wt".into(), "/main/.git".into()],
        );
        assert!(is_relevant(
            &root,
            Path::new("/main/.git/worktrees/wt/HEAD")
        ));
        assert!(is_relevant(
            &root,
            Path::new("/main/.git/worktrees/wt/index")
        ));
        assert!(is_relevant(&root, Path::new("/main/.git/refs/heads/main")));
        assert!(!is_relevant(&root, Path::new("/main/.git/objects/ab/c")));
        assert!(!is_relevant(&root, Path::new("/work/wt/.git")));
    }

    #[test]
    fn only_wanted_directories_get_their_own_watch() {
        let root = Root::new("/work/repo", Vec::new());
        assert!(is_watchable_dir(&root, Path::new("/work/repo")));
        assert!(is_watchable_dir(&root, Path::new("/work/repo/src/deep")));
        assert!(is_watchable_dir(&root, Path::new("/work/repo/.git")));
        assert!(is_watchable_dir(
            &root,
            Path::new("/work/repo/.git/refs/heads")
        ));
        assert!(!is_watchable_dir(
            &root,
            Path::new("/work/repo/.git/objects")
        ));
        assert!(!is_watchable_dir(
            &root,
            Path::new("/work/repo/web/node_modules/x")
        ));
    }

    #[test]
    fn paths_outside_the_root_and_the_root_itself_are_ignored() {
        assert!(!relevant("/work/other/src/a.rs"));
        assert!(!relevant("/work/repo"));
    }
}
