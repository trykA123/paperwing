use super::{native, Failure, SOURCES};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub(super) struct SavedToken {
    pub host: String,
    pub token: String,
}

impl SavedToken {
    pub fn for_host(self, host: &str) -> Result<String, String> {
        if !self.host.eq_ignore_ascii_case(host) {
            return Err(format!(
                "This token was saved for {}. Save a token for {host} first.",
                self.host
            ));
        }
        Ok(self.token)
    }
}

const PREFIX: &str = "skein-token-v1:";

pub(super) fn encode(host: &str, token: &str) -> Result<String, String> {
    serde_json::to_string(&SavedToken {
        host: host.into(),
        token: token.into(),
    })
    .map(|json| format!("{PREFIX}{json}"))
    .map_err(|_| "Cannot encode credential metadata".into())
}

pub(super) fn read(
    raw: String,
    saved_host: &str,
    write: impl FnOnce(&str) -> Result<(), String>,
) -> Result<SavedToken, String> {
    if let Some(json) = raw.strip_prefix(PREFIX) {
        return serde_json::from_str(json)
            .map_err(|_| "Invalid credential host metadata; save the token again".into());
    }
    if saved_host.is_empty() {
        return Err("Save a source host before using its token".into());
    }
    write(&encode(saved_host, &raw)?)?;
    Ok(SavedToken {
        host: saved_host.into(),
        token: raw,
    })
}

pub(super) fn saved_host(source_id: &str) -> Result<String, String> {
    let configured = SOURCES
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "Source configuration is unavailable")?;
    let configuration = configured
        .as_ref()
        .and_then(|sources| sources.get(source_id))
        .ok_or("Save the source before using its stored token")?;
    let value: serde_json::Value =
        serde_json::from_str(configuration).map_err(|_| "Invalid source configuration")?;
    Ok(value[1].as_str().ok_or("Saved source has no host")?.into())
}

pub(crate) fn check_saved_host(source_id: &str, host: &str) -> Result<(), String> {
    let saved = saved_host(source_id)?;
    if !saved.eq_ignore_ascii_case(host) {
        return Err(format!(
            "This token was saved for {saved}. Save a token for {host} first."
        ));
    }
    Ok(())
}

pub(super) fn read_token(
    raw: String,
    saved_host: &str,
    request_host: Option<&str>,
    write: impl FnOnce(&str) -> Result<(), String>,
) -> Result<String, String> {
    if request_host.is_none() && !raw.starts_with(PREFIX) {
        return Ok(raw);
    }
    let token = read(raw, saved_host, write)?;
    match request_host {
        Some(host) => token.for_host(host),
        None => Ok(token.token),
    }
}

pub(super) fn read_for_request(source_id: &str, request_host: Option<&str>) -> Result<Option<String>, String> {
    let Some(raw) = super::read_raw(source_id).map_err(Failure::message)? else {
        return Ok(None);
    };
    read_token(raw, &saved_host(source_id).unwrap_or_default(), request_host, |encoded| {
        native::write(source_id, encoded).map_err(Failure::message)
    }).map(Some)
}

pub(super) fn read_bound(source_id: &str) -> Result<Option<SavedToken>, String> {
    let Some(raw) = super::read_raw(source_id).map_err(Failure::message)? else {
        return Ok(None);
    };
    let saved = saved_host(source_id).unwrap_or_default();
    read(raw, &saved, |encoded| {
        native::write(source_id, encoded).map_err(Failure::message)
    })
    .map(Some)
}

fn host_changes(
    previous: &std::collections::HashMap<String, String>,
    sources: &[crate::settings::Source],
) -> Result<Vec<String>, String> {
    let next: std::collections::HashMap<_, _> = sources
        .iter()
        .map(|source| (source.id.as_str(), source))
        .collect();
    let mut changes = Vec::new();
    for (id, configuration) in previous {
        let value: serde_json::Value =
            serde_json::from_str(configuration).map_err(|_| "Invalid source configuration")?;
        let host = value[1].as_str().ok_or("Saved source has no host")?;
        let unowned_manual =
            value[0].as_str() == Some("manual") && value[4].as_bool() == Some(false);
        if !cfg!(windows)
            && unowned_manual
            && host.is_empty()
            && next.get(id.as_str()).is_none_or(|source| {
                source.kind == "manual" && !source.credential_managed && source.host.is_empty()
            })
        {
            continue;
        }
        if !next
            .get(id.as_str())
            .is_some_and(|next| host.eq_ignore_ascii_case(&next.host))
        {
            changes.push(id.clone());
        }
    }
    Ok(changes)
}

pub(crate) fn bind_before_host_edits(sources: &[crate::settings::Source]) -> Result<(), String> {
    let changes = {
        let configured = SOURCES
            .get_or_init(|| Mutex::new(None))
            .lock()
            .map_err(|_| "Source configuration is unavailable")?;
        match configured.as_ref() {
            Some(previous) => host_changes(previous, sources)?,
            None => Vec::new(),
        }
    };
    if changes.is_empty() {
        return Ok(());
    }
    bind_changes(changes, |id| read_bound(id).map(|_| ()))
}

fn bind_changes(changes: Vec<String>, read: impl Fn(&str) -> Result<(), String>) -> Result<(), String> {
    let _permit = tauri::async_runtime::block_on(super::admit_async())?;
    for id in changes {
        read(&id)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_tokens_migrate_without_resetting_the_token() {
        let mut stored = String::new();
        let token = read("synthetic-token".into(), "saved.invalid", |value| {
            stored = value.into();
            Ok(())
        })
        .unwrap();
        assert_eq!(token.token, "synthetic-token");
        assert_eq!(token.host, "saved.invalid");
        assert!(stored.contains("saved.invalid"));
        let migrated = read(stored, "edited.invalid", |_| panic!("already migrated")).unwrap();
        assert!(migrated.for_host("edited.invalid").is_err());
    }

    #[test]
    fn recorded_hosts_survive_source_host_edits_and_case_changes() {
        let raw = r#"skein-token-v1:{"host":"saved.invalid","token":"synthetic-token"}"#;
        let token = read(raw.into(), "edited.invalid", |_| panic!("already bound")).unwrap();
        assert_eq!(
            token.for_host("edited.invalid").unwrap_err(),
            "This token was saved for saved.invalid. Save a token for edited.invalid first."
        );
        let token = read(raw.into(), "edited.invalid", |_| panic!("already bound")).unwrap();
        assert_eq!(token.for_host("SAVED.INVALID").unwrap(), "synthetic-token");
    }

    #[test]
    fn failed_migration_refuses_use_instead_of_returning_an_unbound_token() {
        assert!(read("synthetic-token".into(), "saved.invalid", |_| Err(
            "locked".into()
        ))
        .is_err());
    }
}

#[cfg(test)]
mod invalid_metadata_tests {
    use super::*;

    #[test]
    fn malformed_metadata_is_refused_without_exposing_the_payload() {
        let error = read(
            "skein-token-v1:synthetic-secret".into(),
            "saved.invalid",
            |_| panic!("invalid metadata must not be overwritten"),
        )
        .err()
        .unwrap();
        assert!(!error.contains("synthetic-secret"));
    }
}

#[cfg(test)]
mod host_edit_tests {
    use super::*;

    #[test]
    fn host_edits_and_deletions_bind_legacy_tokens_before_saved_hosts_disappear() {
        let source: crate::settings::Source = serde_json::from_value(serde_json::json!({"id":"host-edit-fixture","name":"admin","kind":"ghe","host":"saved.invalid"})).unwrap();
        let configured = std::collections::HashMap::from([(
            source.id.clone(),
            super::super::source_configuration(&source),
        )]);
        assert!(host_changes(&configured, std::slice::from_ref(&source))
            .unwrap()
            .is_empty());
        let mut edited = source.clone();
        edited.host = "SAVED.INVALID".into();
        assert!(host_changes(&configured, std::slice::from_ref(&edited))
            .unwrap()
            .is_empty());
        edited.host = "new.invalid".into();
        assert_eq!(
            host_changes(&configured, &[edited]).unwrap().as_slice(),
            std::slice::from_ref(&source.id)
        );
        assert_eq!(host_changes(&configured, &[]).unwrap(), [source.id]);
    }
}

#[cfg(test)]
mod manual_source_tests {
    use super::*;

    #[test]
    fn unowned_manual_sources_without_hosts_do_not_require_a_wallet_on_removal() {
        let mut source: crate::settings::Source = serde_json::from_value(
            serde_json::json!({"id":"manual-no-token-fixture","name":"admin","kind":"manual"}),
        )
        .unwrap();
        let configured = std::collections::HashMap::from([(
            source.id.clone(),
            super::super::source_configuration(&source),
        )]);
        let changes = host_changes(&configured, &[]).unwrap();
        if cfg!(windows) {
            assert_eq!(changes.as_slice(), std::slice::from_ref(&source.id));
        } else {
            assert!(changes.is_empty());
        }
        source.kind = "ghe".into();
        source.host = "new.invalid".into();
        assert_eq!(
            host_changes(&configured, std::slice::from_ref(&source))
                .unwrap()
                .as_slice(),
            std::slice::from_ref(&source.id)
        );
        source.kind = "manual".into();
        source.host.clear();
        source.credential_managed = true;
        let configured = std::collections::HashMap::from([(
            source.id.clone(),
            super::super::source_configuration(&source),
        )]);
        assert_eq!(host_changes(&configured, &[]).unwrap(), [source.id]);
    }
}

#[cfg(test)]
mod regression_tests {
    use super::*;

    #[tokio::test]
    async fn hostless_manual_legacy_token_redacts_git_errors_without_migration_regression() {
        let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
        let source: crate::settings::Source = serde_json::from_value(serde_json::json!({
            "id":"manual-legacy-regression", "name":"admin", "kind":"manual", "credentialManaged":true
        })).unwrap();
        let mut migrated = false;
        let token = read_token("synthetic-legacy-secret".into(), &source.host, None, |_| {
            migrated = true;
            Ok(())
        }).map(Some);
        let _credentials = crate::git::CredentialFixture::new(std::collections::BTreeMap::from([
            (source.id, token),
        ]));
        assert_eq!(crate::git::safe("fatal: rejected synthetic-legacy-secret"), "fatal: rejected [redacted]");
        assert!(!migrated);
        assert!(read_token("synthetic-legacy-secret".into(), "", Some("new.invalid"), |_| Ok(())).is_err());
    }

    #[test]
    fn redaction_reads_do_not_migrate_even_when_the_source_has_a_host_regression() {
        assert_eq!(read_token("synthetic-token".into(), "saved.invalid", None, |_| {
            Err("redaction must not write".into())
        }).unwrap(), "synthetic-token");
    }

    #[tokio::test]
    async fn saving_a_host_edit_waits_for_a_concurrent_credential_read_regression() {
        let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
        let permit = super::super::admit().unwrap();
        let (started, starting) = std::sync::mpsc::sync_channel(0);
        let (done, result) = std::sync::mpsc::channel();
        let save = std::thread::spawn(move || {
            started.send(()).unwrap();
            done.send(bind_changes(vec!["saved-source".into()], |_| Ok(()))).unwrap();
        });
        starting.recv().unwrap();
        let early = result.recv_timeout(std::time::Duration::from_millis(100));
        drop(permit);
        save.join().unwrap();
        assert!(matches!(early, Err(std::sync::mpsc::RecvTimeoutError::Timeout)), "save did not wait: {early:?}");
        result.recv_timeout(std::time::Duration::from_secs(3)).unwrap().unwrap();
    }
}

#[cfg(test)]
mod direct_redaction_tests {
    #[tokio::test]
    async fn hostless_manual_legacy_token_redacts_git_output_through_credentials_regression() {
        let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
        let source: crate::settings::Source = serde_json::from_value(serde_json::json!({
            "id": "direct-manual-legacy", "name": "admin", "kind": "manual", "credentialManaged": true
        })).unwrap();
        let _raw =
            super::super::test_support::RawToken::new(&source.id, "synthetic-direct-legacy-secret");
        assert!(source.host.is_empty());
        crate::git::configure_sources(vec![source.id.clone()]);
        let token = crate::credentials::get_token(&source.id);
        let output = crate::git::safe("fatal: rejected synthetic-direct-legacy-secret");
        crate::git::configure_sources(Vec::new());
        assert_eq!(
            token.unwrap().as_deref(),
            Some("synthetic-direct-legacy-secret")
        );
        assert_eq!(output, "fatal: rejected [redacted]");
        assert_eq!(
            super::super::test_support::read(&source.id).as_deref(),
            Some("synthetic-direct-legacy-secret")
        );
    }
}
