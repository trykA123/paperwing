use super::{emit, repo_key, short, validate, Job, Opts};
mod execution;
mod verification;
use crate::linux_guard::{folders::Directory, mutation::Parent, Root};
use crate::settings::Settings;
use execution::{clone_into, update};
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use verification::{origin_matches, verify_clone};

pub(super) struct Admission {
    pub(super) job: Job,
    workspace: Root,
    parent: Parent,
    directory: Directory,
    repository: Option<Root>,
}

pub(super) fn admit_jobs(
    settings: &Settings,
    jobs: Vec<Job>,
    mode: &str,
) -> Vec<(Job, Result<Admission, String>)> {
    let mut destinations = std::collections::HashSet::new();
    jobs.into_iter()
        .map(|job| {
            let result = crate::platform::destination_key(Path::new(&job.dest)).and_then(|key| {
                if destinations.contains(&key) {
                    return Err("Clone jobs share a destination; run it once.".into());
                }
                let admission = admit(settings, &job, mode)?;
                destinations.insert(key);
                Ok(admission)
            });
            (job, result)
        })
        .collect()
}

pub(super) fn admit(settings: &Settings, job: &Job, mode: &str) -> Result<Admission, String> {
    validate_binding(settings, job)?;
    let root = Path::new(
        settings.workspace["root"]
            .as_str()
            .ok_or("Missing workspace root")?,
    );
    let workspace = Root::open(root, &[]).map_err(|e| e.to_string())?;
    let relative = Path::new(&job.dest)
        .strip_prefix(root)
        .map_err(|_| "Clone destination is outside its workspace")?
        .to_str()
        .ok_or("Unsupported clone destination")?;
    let parent = workspace
        .parent(relative, mode == "clone")
        .map_err(|e| e.to_string())?;
    let directory = Directory::open(
        Path::new(&job.dest)
            .parent()
            .ok_or("Missing clone parent")?,
    )
    .map_err(|e| e.to_string())?;
    let repository = if Path::new(&job.dest).exists() {
        let path = Path::new(&job.dest);
        Some(Root::open(path, &[path.join(".git/config")]).map_err(|e| e.to_string())?)
    } else {
        None
    };
    Ok(Admission {
        job: job.clone(),
        workspace,
        parent,
        directory,
        repository,
    })
}

fn validate_binding(settings: &Settings, job: &Job) -> Result<(), String> {
    validate(job)?;
    crate::compare::registered_clone_destination(
        settings,
        &job.id,
        Path::new(&job.dest),
        &job.url,
    )?;
    let matches = settings.workspace["sets"]
        .as_array()
        .ok_or("No registered sets")?
        .iter()
        .flat_map(|set| set["items"].as_array().into_iter().flatten())
        .filter(|item| item["id"].as_str() == Some(&job.id))
        .collect::<Vec<_>>();
    if matches.len() != 1
        || matches[0]["url"].as_str() != Some(&job.url)
        || matches[0]["ref"]["type"].as_str() != Some(&job.ref_type)
        || matches[0]["ref"]["name"].as_str() != Some(&job.ref_name)
    {
        return Err("Clone origin or ref does not match its saved item".into());
    }
    Ok(())
}

impl Admission {
    pub(super) fn rebind(&self, app: &AppHandle) -> Result<(), String> {
        let settings = crate::settings::load_settings(app.clone())?;
        validate_binding(&settings, &self.job)?;
        if settings.workspace["root"].as_str()
            != self
                .workspace
                .value()
                .map_err(|e| e.to_string())?
                .path
                .to_str()
        {
            return Err("Registered clone workspace changed".into());
        }
        self.check()
    }

    pub(super) fn check(&self) -> Result<(), String> {
        self.workspace.probe_write().map_err(|e| e.to_string())?;
        self.parent.revalidate().map_err(|e| e.to_string())?;
        self.directory.revalidate().map_err(|e| e.to_string())?;
        if let Some(root) = &self.repository {
            root.probe_write().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub(super) async fn existing(&mut self) -> Result<Directory, String> {
        self.check()?;
        crate::git::valid_root(&self.job.dest)?;
        let path = Path::new(&self.job.dest);
        let root = Root::open(path, &[path.join(".git/config")]).map_err(|e| e.to_string())?;
        root.probe_write().map_err(|e| e.to_string())?;
        let directory = Directory::open(path).map_err(|e| e.to_string())?;
        let matches = origin_matches(&directory, &self.job.url).await?;
        root.revalidate().map_err(|e| e.to_string())?;
        self.check()?;
        if !matches {
            return Err("Folder origin changed or holds a different repository".into());
        }
        self.repository = Some(root);
        Ok(directory)
    }

    pub(super) fn preserve(&self, source: &Directory, candidate: &str) -> Result<PathBuf, String> {
        self.check()?;
        for index in 0..128 {
            let name = if index == 0 {
                candidate.to_string()
            } else {
                format!("{candidate}-{index}")
            };
            if !self.directory.missing(&name).map_err(|e| e.to_string())? {
                continue;
            }
            self.check()?;
            match source.move_to(&self.directory, &name) {
                Ok(path) => return Ok(path),
                Err(error) if error.code == Some(libc::EEXIST) => continue,
                Err(error) => {
                    return Err(format!(
                        "Could not preserve clone at {}: {error}",
                        self.directory.path.join(name).display()
                    ))
                }
            }
        }
        Err("Reclone preservation names are exhausted; existing clone retained".into())
    }
}

pub(super) async fn run_job(
    app: &AppHandle,
    mut admission: Admission,
    opts: &Opts,
    mode: &str,
) -> Result<(&'static str, String), String> {
    admission.rebind(app)?;
    let job = admission.job.clone();
    if opts.shallow && job.ref_type == "commit" && job.ref_name.len() != 40 {
        return Err("A shallow commit checkout needs the full 40-character SHA".into());
    }
    if mode != "clone" || Path::new(&job.dest).exists() && opts.on_existing == "fetch" {
        let directory = admission.existing().await?;
        return update(app, &admission, &directory, mode).await;
    }
    if Path::new(&job.dest).exists() {
        match opts.on_existing.as_str() {
            "skip" => return Ok(("skipped", "Folder exists, skipped".into())),
            "reclone" => preserve_existing(app, &mut admission).await?,
            _ => return Err("Unknown 'if folder exists' option".into()),
        }
    }
    admission.rebind(app)?;
    let name =
        crate::linux_guard::storage::unique_name(".skein-clone-").map_err(|e| e.to_string())?;
    let stage = admission
        .directory
        .create_new(&name)
        .map_err(|e| e.to_string())?;
    let result = clone_into(app, &admission, &stage, opts).await;
    result.map_err(|error| format!("{error}. Clone data retained at {}", stage.path.display()))?;
    let repository = verify_clone(&admission, &stage).await?;
    admission
        .rebind(app)
        .and_then(|()| publish(&admission, &stage, &repository))
        .map_err(|error| {
            format!(
                "{error}. Clone data retained at {} or {}",
                stage.path.display(),
                job.dest
            )
        })?;
    Ok((
        "done",
        if job.ref_type == "branch" {
            format!("On {}", job.ref_name)
        } else {
            format!("Detached at {}", short(&job.ref_name))
        },
    ))
}

async fn preserve_existing(app: &AppHandle, admission: &mut Admission) -> Result<(), String> {
    let directory = admission.existing().await?;
    let leaf = Path::new(&admission.job.dest)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Missing clone name")?;
    let name = crate::linux_guard::storage::unique_name(&format!("{leaf}.bak-"))
        .map_err(|e| e.to_string())?;
    admission.rebind(app)?;
    let preserved = admission.preserve(&directory, &name)?;
    emit(
        app,
        &admission.job.id,
        "resolving",
        0.0,
        format!("Existing clone retained at {}", preserved.display()),
    );
    admission.repository = None;
    Ok(())
}

pub(super) fn publish(
    admission: &Admission,
    stage: &Directory,
    repository: &Root,
) -> Result<(), String> {
    admission.check()?;
    repository.probe_write().map_err(|e| e.to_string())?;
    crate::git::valid_root(stage.path.to_str().ok_or("Unsupported staging path")?)?;
    let leaf = Path::new(&admission.job.dest)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Missing clone name")?;
    stage.move_to(&admission.directory, leaf).map_err(|e| {
        format!(
            "Could not publish clone: {e}. Data retained at {} or {}",
            stage.path.display(),
            admission.job.dest
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests;
