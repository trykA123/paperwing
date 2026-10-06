use super::{decode, Job, Problem};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

#[derive(Default)]
pub(super) struct Eligibility {
    pub(super) storage: Option<PathBuf>,
    roots: Mutex<HashMap<PathBuf, bool>>,
}

impl Eligibility {
    pub(super) fn new(storage: PathBuf) -> Self {
        Self {
            storage: Some(storage),
            ..Self::default()
        }
    }

    pub(super) async fn reset(&self) {
        self.roots.lock().await.clear();
    }

    pub(super) async fn allows(&self, root: &Path, job: &Job) -> Result<bool, Problem> {
        if let Some(eligible) = self.roots.lock().await.get(root).copied() {
            return Ok(eligible);
        }
        let eligible = match self.inspect(root, job).await {
            Ok(eligible) => eligible,
            Err(problem) if problem.kind == "cancelled" => return Err(problem),
            Err(_) => false,
        };
        self.roots.lock().await.insert(root.to_path_buf(), eligible);
        Ok(eligible)
    }

    async fn inspect(&self, root: &Path, job: &Job) -> Result<bool, Problem> {
        let Some(storage) = &self.storage else {
            return Ok(false);
        };
        let Some(mut attributes) = repository_attributes(root, job).await? else {
            return Ok(false);
        };
        let Some(storage_paths) = storage_attributes(storage, job).await? else {
            return Ok(false);
        };
        attributes.extend(storage_paths);
        let (root, storage) = (root.to_path_buf(), storage.clone());
        tokio::task::spawn_blocking(move || filesystem_allows(&root, &storage, &attributes))
            .await
            .map_err(|_| Problem::new("unavailable", "Attribute eligibility task failed"))
    }
}

async fn repository_attributes(root: &Path, job: &Job) -> Result<Option<Vec<PathBuf>>, Problem> {
    let mut paths = Vec::new();
    for variable in ["GIT_ATTR_SYSTEM", "GIT_ATTR_GLOBAL"] {
        let result = job.run(root, &["var", variable], &[0, 1]).await?;
        if result.code != Some(0) {
            if variable == "GIT_ATTR_SYSTEM"
                && std::env::var_os("GIT_ATTR_NOSYSTEM").is_some_and(|value| value == "1")
            {
                continue;
            }
            return Ok(None);
        }
        paths.extend(decode(&result.stdout)?.lines().map(PathBuf::from));
    }
    if !configuration_allows(root, job).await? {
        return Ok(None);
    }
    let info = job
        .output(
            root,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                "info/attributes",
            ],
        )
        .await?;
    paths.push(PathBuf::from(decode(&info)?.trim()));
    let tracked = job
        .output(
            root,
            &[
                "ls-files",
                "--cached",
                "-z",
                "--",
                ".gitattributes",
                ":(glob)**/.gitattributes",
            ],
        )
        .await?;
    Ok(tracked.is_empty().then_some(paths))
}

async fn storage_attributes(storage: &Path, job: &Job) -> Result<Option<Vec<PathBuf>>, Problem> {
    let storage = storage.to_path_buf();
    let (existing, repository) = tokio::task::spawn_blocking(move || {
        let existing = storage
            .ancestors()
            .find(|ancestor| ancestor.is_dir())
            .map(Path::to_path_buf);
        let repository = storage
            .ancestors()
            .find(|ancestor| !absent(&ancestor.join(".git")) || ancestor.join("HEAD").is_file())
            .map(Path::to_path_buf);
        (existing, repository)
    })
    .await
    .map_err(|_| Problem::new("unavailable", "Storage attribute task failed"))?;
    let Some(existing) = existing else {
        return Ok(None);
    };
    if !configuration_allows(&existing, job).await? {
        return Ok(None);
    }
    let mut paths = Vec::new();
    if let Some(repository) = repository {
        let result = job
            .run(
                &repository,
                &[
                    "rev-parse",
                    "--path-format=absolute",
                    "--git-path",
                    "info/attributes",
                ],
                &[0, 128],
            )
            .await?;
        if result.code != Some(0) {
            return Ok(None);
        }
        paths.push(PathBuf::from(decode(&result.stdout)?.trim()));
    }
    Ok(Some(paths))
}

fn filesystem_allows(root: &Path, storage: &Path, attributes: &[PathBuf]) -> bool {
    attributes.iter().all(|path| absent(path))
        && storage
            .ancestors()
            .all(|ancestor| match std::fs::symlink_metadata(ancestor) {
                Ok(metadata) => metadata.is_dir() && !metadata.file_type().is_symlink(),
                Err(error) => error.kind() == std::io::ErrorKind::NotFound,
            })
        && !has_attributes(root)
        && storage
            .ancestors()
            .all(|ancestor| absent(&ancestor.join(".gitattributes")))
}

async fn configuration_allows(root: &Path, job: &Job) -> Result<bool, Problem> {
    let result = job
        .run(
            root,
            &[
                "config",
                "--get-regexp",
                "^core\\.(attributesfile|autocrlf)$",
            ],
            &[0, 1],
        )
        .await?;
    Ok(decode(&result.stdout)?.lines().all(|line| {
        let Some((key, value)) = line.split_once(' ') else {
            return false;
        };
        match key {
            "core.attributesfile" => value.trim().is_empty(),
            "core.autocrlf" => matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "false" | "no" | "off" | "0"
            ),
            _ => false,
        }
    }))
}

fn absent(path: &Path) -> bool {
    matches!(std::fs::symlink_metadata(path), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
}

fn has_attributes(root: &Path) -> bool {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        if !absent(&directory.join(".gitattributes")) {
            return true;
        }
        let Ok(entries) = std::fs::read_dir(directory) else {
            return true;
        };
        for entry in entries {
            let Ok(entry) = entry else {
                return true;
            };
            if entry.file_name() == ".git" {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                return true;
            };
            if kind.is_dir() {
                pending.push(entry.path());
            }
        }
    }
    false
}
