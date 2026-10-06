use super::Admission;
use crate::clone::{emit, run, short, Opts};
use crate::linux_guard::folders::Directory;
use tauri::AppHandle;

async fn checked_run(
    app: &AppHandle,
    admission: &Admission,
    directory: &Directory,
    args: &[&str],
) -> Result<(), String> {
    admission.rebind(app)?;
    directory.revalidate().map_err(|e| e.to_string())?;
    let command = if args.first() == Some(&"-C") {
        args.get(2)
    } else {
        args.first()
    };
    let (phase, message) = match command.copied() {
        Some("fetch") => ("fetching", "Fetching origin"),
        Some("checkout" | "merge") => ("checkout", "Updating checkout"),
        _ => ("cloning", "Cloning"),
    };
    emit(app, &admission.job.id, phase, 0.0, message);
    run(app, &admission.job, phase, args).await?;
    directory.revalidate().map_err(|e| e.to_string())?;
    admission.check()
}

pub(super) async fn clone_into(
    app: &AppHandle,
    admission: &Admission,
    stage: &Directory,
    opts: &Opts,
) -> Result<(), String> {
    let job = &admission.job;
    let path = stage.fd_path();
    let name = job.ref_name.as_str();
    emit(app, &job.id, "cloning", 0.0, "Connecting");
    if job.ref_type != "commit" {
        let mut args = vec!["clone", "--progress", "-b", name];
        if opts.shallow {
            args.extend(["--depth", "1"]);
        }
        args.extend(["--", &job.url, &path]);
        return checked_run(app, admission, stage, &args).await;
    }
    if opts.shallow {
        checked_run(app, admission, stage, &["init", "--quiet", &path]).await?;
        checked_run(
            app,
            admission,
            stage,
            &["-C", &path, "remote", "add", "origin", &job.url],
        )
        .await?;
        checked_run(
            app,
            admission,
            stage,
            &[
                "-C",
                &path,
                "fetch",
                "--progress",
                "--depth",
                "1",
                "origin",
                name,
            ],
        )
        .await?;
        checked_run(
            app,
            admission,
            stage,
            &["-C", &path, "checkout", "--detach", "FETCH_HEAD"],
        )
        .await
    } else {
        checked_run(
            app,
            admission,
            stage,
            &[
                "clone",
                "--progress",
                "--no-checkout",
                "--",
                &job.url,
                &path,
            ],
        )
        .await?;
        checked_run(
            app,
            admission,
            stage,
            &["-C", &path, "checkout", "--detach", name],
        )
        .await
    }
}

pub(super) async fn update(
    app: &AppHandle,
    admission: &Admission,
    directory: &Directory,
    mode: &str,
) -> Result<(&'static str, String), String> {
    let path = directory.fd_path();
    checked_run(
        app,
        admission,
        directory,
        &[
            "-C",
            &path,
            "fetch",
            "--progress",
            "--tags",
            "--prune",
            "origin",
        ],
    )
    .await?;
    if mode == "fetch" {
        return Ok(("done", "Fetched".into()));
    }
    let job = &admission.job;
    if mode != "pull" {
        checkout_ref(app, admission, directory).await?;
    }
    if mode == "pull" || job.ref_type == "branch" {
        checked_run(
            app,
            admission,
            directory,
            &["-C", &path, "merge", "--ff-only", "@{u}"],
        )
        .await?;
    }
    Ok((
        "done",
        if mode == "pull" {
            "Up to date with upstream".into()
        } else if job.ref_type == "branch" {
            format!("On {}", job.ref_name)
        } else {
            format!("Detached at {}", short(&job.ref_name))
        },
    ))
}

async fn checkout_ref(
    app: &AppHandle,
    admission: &Admission,
    directory: &Directory,
) -> Result<(), String> {
    let path = directory.fd_path();
    let job = &admission.job;
    let target = if job.ref_type == "tag" {
        format!("refs/tags/{}", job.ref_name)
    } else {
        job.ref_name.clone()
    };
    if let Err(error) = checked_run(
        app,
        admission,
        directory,
        &["-C", &path, "checkout", &target],
    )
    .await
    {
        if job.ref_type != "commit" || super::super::interrupted(&error) {
            return Err(error);
        }
        checked_run(
            app,
            admission,
            directory,
            &["-C", &path, "fetch", "--progress", "origin", &job.ref_name],
        )
        .await?;
        checked_run(
            app,
            admission,
            directory,
            &["-C", &path, "checkout", "--detach", &job.ref_name],
        )
        .await?;
    }
    Ok(())
}
