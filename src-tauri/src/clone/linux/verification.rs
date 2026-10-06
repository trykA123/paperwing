use super::{repo_key, Admission, Directory, Root};

pub(super) async fn origin_matches(directory: &Directory, expected: &str) -> Result<bool, String> {
    let path = directory.fd_path();
    let origin = crate::git::buffered(
        &["-C", &path, "remote", "get-url", "origin"],
        "Clone origin check",
        &[0],
    )
    .await?;
    if origin.code != Some(0) {
        return Ok(false);
    }
    let actual = repo_key(&String::from_utf8_lossy(&origin.stdout));
    if actual == repo_key(expected) {
        return Ok(true);
    }
    let resolved = crate::git::buffered(
        &["-C", &path, "ls-remote", "--get-url", "--", expected],
        "Resolve saved clone origin",
        &[0],
    )
    .await?;
    Ok(resolved.code == Some(0) && actual == repo_key(&String::from_utf8_lossy(&resolved.stdout)))
}

pub(super) async fn verify_clone(admission: &Admission, stage: &Directory) -> Result<Root, String> {
    match verify_repository(admission, stage).await {
        Ok(repository) => Ok(repository),
        Err(error) => match stage.remove_created() {
            Ok(()) => Err(error),
            Err(cleanup) => Err(format!(
                "{error}. Could not clean staging at {}: {cleanup}",
                stage.path.display()
            )),
        },
    }
}

async fn verify_repository(admission: &Admission, stage: &Directory) -> Result<Root, String> {
    admission.check()?;
    let target = match admission.job.ref_type.as_str() {
        "branch" => format!("refs/remotes/origin/{}", admission.job.ref_name),
        "tag" => format!("refs/tags/{}", admission.job.ref_name),
        _ => admission.job.ref_name.clone(),
    };
    let repository = Root::open(
        &stage.path,
        &[
            stage.path.join(".git/config"),
            stage.path.join(".git/HEAD"),
            stage.path.join(".git/packed-refs"),
            stage.path.join(".git").join(&target),
        ],
    )
    .map_err(|e| e.to_string())?;
    repository.probe_write().map_err(|e| e.to_string())?;
    let path = stage.fd_path();
    if !origin_matches(stage, &admission.job.url).await? {
        return Err("Cloned origin does not match its saved item".into());
    }
    let target = format!("{target}^{{commit}}");
    let heads = crate::git::buffered(
        &["-C", &path, "rev-parse", "--verify", "HEAD"],
        "Verify clone HEAD",
        &[0],
    )
    .await?;
    let expected = crate::git::buffered(
        &["-C", &path, "rev-parse", "--verify", &target],
        "Verify clone ref",
        &[0],
    )
    .await?;
    repository.revalidate().map_err(|e| e.to_string())?;
    stage.revalidate().map_err(|e| e.to_string())?;
    admission.check()?;
    if heads.code != Some(0) || expected.code != Some(0) || heads.stdout != expected.stdout {
        return Err("Cloned checkout does not match its saved ref".into());
    }
    Ok(repository)
}
