use super::vocabulary;
use crate::settings::Settings;
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const REFUSAL: &str = "Diagnostics contained identifying text; nothing was written";

#[derive(Default)]
pub struct Denylist {
    tokens: HashSet<String>,
}

impl Denylist {
    pub fn from_settings(settings: &Settings, repo_paths: &[PathBuf]) -> Self {
        let mut denylist = Self::default();
        denylist.add_system_identity();
        if let Some(home) = home_path() {
            denylist.add_path(&home);
        }
        for source in &settings.sources {
            denylist.add(&source.id);
            denylist.add(&source.name);
            denylist.add(&source.host);
            for org in &source.orgs {
                denylist.add(org);
            }
            for url in &source.urls {
                denylist.add_remote(url);
            }
        }
        if let Some(sets) = settings
            .workspace
            .get("sets")
            .and_then(serde_json::Value::as_array)
        {
            for set in sets {
                if let Some(name) = set.get("name").and_then(serde_json::Value::as_str) {
                    denylist.add(name);
                }
                if let Some(items) = set.get("items").and_then(serde_json::Value::as_array) {
                    for item in items {
                        for field in ["name", "folder", "org"] {
                            if let Some(value) = item.get(field).and_then(serde_json::Value::as_str)
                            {
                                denylist.add(value);
                            }
                        }
                        for field in ["url", "path"] {
                            if let Some(value) = item.get(field).and_then(serde_json::Value::as_str)
                            {
                                if field == "url" {
                                    denylist.add_remote(value);
                                } else {
                                    denylist.add_path(Path::new(value));
                                }
                            }
                        }
                    }
                }
            }
        }
        for path in repo_paths {
            denylist.add_path(path);
            for remote in remote_urls(path) {
                denylist.add_remote(&remote);
            }
        }
        denylist
    }

    fn add_system_identity(&mut self) {
        for key in ["USER", "USERNAME", "LOGNAME", "COMPUTERNAME", "HOSTNAME"] {
            if let Some(value) = std::env::var_os(key).and_then(|value| value.into_string().ok()) {
                if matches!(key, "USER" | "USERNAME" | "LOGNAME") {
                    self.add_username(&value);
                } else {
                    self.add(&value);
                }
            }
        }
        #[cfg(target_os = "linux")]
        if let Ok(hostname) = std::fs::read_to_string("/etc/hostname") {
            self.add(hostname.trim());
        }
    }

    pub fn add_path(&mut self, path: &Path) {
        for component in path.components() {
            if let Some(value) = component.as_os_str().to_str() {
                self.add(value);
            }
        }
    }

    pub fn add(&mut self, value: &str) {
        let value = value.trim();
        if value.chars().count() >= 3 {
            let token = value.to_lowercase();
            if !vocabulary::contains(&token) {
                self.tokens.insert(token);
            }
        }
    }

    fn add_username(&mut self, value: &str) {
        self.add(value);
    }

    fn add_remote(&mut self, value: &str) {
        for part in remote_parts(value) {
            self.add(&part);
        }
    }

    pub fn contains(&self, value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::String(text) => {
                let text = text.to_lowercase();
                self.tokens
                    .iter()
                    .any(|token| vocabulary::contains_word(&text, token))
            }
            serde_json::Value::Array(values) => values.iter().any(|value| self.contains(value)),
            serde_json::Value::Object(values) => values.values().any(|value| self.contains(value)),
            _ => false,
        }
    }
}

pub fn serialize_checked<T: Serialize>(
    value: &T,
    denylist: &Denylist,
) -> Result<String, &'static str> {
    let serialized = serde_json::to_string_pretty(value)
        .map_err(|_| "Diagnostics document could not be serialized")?;
    let value = serde_json::from_str(&serialized)
        .map_err(|_| "Diagnostics document could not be serialized")?;
    if denylist.contains(&value) {
        return Err(REFUSAL);
    }
    Ok(serialized)
}

fn home_path() -> Option<PathBuf> {
    ["USERPROFILE", "HOME"]
        .into_iter()
        .find_map(std::env::var_os)
        .map(PathBuf::from)
}

fn remote_urls(repo: &Path) -> Vec<String> {
    let git_entry = repo.join(".git");
    let git_dir = if git_entry.is_dir() {
        git_entry
    } else {
        let Ok(contents) = std::fs::read_to_string(&git_entry) else {
            return Vec::new();
        };
        let Some(path) = contents
            .lines()
            .find_map(|line| line.trim().strip_prefix("gitdir:"))
        else {
            return Vec::new();
        };
        let path = PathBuf::from(path.trim());
        if path.is_absolute() {
            path
        } else {
            repo.join(path)
        }
    };
    let common_dir = std::fs::read_to_string(git_dir.join("commondir"))
        .ok()
        .map(|value| {
            let path = PathBuf::from(value.trim());
            if path.is_absolute() {
                path
            } else {
                git_dir.join(path)
            }
        })
        .unwrap_or(git_dir);
    let Ok(config) = std::fs::read_to_string(common_dir.join("config")) else {
        return Vec::new();
    };
    let mut in_remote = false;
    config
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with('[') {
                in_remote = line.to_ascii_lowercase().starts_with("[remote ");
                return None;
            }
            if !in_remote {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            if !key.trim().eq_ignore_ascii_case("url") {
                return None;
            }
            let value = value.trim().trim_matches('"').trim_matches('\'');
            (!value.is_empty()).then(|| value.to_string())
        })
        .collect()
}

fn remote_parts(value: &str) -> Vec<String> {
    let value = value.trim().trim_matches('"').trim_matches('\'');
    let (authority, path) = if let Some((_, rest)) = value.split_once("://") {
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        (&rest[..end], rest.get(end..).unwrap_or_default())
    } else if let Some((authority, path)) = value.split_once(':') {
        if authority.contains('@') {
            (authority, path)
        } else {
            ("", value)
        }
    } else {
        ("", value)
    };
    let mut parts = Vec::new();
    if !authority.is_empty() {
        if let Some((login, _)) = authority.rsplit_once('@') {
            if !login.eq_ignore_ascii_case("git") {
                parts.push(login.to_string());
            }
        }
        let host = authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host);
        let host = host.split(':').next().unwrap_or(host);
        if !host.is_empty() {
            parts.push(host.to_string());
        }
    }
    let clean_path = path
        .trim_start_matches('/')
        .split(['?', '#'])
        .next()
        .unwrap_or_default();
    let mut segments = clean_path.split('/').filter(|segment| !segment.is_empty());
    if let Some(owner) = segments.next() {
        parts.push(owner.to_string());
    }
    if let Some(repo) = segments.next() {
        parts.push(repo.strip_suffix(".git").unwrap_or(repo).to_string());
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Source;
    use serde_json::json;

    #[test]
    fn planted_username_set_owner_and_path_component_refuse_case_insensitively() {
        let settings = Settings {
            sources: vec![Source {
                id: "source-id".into(),
                name: "Source".into(),
                kind: "github".into(),
                host: "github.example".into(),
                orgs: vec!["account-secret".into()],
                urls: vec!["https://remote.example/UrlOwner/private-repo.git".into()],
                credential_managed: false,
            }],
            workspace: json!({
                "sets": [{"name":"ClientCorp", "items":[{
                    "name":"acme-secret-repo", "folder":"repo-folder", "org":"AcmeOwner",
                    "url":"https://github.example/AcmeOwner/acme-secret-repo.git"
                }]}]
            }),
        };
        let mut denylist = Denylist::from_settings(
            &settings,
            &[PathBuf::from("private-folder/acme-secret-repo")],
        );
        denylist.add_username("username-secret");
        for token in [
            "username-secret",
            "account-secret",
            "ClientCorp",
            "AcmeOwner",
            "UrlOwner",
            "private-folder",
        ] {
            let serialized = json!({"value": token.to_ascii_lowercase()});
            assert!(denylist.contains(&serialized));
            assert_eq!(
                serialize_checked(&serialized, &denylist).unwrap_err(),
                REFUSAL
            );
        }
    }

    #[test]
    fn extracts_host_owner_and_repository_from_https_and_ssh_remotes() {
        assert_eq!(
            remote_parts("https://github.example/Owner/Repo.git"),
            ["github.example", "Owner", "Repo"]
        );
        assert_eq!(
            remote_parts("git@github.example:Owner/Repo.git"),
            ["github.example", "Owner", "Repo"]
        );
    }

    #[test]
    fn common_path_components_and_source_names_allow_fixed_export_strings() {
        let settings = Settings {
            sources: vec![Source {
                id: "source".into(),
                name: "GitHub Enterprise".into(),
                kind: "github".into(),
                host: "github.example".into(),
                orgs: Vec::new(),
                urls: Vec::new(),
                credential_managed: false,
            }],
            workspace: json!({"sets": []}),
        };
        let mut denylist = Denylist::from_settings(&settings, &[]);
        for path in ["C:/Users/admin/source/repos", "D:/git/work/app"] {
            denylist.add_path(Path::new(path));
        }
        let document = json!({
            "machine": {"osFamily": "windows", "systemDriveType": "ssd"},
            "timings": {"events": [{"phase": "git.process", "operation": "status"}]},
            "resources": {"git": {"count": 1}},
            "scale": {"repos": [{"repo": "repo-1", "worktree": false}]}
        });
        assert!(serialize_checked(&document, &denylist).is_ok());
    }

    #[test]
    fn scans_only_string_values_with_case_insensitive_word_boundaries() {
        let mut denylist = Denylist::default();
        denylist.add("ClientCorp");
        denylist.add("314");
        let clean = json!({
            "ClientCorp": "ClientCorporation",
            "count": 314,
            "nested": ["preClientCorp", "ClientCorp_suffix", "314159"]
        });
        assert!(serialize_checked(&clean, &denylist).is_ok());
        for value in [
            json!({"nested": ["clientcorp"]}),
            json!({"nested": {"value": "(CLIENTCORP)"}}),
            json!({"value": "https://example.test/ClientCorp/repo"}),
            json!({"value": "314"}),
        ] {
            assert_eq!(serialize_checked(&value, &denylist).unwrap_err(), REFUSAL);
        }
    }
}
