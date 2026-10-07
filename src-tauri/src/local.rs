use crate::git::{buffered, valid_root};
use serde::Serialize;
use std::path::Path;

/// What a destination folder currently has checked out (no network access).
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LocalStatus {
    path: String,
    exists: bool,
    repo: bool,
    branch: Option<String>,
    branch_label: Option<String>,
    tag: Option<String>,
    tag_label: Option<String>,
    sha: String,
    upstream: Option<String>,
    upstream_label: Option<String>,
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
            st.error = Some(o.last_error());
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
                st.branch_label = Some(out.safe(v));
            }
        } else if let Some(v) = line.strip_prefix("# branch.upstream ") {
            st.upstream = Some(v.to_string());
            st.upstream_label = Some(out.safe(v));
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
                let name = String::from_utf8_lossy(&o.stdout);
                st.tag = Some(name.trim().to_string());
                st.tag_label = Some(o.safe(name.trim()));
            }
        }
    }
    st
}

#[tauri::command]
pub async fn local_status(paths: Vec<String>) -> Vec<LocalStatus> {
    let names = paths.clone();
    crate::ordered::map_bounded(paths, 8, status_of, move |index| {
        unavailable_status(names[index].clone())
    })
    .await
}

fn unavailable_status(path: String) -> LocalStatus {
    let dir = Path::new(&path);
    let exists = dir.exists();
    let repo = dir.join(".git").exists();
    LocalStatus {
        path,
        exists,
        repo,
        error: Some("Status check did not finish".into()),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_status_keeps_filesystem_presence() {
        let fixture = crate::platform::Fixture::new("local-fallback-status");
        let checkout = fixture.0.join("checkout");
        std::fs::create_dir_all(checkout.join(".git")).unwrap();

        let status = unavailable_status(checkout.to_string_lossy().into_owned());

        assert!(status.exists);
        assert!(status.repo);
        assert_eq!(status.error.as_deref(), Some("Status check did not finish"));
    }
}

#[cfg(all(test, unix))]
mod slow_tests;
