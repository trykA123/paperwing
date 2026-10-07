use ignore::{WalkBuilder, WalkState};
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct WalkRequest<'a> {
    pub root: &'a str,
    pub files: Vec<PathBuf>,
    pub cancel: &'a AtomicBool,
    pub exclude_ignored: bool,
}

pub fn visit<'a>(
    request: WalkRequest<'_>,
    create: impl Fn() -> Box<dyn FnMut(PathBuf) + Send + 'a> + Sync,
) -> Result<(), String> {
    let WalkRequest {
        root,
        files,
        cancel,
        exclude_ignored,
    } = request;
    let root =
        crate::platform::canonical_path(Path::new(root)).map_err(|error| error.to_string())?;
    let paths = Arc::new(
        files
            .into_iter()
            .map(|path| key(&root.join(path)))
            .collect::<HashSet<_>>(),
    );
    let builder = build_walker(&root, paths.clone(), exclude_ignored);
    let error = Mutex::new(None);
    builder.build_parallel().run(|| {
        let mut receive = create();
        let root = &root;
        let paths = &paths;
        let error = &error;
        Box::new(move |entry| {
            if cancel.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }
            match entry {
                Ok(entry)
                    if entry.file_type().is_some_and(|kind| kind.is_file())
                        && contains(paths, entry.path()) =>
                {
                    if let Ok(relative) = entry.path().strip_prefix(root) {
                        receive(relative.to_path_buf());
                    }
                }
                Ok(_) => {}
                Err(failure) => {
                    *error
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                        Some(failure.to_string());
                    return WalkState::Quit;
                }
            }
            WalkState::Continue
        })
    });
    match error
        .into_inner()
        .map_err(|_| "Search walk is unavailable")?
    {
        Some(error) => Err(crate::git::safe(&format!(
            "Could not walk search files: {error}"
        ))),
        None => Ok(()),
    }
}

fn build_walker(root: &Path, paths: Arc<HashSet<PathBuf>>, exclude_ignored: bool) -> WalkBuilder {
    let mut directories = HashSet::new();
    for path in paths.iter() {
        for parent in path.ancestors().skip(1) {
            if !directories.insert(parent.to_path_buf()) {
                break;
            }
        }
    }
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .ignore(false)
        .git_ignore(exclude_ignored)
        .git_global(exclude_ignored)
        .git_exclude(exclude_ignored)
        .parents(exclude_ignored)
        .follow_links(false)
        .threads(4)
        .filter_entry(move |entry| {
            let path = entry.path();
            contains(&paths, path) || contains(&directories, path)
        });
    builder
}

fn key(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

fn contains(paths: &HashSet<PathBuf>, path: &Path) -> bool {
    #[cfg(windows)]
    {
        paths.contains(&key(path))
    }
    #[cfg(not(windows))]
    {
        paths.contains(path)
    }
}

pub fn collect(
    root: &str,
    files: Vec<PathBuf>,
    cancel: &AtomicBool,
) -> Result<Vec<PathBuf>, String> {
    let paths = Mutex::new(BTreeSet::new());
    visit(
        WalkRequest {
            root,
            files,
            cancel,
            exclude_ignored: true,
        },
        || {
            Box::new(|path| {
                paths
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .insert(path);
            })
        },
    )?;
    Ok(paths
        .into_inner()
        .map_err(|_| "Search walk is unavailable")?
        .into_iter()
        .collect())
}
