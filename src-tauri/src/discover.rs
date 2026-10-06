use serde::Serialize;
use std::collections::VecDeque;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const SKIPPED: [&str; 7] = [
    "node_modules",
    "target",
    "dist",
    "build",
    "venv",
    "vendor",
    "__pycache__",
];
const TEXT_LIMIT: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub depth: u32,
    pub directories: u32,
    pub repositories: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            depth: 4,
            directories: 10_000,
            repositories: 500,
        }
    }
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RepoKind {
    Normal,
    Worktree,
    Bare,
    Submodule,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FoundRepo {
    pub path: String,
    pub name: String,
    pub kind: RepoKind,
    pub branch: Option<String>,
    pub detached: bool,
    pub parent: Option<String>,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Cap {
    Directories,
    Repositories,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub repositories: u32,
    pub directories: u32,
    pub unreadable: u32,
    pub links_skipped: u32,
    pub capped: Option<Cap>,
    pub cancelled: bool,
}

pub enum Event {
    Repo(FoundRepo),
    Visited,
}

struct Head {
    branch: Option<String>,
    detached: bool,
}

struct Dir {
    path: PathBuf,
    depth: u32,
}

pub fn scan(
    root: &Path,
    limits: Limits,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Event),
) -> Summary {
    let mut run = Run {
        limits,
        summary: Summary::default(),
        cancel,
    };
    let mut queue = VecDeque::from([Dir {
        path: root.to_path_buf(),
        depth: 0,
    }]);
    while let Some(dir) = queue.pop_front() {
        if !run.admit() {
            break;
        }
        emit(Event::Visited);
        if !run.visit(&dir, &mut queue, emit) {
            break;
        }
    }
    run.summary
}

struct Run<'a> {
    limits: Limits,
    summary: Summary,
    cancel: &'a AtomicBool,
}

impl Run<'_> {
    fn admit(&mut self) -> bool {
        if self.cancel.load(Ordering::Relaxed) {
            self.summary.cancelled = true;
            return false;
        }
        if self.summary.directories >= self.limits.directories {
            self.summary.capped = Some(Cap::Directories);
            return false;
        }
        self.summary.directories += 1;
        true
    }

    fn visit(&mut self, dir: &Dir, queue: &mut VecDeque<Dir>, emit: &mut dyn FnMut(Event)) -> bool {
        if let Some((kind, head)) = classify(&dir.path) {
            if !self.report(&dir.path, kind, head, None, emit) {
                return false;
            }
            return self.report_submodules(&dir.path, emit);
        }
        if dir.depth < self.limits.depth {
            self.enqueue_children(dir, queue);
        }
        true
    }

    fn report(
        &mut self,
        path: &Path,
        kind: RepoKind,
        head: Head,
        parent: Option<&Path>,
        emit: &mut dyn FnMut(Event),
    ) -> bool {
        if self.summary.repositories >= self.limits.repositories {
            self.summary.capped = Some(Cap::Repositories);
            return false;
        }
        let Some(text) = path.to_str() else {
            self.summary.unreadable += 1;
            return true;
        };
        self.summary.repositories += 1;
        let name = path.file_name().map_or_else(
            || text.to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let parent = parent.and_then(Path::to_str).map(str::to_string);
        emit(Event::Repo(FoundRepo {
            path: text.to_string(),
            name,
            kind,
            branch: head.branch,
            detached: head.detached,
            parent,
        }));
        true
    }

    fn report_submodules(&mut self, repo: &Path, emit: &mut dyn FnMut(Event)) -> bool {
        for relative in submodule_paths(repo) {
            let Some(path) = safe_child(repo, &relative) else {
                continue;
            };
            let Some((_, head)) = classify(&path) else {
                continue;
            };
            if !self.report(&path, RepoKind::Submodule, head, Some(repo), emit) {
                return false;
            }
        }
        true
    }

    fn enqueue_children(&mut self, dir: &Dir, queue: &mut VecDeque<Dir>) {
        let Ok(entries) = fs::read_dir(&dir.path) else {
            self.summary.unreadable += 1;
            return;
        };
        let mut children = Vec::new();
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if is_link(&metadata) {
                self.summary.links_skipped += 1;
            } else if metadata.is_dir() && !skipped(&entry.file_name().to_string_lossy()) {
                children.push(entry.path());
            }
        }
        children.sort();
        queue.extend(children.into_iter().map(|path| Dir {
            path,
            depth: dir.depth + 1,
        }));
    }
}

fn skipped(name: &str) -> bool {
    name.starts_with('.')
        || SKIPPED.iter().any(|entry| {
            if cfg!(windows) {
                entry.eq_ignore_ascii_case(name)
            } else {
                *entry == name
            }
        })
}

fn is_link(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}

fn read_text(path: &Path) -> Option<String> {
    let mut text = String::new();
    fs::File::open(path)
        .ok()?
        .take(TEXT_LIMIT)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

fn classify(dir: &Path) -> Option<(RepoKind, Head)> {
    let marker = dir.join(".git");
    match fs::symlink_metadata(&marker) {
        Ok(metadata) if metadata.is_dir() => {
            return Some((RepoKind::Normal, read_head(&marker.join("HEAD"))))
        }
        Ok(metadata) if metadata.is_file() => return Some(linked_repository(dir, &marker)),
        Ok(_) => return None,
        Err(_) => {}
    }
    is_bare(dir).then(|| (RepoKind::Bare, read_head(&dir.join("HEAD"))))
}

fn is_bare(dir: &Path) -> bool {
    let kind =
        |name: &str| fs::symlink_metadata(dir.join(name)).map(|metadata| metadata.file_type());
    kind("HEAD").is_ok_and(|kind| kind.is_file())
        && kind("objects").is_ok_and(|kind| kind.is_dir())
        && kind("refs").is_ok_and(|kind| kind.is_dir())
}

fn linked_repository(dir: &Path, marker: &Path) -> (RepoKind, Head) {
    let target = read_text(marker)
        .and_then(|text| {
            text.lines().find_map(|line| {
                line.strip_prefix("gitdir:")
                    .map(|rest| PathBuf::from(rest.trim()))
            })
        })
        .map(|target| {
            if target.is_absolute() {
                target
            } else {
                dir.join(target)
            }
        });
    let Some(target) = target else {
        return (
            RepoKind::Worktree,
            Head {
                branch: None,
                detached: false,
            },
        );
    };
    let names: Vec<_> = target
        .components()
        .filter_map(|part| part.as_os_str().to_str())
        .collect();
    let nearest = names
        .iter()
        .rev()
        .find(|name| matches!(**name, "worktrees" | "modules"));
    let kind = if nearest == Some(&"modules") {
        RepoKind::Submodule
    } else {
        RepoKind::Worktree
    };
    (kind, read_head(&target.join("HEAD")))
}

fn read_head(path: &Path) -> Head {
    let text = read_text(path).unwrap_or_default();
    match text.trim().strip_prefix("ref:").map(str::trim) {
        Some(reference) => Head {
            branch: Some(
                reference
                    .strip_prefix("refs/heads/")
                    .unwrap_or(reference)
                    .to_string(),
            ),
            detached: false,
        },
        None => Head {
            branch: None,
            detached: !text.trim().is_empty(),
        },
    }
}

fn submodule_paths(repo: &Path) -> Vec<String> {
    let Some(text) = read_text(&repo.join(".gitmodules")) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("path"))
        .filter_map(|rest| rest.trim_start().strip_prefix('='))
        .map(|value| value.trim().to_string())
        .collect()
}

fn safe_child(root: &Path, relative: &str) -> Option<PathBuf> {
    let mut path = root.to_path_buf();
    for part in Path::new(relative).components() {
        let Component::Normal(part) = part else {
            return None;
        };
        path.push(part);
        let metadata = fs::symlink_metadata(&path).ok()?;
        if is_link(&metadata) || !metadata.is_dir() {
            return None;
        }
    }
    (path != root).then_some(path)
}

#[cfg(test)]
#[path = "discover_tests.rs"]
mod tests;
