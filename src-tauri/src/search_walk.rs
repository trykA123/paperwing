use ignore::{WalkBuilder, WalkState};
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub struct WalkRequest<'a> {
    pub root: &'a str,
    pub files: Vec<PathBuf>,
    pub cancel: &'a AtomicBool,
    pub exclude_ignored: bool,
}

pub fn visit(
    request: WalkRequest<'_>,
    receive: &(dyn Fn(PathBuf) + Send + Sync),
) -> Result<(), String> {
    let WalkRequest {
        root,
        files,
        cancel,
        exclude_ignored,
    } = request;
    let root =
        crate::platform::canonical_path(Path::new(root)).map_err(|error| error.to_string())?;
    let paths: HashSet<_> = files.into_iter().map(|path| root.join(path)).collect();
    let directories: HashSet<_> = paths
        .iter()
        .flat_map(|path| path.ancestors().skip(1).map(Path::to_path_buf))
        .collect();
    let admitted = paths.clone();
    let mut builder = WalkBuilder::new(&root);
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
            admitted.contains(path) || directories.contains(path)
        });
    let error = Mutex::new(None);
    builder.build_parallel().run(|| {
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
                        && paths.contains(entry.path()) =>
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
        &|path| {
            paths
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert(path);
        },
    )?;
    Ok(paths
        .into_inner()
        .map_err(|_| "Search walk is unavailable")?
        .into_iter()
        .collect())
}
