use crate::git::{buffered, last_error, valid_root};
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Semaphore;

/// What a destination folder currently has checked out (no network access).
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LocalStatus {
    path: String,
    exists: bool,
    repo: bool,
    branch: Option<String>,
    tag: Option<String>,
    sha: String,
    upstream: Option<String>,
    ahead: u32,
    behind: u32,
    dirty: u32,
    error: Option<String>,
}

async fn status_of(path: String) -> LocalStatus {
    let dir = Path::new(&path);
    let mut st = LocalStatus { exists: dir.exists(), repo: dir.join(".git").exists(), path: path.clone(), ..Default::default() };
    if !st.repo {
        return st;
    }
    if let Err(error) = valid_root(&path) {
        st.error = Some(error);
        return st;
    }
    let out = match buffered(&["-C", &path, "status", "--porcelain=v2", "--branch"], &format!("Status: {path}"), &[0]).await {
        Ok(o) if o.code == Some(0) => o,
        Ok(o) => {
            st.error = Some(last_error(&String::from_utf8_lossy(&o.stderr)));
            return st;
        }
        Err(e) => {
            st.error = Some(format!("Could not run git: {e}"));
            return st;
        }
    };
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if let Some(v) = line.strip_prefix("# branch.oid ") {
            st.sha = v.chars().take(8).collect();
        } else if let Some(v) = line.strip_prefix("# branch.head ") {
            if v != "(detached)" {
                st.branch = Some(v.to_string());
            }
        } else if let Some(v) = line.strip_prefix("# branch.upstream ") {
            st.upstream = Some(v.to_string());
        } else if let Some(v) = line.strip_prefix("# branch.ab ") {
            let mut it = v.split(' ');
            st.ahead = it.next().and_then(|a| a.trim_start_matches('+').parse().ok()).unwrap_or(0);
            st.behind = it.next().and_then(|b| b.trim_start_matches('-').parse().ok()).unwrap_or(0);
        } else if !line.is_empty() && !line.starts_with('#') {
            st.dirty += 1;
        }
    }
    if st.branch.is_none() {
        let tag = buffered(&["-C", &path, "describe", "--tags", "--exact-match", "HEAD"], &format!("Tag probe: {path}"), &[0, 128]).await;
        if let Ok(o) = tag {
            if o.code == Some(0) {
                st.tag = Some(String::from_utf8_lossy(&o.stdout).trim().to_string());
            }
        }
    }
    st
}

#[tauri::command]
pub async fn local_status(paths: Vec<String>) -> Vec<LocalStatus> {
    let sem = Arc::new(Semaphore::new(8));
    let handles: Vec<_> = paths
        .into_iter()
        .map(|p| {
            let sem = sem.clone();
            tauri::async_runtime::spawn(async move {
                let _permit = sem.acquire_owned().await;
                status_of(p).await
            })
        })
        .collect();
    let mut out = Vec::with_capacity(handles.len());
    for h in handles {
        if let Ok(s) = h.await {
            out.push(s);
        }
    }
    out
}
