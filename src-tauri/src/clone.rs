#[cfg(not(target_os = "linux"))]
use crate::git::{buffered, valid_root};
use crate::git::{execute, safe, valid_path, valid_ref, valid_url, OutputPolicy, Request};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
#[cfg(not(target_os = "linux"))]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_os = "linux")]
mod linux;
use tauri::{AppHandle, Emitter};
use tokio::sync::Semaphore;

static ACTIVE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
pub fn busy() -> bool { ACTIVE.load(std::sync::atomic::Ordering::SeqCst) > 0 }
struct Lease;
impl Lease {
    fn acquire() -> Result<Self, String> {
        let _gate = crate::git::filesystem_gate().try_read().map_err(|_| "A recoverable write is in progress; retry the Git action")?;
        ACTIVE.fetch_add(1, std::sync::atomic::Ordering::SeqCst); Ok(Self)
    }
}
impl Drop for Lease { fn drop(&mut self) { ACTIVE.fetch_sub(1, std::sync::atomic::Ordering::SeqCst); } }

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    id: String,
    url: String,
    dest: String,
    ref_type: String,
    ref_name: String,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Opts {
    parallel: usize,
    shallow: bool,
    on_existing: String,
}

#[derive(Serialize, Clone)]
struct Progress<'a> {
    id: &'a str,
    phase: &'a str,
    pct: f32,
    msg: String,
}

fn emit(app: &AppHandle, id: &str, phase: &str, pct: f32, msg: impl Into<String>) {
    emit_clean(app, id, phase, pct, safe(&msg.into()));
}

fn emit_clean(app: &AppHandle, id: &str, phase: &str, pct: f32, msg: impl Into<String>) {
    let _ = app.emit("clone-progress", Progress { id, phase, pct, msg: msg.into() });
}

fn validate(job: &Job) -> Result<(), String> {
    valid_url(&job.url)?;
    let n = job.ref_name.as_str();
    match job.ref_type.as_str() {
        "commit" => {
            if !(7..=40).contains(&n.len()) || !n.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err("A commit must be a 7-40 character hex SHA".into());
            }
        }
        "branch" | "tag" => {
            valid_ref(n)?;
        }
        _ => return Err("Unknown ref type".into()),
    }
    valid_path(&job.dest, false)?;
    let mut ancestor = PathBuf::new();
    for part in Path::new(&job.dest).components() {
        ancestor.push(part);
        if part.as_os_str().to_string_lossy().eq_ignore_ascii_case(".git")
            || (ancestor.join("HEAD").is_file() && ancestor.join("objects").is_dir()) {
            return Err("Clone destination is inside Git metadata".into());
        }
    }
    Ok(())
}

/// Overall percentage for a git progress line, or None if it isn't one.
fn progress_pct(line: &str) -> Option<f32> {
    let (base, span) = if line.contains("Receiving objects:") {
        (0.0, 80.0)
    } else if line.contains("Resolving deltas:") {
        (80.0, 15.0)
    } else if line.contains("Updating files:") {
        (95.0, 5.0)
    } else if line.contains("Counting objects:") || line.contains("Compressing objects:") {
        (0.0, 0.0)
    } else {
        return None;
    };
    let digits = line.split('%').next()?.rsplit(|c: char| !c.is_ascii_digit()).next()?;
    let p: f32 = digits.parse().ok()?;
    Some(base + span * p / 100.0)
}

async fn run(app: &AppHandle, job: &Job, phase: &str, args: &[&str]) -> Result<(), String> {
    let (app, job, phase) = (app.clone(), job.clone(), phase.to_string());
    let last = Arc::new(std::sync::Mutex::new(-1));
    let context = format!("{}: {} ({})", job.id, phase, job.dest);
    let output = execute(Request { args, context: &context, timeout: Duration::from_secs(600), expected: &[0], policy: OutputPolicy::Text },
        Some(Arc::new(move |stream, text| {
            if stream != "stderr" { return; }
            if let Some(pct) = progress_pct(text) {
                let mut last = last.lock().unwrap();
                if pct as i32 != *last {
                    *last = pct as i32;
                    emit_clean(&app, &job.id, &phase, pct, text.trim_start_matches("remote: "));
                }
            }
        }))).await?;
    if output.code == Some(0) {
        Ok(())
    } else {
        Err(output.last_error())
    }
}

fn short(r: &str) -> &str {
    if r.len() == 40 { &r[..8] } else { r }
}

/// `host/path` in lower case, so SSH and HTTPS URLs of the same repo compare equal.
fn repo_key(url: &str) -> String {
    let mut u = url.trim().to_ascii_lowercase();
    for p in ["ssh://", "https://", "http://", "git://"] {
        if let Some(rest) = u.strip_prefix(p) {
            u = rest.to_string();
            break;
        }
    }
    if let Some((_, rest)) = u.split_once('@') {
        u = rest.to_string();
    }
    if let Some((host, rest)) = u.split_once(':') {
        let path = rest.trim_start_matches(|c: char| c.is_ascii_digit()).trim_start_matches('/');
        u = format!("{host}/{path}");
    }
    u.trim_end_matches('/').trim_end_matches(".git").to_string()
}

#[cfg(not(target_os = "linux"))]
async fn check_origin(dir: &str, expected: &str) -> Result<(), String> {
    valid_root(dir)?;
    let out = buffered(&["-C", dir, "remote", "get-url", "origin"], &format!("Origin check: {dir}"), &[0]).await?;
    if out.code != Some(0) {
        return Err("Folder is a git repository without an 'origin' remote".into());
    }
    let actual = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if repo_key(&actual) != repo_key(expected) {
        return Err(format!("Folder already holds a different repository ({})", out.safe(&actual)));
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
async fn open_existing(job: &Job) -> Result<(), String> {
    let dest = Path::new(&job.dest);
    if !dest.join(".git").exists() {
        return Err(if dest.exists() { "Folder exists but is not a git repository" } else { "Not cloned yet" }.into());
    }
    check_origin(&job.dest, &job.url).await
}

#[cfg(not(target_os = "linux"))]
async fn fetch_existing(app: &AppHandle, job: &Job) -> Result<(), String> {
    emit(app, &job.id, "fetching", 0.0, "Fetching origin");
    run(app, job, "fetching", &["-C", &job.dest, "fetch", "--progress", "--tags", "--prune", "origin"]).await
}

/// Fetch, check out the job's ref and fast-forward it (used for "Switch" and "Fetch & checkout").
#[cfg(not(target_os = "linux"))]
async fn switch_existing(app: &AppHandle, job: &Job) -> Result<(&'static str, String), String> {
    open_existing(job).await?;
    let d = job.dest.as_str();
    let name = job.ref_name.as_str();
    let target = if job.ref_type == "tag" { format!("refs/tags/{name}") } else { name.to_string() };
    fetch_existing(app, job).await?;
    emit(app, &job.id, "checkout", 100.0, format!("Checking out {}", short(name)));
    if let Err(e) = run(app, job, "checkout", &["-C", d, "checkout", &target]).await {
        if job.ref_type != "commit" || interrupted(&e) {
            return Err(e);
        }
        // Commit not reachable from fetched refs yet; ask for it directly.
        run(app, job, "fetching", &["-C", d, "fetch", "--progress", "origin", name]).await?;
        run(app, job, "checkout", &["-C", d, "checkout", "--detach", name]).await?;
    }
    if job.ref_type == "branch" {
        if let Err(error) = run(app, job, "checkout", &["-C", d, "merge", "--ff-only", "@{u}"]).await {
            if interrupted(&error) { return Err(error); }
            return Ok(("done", format!("On {name} (not fast-forwarded: local changes or diverged)")));
        }
        return Ok(("done", format!("On {name}")));
    }
    Ok(("done", format!("Detached at {}", short(name))))
}

fn interrupted(error: &str) -> bool {
    error == "Git command cancelled" || error == "Git command timed out"
}

#[cfg(not(target_os = "linux"))]
async fn pull_existing(app: &AppHandle, job: &Job) -> Result<(&'static str, String), String> {
    open_existing(job).await?;
    fetch_existing(app, job).await?;
    emit(app, &job.id, "checkout", 100.0, "Fast-forwarding");
    run(app, job, "checkout", &["-C", &job.dest, "merge", "--ff-only", "@{u}"])
        .await
        .map_err(|e| format!("Pull needs a fast-forward: {e}"))?;
    Ok(("done", "Up to date with upstream".into()))
}

#[cfg(not(target_os = "linux"))]
async fn run_job(app: &AppHandle, job: &Job, opts: &Opts) -> Result<(&'static str, String), String> {
    validate(job)?;
    let dest = PathBuf::from(&job.dest);
    let d = job.dest.as_str();
    let name = job.ref_name.as_str();
    let done = if job.ref_type == "branch" { format!("On {name}") } else { format!("Detached at {}", short(name)) };
    let existed = dest.exists();
    #[cfg(windows)]
    let mut _destination_guard = if !existed { Some(crate::file_guard::PinnedPath::ensure_directory(&dest)?) } else { None };

    if existed {
        match opts.on_existing.as_str() {
            "skip" => return Ok(("skipped", "Folder exists, skipped".into())),
            "fetch" => return switch_existing(app, job).await,
            "reclone" => {
                crate::git::BatchReader::close_root(&dest).await?;
                let _exclusive = crate::git::filesystem_gate().write().await;
                crate::git::BatchReader::close_root(&dest).await?;
                let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|t| t.as_nanos()).unwrap_or(0);
                let backup = PathBuf::from(format!("{d}.bak-{ts}-{}", std::process::id()));
                if backup.exists() { return Err("Reclone backup already exists; existing clone retained".into()); }
                #[cfg(windows)]
                {
                    use std::os::windows::ffi::OsStrExt;
                    let original_identity = crate::files::identity(&dest)?;
                    let mut parent = crate::file_guard::PinnedPath::existing_directory(dest.parent().ok_or("Missing clone parent")?)?;
                    let keeper_path = parent.path.join(format!(".skein-reclone-{ts}.lock"));
                    use std::os::windows::fs::OpenOptionsExt;
                    let keeper = std::fs::OpenOptions::new().write(true).create_new(true).share_mode(1)
                        .custom_flags(0x04000000).open(keeper_path).map_err(|error| error.to_string())?;
                    parent.permit_entry_update()?;
                    let source: Vec<u16> = dest.as_os_str().encode_wide().chain(Some(0)).collect();
                    let target: Vec<u16> = backup.as_os_str().encode_wide().chain(Some(0)).collect();
                    if unsafe { windows_sys::Win32::Storage::FileSystem::MoveFileExW(source.as_ptr(), target.as_ptr(), 8) } == 0 {
                        return Err(format!("Could not preserve existing clone: {}", std::io::Error::last_os_error()));
                    }
                    let backup_guard = crate::file_guard::PinnedPath::existing_directory(&backup)?;
                    if crate::files::identity(&backup)? != original_identity { return Err(format!("Repository changed during reclone; retained backup at {}", backup.display())); }
                    _destination_guard = Some(crate::file_guard::PinnedPath::ensure_directory(&dest)?);
                    drop(backup_guard); drop(parent); drop(keeper);
                }
                #[cfg(not(windows))]
                std::fs::rename(&dest, &backup).map_err(|e| format!("Could not preserve existing clone: {e}"))?;
                emit(app, &job.id, "resolving", 0.0, format!("Existing clone retained at {}", backup.display()));
            }
            _ => return Err("Unknown 'if folder exists' option".into()),
        }
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
    }
    emit(app, &job.id, "cloning", 0.0, "Connecting");
    if job.ref_type != "commit" {
        let mut args = vec!["clone", "--progress", "-b", name];
        if opts.shallow {
            args.extend(["--depth", "1"]);
        }
        args.extend(["--", job.url.as_str(), d]);
        run(app, job, "cloning", &args).await?;
    } else if opts.shallow {
        if name.len() != 40 {
            return Err("A shallow commit checkout needs the full 40-character SHA".into());
        }
        run(app, job, "cloning", &["init", "--quiet", d]).await?;
        run(app, job, "cloning", &["-C", d, "remote", "add", "origin", &job.url]).await?;
        run(app, job, "cloning", &["-C", d, "fetch", "--progress", "--depth", "1", "origin", name]).await?;
        emit(app, &job.id, "checkout", 100.0, format!("Checking out {}", short(name)));
        run(app, job, "checkout", &["-C", d, "checkout", "--detach", "FETCH_HEAD"]).await?;
    } else {
        run(app, job, "cloning", &["clone", "--progress", "--no-checkout", "--", &job.url, d]).await?;
        emit(app, &job.id, "checkout", 100.0, format!("Checking out {}", short(name)));
        run(app, job, "checkout", &["-C", d, "checkout", "--detach", name]).await?;
    }
    Ok(("done", done))
}

#[cfg(any(not(target_os = "linux"), test))]
fn validate_jobs(settings: &crate::settings::Settings, jobs: &[Job]) -> Result<(), String> {
    let mut destinations = std::collections::HashSet::new();
    for job in jobs {
        validate(job)?;
        crate::compare::registered_clone_destination(settings, &job.id, Path::new(&job.dest), &job.url)?;
        if !destinations.insert(crate::platform::destination_key(Path::new(&job.dest))?) {
            return Err("Clone jobs share a destination; run it once.".into());
        }
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
#[tauri::command]
pub async fn start_clone(app: AppHandle, jobs: Vec<Job>, opts: Opts, mode: Option<String>) -> Result<(), String> {
    let (validation_app, validation_jobs) = (app.clone(), jobs.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let settings = crate::settings::load_settings(validation_app)?;
        validate_jobs(&settings, &validation_jobs)
    }).await.map_err(|_| "Could not validate clone folders")??;
    let sem = Arc::new(Semaphore::new(opts.parallel.clamp(1, 16)));
    let mode = mode.unwrap_or_else(|| "clone".into());
    tauri::async_runtime::spawn(async move {
        let handles: Vec<_> = jobs
            .into_iter()
            .map(|job| {
                let (app, sem, opts, mode) = (app.clone(), sem.clone(), opts.clone(), mode.clone());
                tauri::async_runtime::spawn(async move {
                    let _permit = sem.acquire_owned().await;
                    if let Err(error) = crate::git::BatchReader::close_root(Path::new(&job.dest)).await { emit(&app, &job.id, "failed", 0.0, error); return; }
                    let _lease = match Lease::acquire() {
                        Ok(lease) => lease,
                        Err(error) => { emit(&app, &job.id, "failed", 0.0, error); return; }
                    };
                    let (registration_app, registration_job) = (app.clone(), job.clone());
                    let registered = tauri::async_runtime::spawn_blocking(move || {
                        crate::settings::load_settings(registration_app).and_then(|settings|
                            crate::compare::registered_clone_destination(&settings, &registration_job.id, Path::new(&registration_job.dest), &registration_job.url))
                    }).await.map_err(|_| "Could not validate clone destination".to_string()).and_then(|result| result);
                    if let Err(error) = registered { emit(&app, &job.id, "failed", 0.0, error); return; }
                    #[cfg(windows)]
                    let _root_guard = if Path::new(&job.dest).exists() && !(mode == "clone" && opts.on_existing == "reclone") {
                        match crate::file_guard::PinnedPath::existing_directory(Path::new(&job.dest)) {
                            Ok(guard) => Some(guard),
                            Err(error) => { emit(&app, &job.id, "failed", 0.0, error); return; }
                        }
                    } else { None };
                    emit(&app, &job.id, "resolving", 0.0, "Starting");
                    let res = match validate(&job).map(|_| mode.as_str()) {
                        Err(e) => Err(e),
                        Ok("fetch") => match open_existing(&job).await {
                            Ok(()) => fetch_existing(&app, &job).await.map(|_| ("done", "Fetched".to_string())),
                            Err(e) => Err(e),
                        },
                        Ok("pull") => pull_existing(&app, &job).await,
                        Ok("switch") => switch_existing(&app, &job).await,
                        Ok(_) => run_job(&app, &job, &opts).await,
                    };
                    match res {
                        Ok((phase, msg)) => emit(&app, &job.id, phase, 100.0, msg),
                        Err(e) => emit(&app, &job.id, "failed", 0.0, e),
                    }
                })
            })
            .collect();
        for h in handles {
            let _ = h.await;
        }
        let _ = app.emit("clone-finished", ());
    });
    Ok(())
}

#[cfg(target_os = "linux")]
#[tauri::command]
pub async fn start_clone(
    app: AppHandle,
    jobs: Vec<Job>,
    opts: Opts,
    mode: Option<String>,
) -> Result<(), String> {
    let mode = mode.unwrap_or_else(|| "clone".into());
    if !["clone", "fetch", "pull", "switch"].contains(&mode.as_str()) {
        return Err("Unknown clone mode".into());
    }
    if !["skip", "fetch", "reclone"].contains(&opts.on_existing.as_str()) {
        return Err("Unknown 'if folder exists' option".into());
    }
    for job in &jobs { crate::git::BatchReader::close_root(Path::new(&job.dest)).await?; }
    let lease = Lease::acquire()?;
    let worker_app = app.clone();
    let admission_mode = mode.clone();
    let (lease, admissions) = tauri::async_runtime::spawn_blocking(move || {
        let settings = crate::settings::load_settings(worker_app)?;
        let admissions = linux::admit_jobs(&settings, jobs, &admission_mode);
        Ok::<_, String>((lease, admissions))
    })
    .await
    .map_err(|_| "Could not validate Linux clone folders")??;
    let sem = Arc::new(Semaphore::new(opts.parallel.clamp(1, 16)));
    tauri::async_runtime::spawn(async move {
        let _lease = lease;
        let mut handles = Vec::new();
        for (job, admission) in admissions {
            let admission = match admission {
                Ok(admission) => admission,
                Err(error) => {
                    emit(&app, &job.id, "failed", 0.0, error);
                    continue;
                }
            };
            let (app, opts, mode, sem) = (app.clone(), opts.clone(), mode.clone(), sem.clone());
            handles.push(tauri::async_runtime::spawn(async move {
                let _permit = sem.acquire_owned().await;
                let job = admission.job.clone();
                emit(&app, &job.id, "resolving", 0.0, "Starting");
                let worker_app = app.clone();
                let result = tauri::async_runtime::spawn_blocking(move || {
                    tauri::async_runtime::block_on(linux::run_job(
                        &worker_app,
                        admission,
                        &opts,
                        &mode,
                    ))
                })
                .await
                .map_err(|_| "Linux clone worker could not finish".to_string())
                .and_then(|result| result);
                match result {
                    Ok((phase, message)) => emit(&app, &job.id, phase, 100.0, message),
                    Err(error) => emit(&app, &job.id, "failed", 0.0, error),
                }
            }));
        }
        for handle in handles {
            let _ = handle.await;
        }
        let _ = app.emit("clone-finished", ());
    });
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn clone_batches_refuse_duplicate_destinations_but_keep_case_distinct_roots() {
        let fixture = crate::platform::Fixture::new("clone-dedup");
        for name in ["Folder", "folder"] { std::fs::create_dir(fixture.0.join(name)).unwrap(); }
        let item = |id: &str, name: &str| serde_json::json!({"id":id, "name":name, "url":"https://example.test/repo", "repoId":"source:repo", "org":"o", "ref":{"type":"branch", "name":"main"}});
        let settings = crate::settings::Settings { sources: vec![], workspace: serde_json::json!({
            "root":fixture.0, "layout":"flat", "sets":[{"id":"set", "name":"Set", "items":[item("one", "Folder"), item("two", "Folder"), item("three", "folder"), item("four", "Missing"), item("five", "Missing")]}]
        }) };
        let job = |id: &str, name: &str| Job { id:id.into(), dest:fixture.0.join(name).to_str().unwrap().into(), url:"https://example.test/repo".into(), ref_type:"branch".into(), ref_name:"main".into() };
        assert!(validate_jobs(&settings, &[job("one", "Folder"), job("three", "folder")]).is_ok());
        assert!(validate_jobs(&settings, &[job("one", "Folder"), job("two", "Folder/")]).unwrap_err().contains("share a destination"));
        assert!(validate_jobs(&settings, &[job("four", "Missing"), job("five", "Missing")]).unwrap_err().contains("share a destination"));
        assert!(validate_jobs(&settings, &[job("one", "folder")]).is_err());
        assert!(validate_jobs(&settings, &[job("one", ".git/hidden")]).is_err());
        assert_eq!(std::fs::read_dir(&fixture.0).unwrap().count(), 3);
    }
}
