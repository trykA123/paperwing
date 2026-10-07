use super::{Job, Problem};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::OnceCell;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum EolMode {
    Verbatim,
    AutoCrlf,
}

type CachedConfiguration = Arc<OnceCell<Option<EolMode>>>;

#[derive(Default)]
pub(super) struct Eligibility {
    pub(super) storage: Option<PathBuf>,
    roots: Mutex<HashMap<PathBuf, CachedConfiguration>>,
}

impl Eligibility {
    pub(super) fn new(storage: PathBuf) -> Self {
        Self {
            storage: Some(storage),
            ..Self::default()
        }
    }

    pub(super) fn reset(&self) {
        self.roots().clear();
    }

    fn roots(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, CachedConfiguration>> {
        self.roots
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(super) async fn configuration(
        &self,
        root: &Path,
        job: &Job,
    ) -> Result<Option<EolMode>, Problem> {
        let cached = self.roots().entry(root.to_path_buf()).or_default().clone();
        cached
            .get_or_try_init(|| async {
                match self.inspect(root, job).await {
                    Ok(eligible) => Ok(eligible),
                    Err(problem) if problem.kind == "cancelled" => Err(problem),
                    Err(_) => Ok(None),
                }
            })
            .await
            .copied()
    }

    async fn inspect(&self, root: &Path, job: &Job) -> Result<Option<EolMode>, Problem> {
        let Some(storage) = &self.storage else {
            return Ok(None);
        };
        let (source, storage) = (root.to_path_buf(), storage.clone());
        let probe = tokio::task::spawn_blocking(move || probe(&source, &storage))
            .await
            .map_err(|_| Problem::new("unavailable", "Attribute eligibility task failed"))?;
        let Some((existing, directory, storage)) = probe else {
            return Ok(None);
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
        if result.code != Some(0) || !result.stdout.is_empty() {
            return Ok(None);
        }
        read_eol_mode(&existing, job).await
    }
}

async fn read_eol_mode(storage: &Path, job: &Job) -> Result<Option<EolMode>, Problem> {
    let config = job
        .run(
            storage,
            &["config", "--type=bool-or-str", "--get", "core.autocrlf"],
            &[0, 1],
        )
        .await?;
    match (config.code, super::decode(&config.stdout)?.trim()) {
        (Some(1), _) | (Some(0), "false") => Ok(Some(EolMode::Verbatim)),
        (Some(0), "true" | "input") => Ok(Some(EolMode::AutoCrlf)),
        _ => Ok(None),
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
