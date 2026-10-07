use super::{Job, Problem};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell};

#[derive(Default)]
pub(super) struct Eligibility {
    pub(super) storage: Option<PathBuf>,
    roots: Mutex<HashMap<PathBuf, Arc<OnceCell<bool>>>>,
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
        let cached = self
            .roots
            .lock()
            .await
            .entry(root.to_path_buf())
            .or_default()
            .clone();
        cached
            .get_or_try_init(|| async {
                match self.inspect(root, job).await {
                    Ok(eligible) => Ok(eligible),
                    Err(problem) if problem.kind == "cancelled" => Err(problem),
                    Err(_) => Ok(false),
                }
            })
            .await
            .copied()
    }

    async fn inspect(&self, root: &Path, job: &Job) -> Result<bool, Problem> {
        let Some(storage) = &self.storage else {
            return Ok(false);
        };
        let (source, storage) = (root.to_path_buf(), storage.clone());
        let probe = tokio::task::spawn_blocking(move || probe(&source, &storage))
            .await
            .map_err(|_| Problem::new("unavailable", "Attribute eligibility task failed"))?;
        let Some((existing, directory, storage)) = probe else {
            return Ok(false);
        };
        let mut args = vec!["-c".to_string(), "core.attributesFile=".to_string()];
        if let Some(directory) = directory {
            args.extend([
                "--git-dir".into(),
                path(&directory)?,
                "--work-tree".into(),
                path(&storage)?,
            ]);
        }
        args.extend([
            "check-attr".into(),
            "-a".into(),
            "--".into(),
            path(&storage.join("left"))?,
            path(&storage.join("right"))?,
        ]);
        let args: Vec<_> = args.iter().map(String::as_str).collect();
        let result = job.run(&existing, &args, &[0, 128]).await?;
        Ok(result.code == Some(0) && result.stdout.is_empty())
    }
}

fn path(path: &Path) -> Result<String, Problem> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| Problem::new("unsafePath", "Unsupported attribute probe path"))
}

fn probe(source: &Path, storage: &Path) -> Option<(PathBuf, Option<PathBuf>, PathBuf)> {
    for ancestor in storage.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return None,
        }
        if ancestor.join(".gitattributes").exists() {
            return None;
        }
    }
    let existing = storage
        .ancestors()
        .find(|ancestor| ancestor.is_dir())?
        .to_path_buf();
    let repository = existing
        .ancestors()
        .any(|ancestor| ancestor.join(".git").exists());
    let directory = if repository {
        None
    } else {
        Some(
            crate::git::batch_repository::resolve(source)
                .ok()?
                .directory,
        )
    };
    Some((existing, directory, storage.to_path_buf()))
}
