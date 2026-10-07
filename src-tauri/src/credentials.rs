use crate::kernel::events::CoreEvent;
mod host;
pub(crate) use host::{bind_before_host_edits, check_saved_host};

use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Manager};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[cfg(not(feature = "test-profile"))]
const SERVICE: &str = "paperwing";
#[cfg(feature = "test-profile")]
const SERVICE: &str = "paperwing-testing-fixtures-v1";

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CredentialState {
    Saved,
    Missing,
    Locked,
    Unavailable,
    PermissionDenied,
    Uncertain,
    Error,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capability {
    pub backend: &'static str,
    pub persistent: bool,
    pub supported: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub source_id: String,
    pub backend: &'static str,
    pub state: CredentialState,
    pub revision: u64,
    pub reason: Option<String>,
}

#[derive(Clone, Copy, Debug)]
struct Failure {
    state: CredentialState,
    ambiguous: bool,
}

impl Failure {
    fn new(state: CredentialState) -> Self {
        Self { state, ambiguous: false }
    }

    fn message(self) -> String {
        reason(self.state).into()
    }
}

fn reason(state: CredentialState) -> &'static str {
    match state {
        CredentialState::Saved => "Token is saved in the credential store.",
        CredentialState::Missing => "No token is saved for this source.",
        CredentialState::Locked => "The credential store is locked. Unlock it in your desktop wallet, then retry.",
        CredentialState::Unavailable => "The desktop credential service is unavailable. Start or configure a persistent Secret Service store, then retry.",
        CredentialState::PermissionDenied => "The credential store denied access. Allow Skein access in your desktop wallet, then retry.",
        CredentialState::Uncertain => "The credential operation lost its response. The token may have changed. Unlock or reconnect the store, then explicitly retry saving or deleting.",
        CredentialState::Error => "The credential store could not complete the operation. Retry after checking your desktop wallet.",
    }
}

fn backend() -> &'static str {
    if cfg!(windows) {
        "windowsCredentialManager"
    } else if cfg!(target_os = "linux") {
        "secretService"
    } else {
        "unsupported"
    }
}

#[derive(Default)]
struct Revision {
    value: u64,
    uncertain: bool,
}

static REVISIONS: OnceLock<Mutex<HashMap<String, Revision>>> = OnceLock::new();
static SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
static WAITERS: OnceLock<Arc<Semaphore>> = OnceLock::new();
static SOURCES: OnceLock<Mutex<Option<HashMap<String, String>>>> = OnceLock::new();

fn revisions() -> &'static Mutex<HashMap<String, Revision>> {
    REVISIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(test)]
fn admit() -> Result<OwnedSemaphorePermit, String> {
    SLOTS.get_or_init(|| Arc::new(Semaphore::new(1))).clone().try_acquire_owned()
        .map_err(|_| "The credential store is busy. Retry when the current operation finishes.".into())
}

#[cfg(test)]
pub(crate) fn hold_slot() -> OwnedSemaphorePermit {
    admit().expect("credential slot is free")
}

async fn admit_async() -> Result<OwnedSemaphorePermit, String> {
    let _waiting = WAITERS.get_or_init(|| Arc::new(Semaphore::new(32))).clone().try_acquire_owned()
        .map_err(|_| "The credential store is busy. Retry when the current operation finishes.".to_string())?;
    tokio::time::timeout(std::time::Duration::from_secs(2),
        SLOTS.get_or_init(|| Arc::new(Semaphore::new(1))).clone().acquire_owned()).await
        .map_err(|_| "The credential store is busy. Retry when the current operation finishes.".to_string())?
        .map_err(|_| "The credential store is unavailable".into())
}

fn validate(source_id: &str) -> Result<(), String> {
    crate::settings::valid_id(source_id)?;
    #[cfg(feature = "test-profile")]
    crate::test_profile::source(source_id)?;
    Ok(())
}

pub fn revision(source_id: &str) -> u64 {
    revisions().lock().unwrap().get(source_id).map_or(0, |entry| entry.value)
}

#[tauri::command]
pub fn source_revision(source_id: String) -> Result<u64, String> {
    validate(&source_id)?;
    Ok(revision(&source_id))
}

pub fn if_current<T>(source_id: &str, expected: u64, action: impl FnOnce() -> T) -> Option<T> {
    let entries = revisions().lock().unwrap();
    (entries.get(source_id).map_or(0, |entry| entry.value) == expected).then(action)
}

fn source_configuration(source: &crate::settings::Source) -> String {
    serde_json::json!([source.kind, source.host, source.orgs, source.urls, source.credential_managed, source.enabled]).to_string()
}

fn check_configuration(source: &crate::settings::Source, configured: Option<&HashMap<String, String>>) -> Result<(), String> {
    if configured.is_some_and(|sources| sources.get(&source.id) != Some(&source_configuration(source))) {
        return Err("Source configuration changed; reload repository metadata".into());
    }
    Ok(())
}

pub fn metadata_revision(source: &crate::settings::Source) -> Result<u64, String> {
    let configured = SOURCES.get_or_init(|| Mutex::new(None)).lock().map_err(|_| "Source configuration is unavailable")?;
    check_configuration(source, configured.as_ref())?;
    Ok(revision(&source.id))
}

fn invalidate_blocking<R: tauri::Runtime>(app: &AppHandle<R>, source_id: &str) {
    let revision = advance_revision(source_id, || {
        if let Some(store) = app.try_state::<crate::store::Store>() {
            store.remove_source(source_id);
        }
    });
    let _ = crate::events::publish_payload(app, CoreEvent::CredentialChanged, &serde_json::json!({ "sourceId": source_id, "revision": revision }));
}

async fn invalidate_with(source_id: String, clear: impl FnOnce() + Send + 'static) -> Result<u64, String> {
    tauri::async_runtime::spawn_blocking(move || advance_revision(&source_id, clear)).await
        .map_err(|_| "Credential invalidation task failed".to_string())
}

async fn invalidate(app: &AppHandle, source_id: &str) -> Result<(), String> {
    let store = app.try_state::<crate::store::Store>().map(|store| store.inner().clone());
    let id = source_id.to_string();
    let revision = invalidate_with(id.clone(), move || {
        if let Some(store) = store { store.remove_source(&id); }
    }).await?;
    let _ = crate::events::publish_payload(app, CoreEvent::CredentialChanged, &serde_json::json!({ "sourceId": source_id, "revision": revision }));
    Ok(())
}

pub(crate) fn advance_revision(source_id: &str, clear: impl FnOnce()) -> u64 {
    let mut entries = revisions().lock().unwrap();
    let entry = entries.entry(source_id.into()).or_default();
    entry.value += 1;
    clear();
    entry.value
}

pub fn configure_sources<R: tauri::Runtime>(app: &AppHandle<R>, sources: &[crate::settings::Source], notify: bool) {
    let next: HashMap<_, _> = sources.iter().map(|source| (source.id.clone(),
        source_configuration(source))).collect();
    let mut previous = SOURCES.get_or_init(|| Mutex::new(None)).lock().unwrap();
    if previous.is_some() || notify {
        let empty = HashMap::new();
        let previous = previous.as_ref().unwrap_or(&empty);
        for id in previous.keys().chain(next.keys()).collect::<std::collections::HashSet<_>>() {
            if previous.get(id) != next.get(id) { invalidate_blocking(app, id); }
        }
    }
    *previous = Some(next);
}

#[cfg(test)]
mod test_support;

fn read_raw(source_id: &str) -> Result<Option<String>, Failure> {
    #[cfg(test)]
    if let Some(raw) = test_support::read(source_id) {
        return Ok(Some(raw));
    }
    native::read(source_id)
}

#[cfg(test)]
pub fn get_token(source_id: &str) -> Result<Option<String>, String> {
    validate(source_id)?;
    let _permit = admit()?;
    if revisions().lock().unwrap().get(source_id).is_some_and(|entry| entry.uncertain) {
        return Err(reason(CredentialState::Uncertain).into());
    }
    host::read_for_request(source_id, None)
}

pub async fn read(source_id: String) -> Result<Option<String>, String> {
    read_for_host(source_id, None).await
}

pub(crate) async fn read_for_host(source_id: String, request_host: Option<String>) -> Result<Option<String>, String> {
    validate(&source_id)?;
    let permit = admit_async().await?;
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        if revisions().lock().unwrap().get(&source_id).is_some_and(|entry| entry.uncertain) {
            return Err(reason(CredentialState::Uncertain).into());
        }
        host::read_for_request(&source_id, request_host.as_deref())
    }).await
        .map_err(|_| "Credential task failed".to_string())?
}

pub async fn capability() -> Capability {
    let permit = match admit_async().await {
        Ok(permit) => permit,
        Err(reason) => return Capability { backend: backend(), persistent: cfg!(any(windows, target_os = "linux")),
            supported: false, reason: Some(reason) },
    };
    let result = tauri::async_runtime::spawn_blocking(move || { let _permit = permit; native::ready() }).await
        .unwrap_or_else(|_| Err(Failure::new(CredentialState::Error)));
    Capability { backend: backend(), persistent: cfg!(any(windows, target_os = "linux")),
        supported: result.is_ok(), reason: result.err().map(Failure::message) }
}

#[tauri::command]
pub async fn credential_status(source_id: String) -> Result<Status, String> {
    validate(&source_id)?;
    let permit = admit_async().await?;
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        let observed = native::status(&source_id);
        let entries = revisions().lock().unwrap();
        let entry = entries.get(&source_id);
        let state = if entry.is_some_and(|entry| entry.uncertain) { CredentialState::Uncertain }
            else { observed.unwrap_or_else(|error| error.state) };
        Ok(Status { source_id, backend: backend(), state, revision: entry.map_or(0, |entry| entry.value),
            reason: (!matches!(state, CredentialState::Saved | CredentialState::Missing)).then(|| reason(state).into()) })
    }).await.map_err(|_| "Credential task failed".to_string())?
}

async fn mutate(app: AppHandle, source_id: String, token: Option<String>, host: Option<String>) -> Result<(), String> {
    validate(&source_id)?;
    if token.as_ref().is_some_and(|token| token.trim().is_empty()) { return Err("Token is empty".into()); }
    let permit = admit_async().await?;
    let secret = token.as_ref().map(|token| token.trim().to_string());
    let token = token.map(|token| {
        let host = host.map(Ok).unwrap_or_else(|| host::saved_host(&source_id))?;
        crate::github::valid_host(&host)?;
        host::encode(&host, token.trim())
    }).transpose()?;
    invalidate(&app, &source_id).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        finish_mutation(&source_id, secret.as_deref(), || match token {
            Some(token) => native::write(&source_id, token.trim()), None => native::delete(&source_id)
        })
    }).await.map_err(|_| "Credential task failed".to_string())?
}

fn finish_mutation(source_id: &str, secret: Option<&str>, operation: impl FnOnce() -> Result<(), Failure>) -> Result<(), String> {
    complete_mutation(source_id, operation)?;
    if let Some(secret) = secret { crate::git::remember_secret(secret); }
    Ok(())
}

#[cfg(test)]
pub(crate) fn finish_for_test(source_id: &str, secret: Option<&str>) {
    finish_mutation(source_id, secret, || Ok(())).unwrap();
}

fn complete_mutation(source_id: &str, operation: impl FnOnce() -> Result<(), Failure>) -> Result<(), String> {
    let previous = {
        let mut entries = revisions().lock().unwrap();
        let entry = entries.entry(source_id.into()).or_default();
        let previous = entry.uncertain;
        entry.uncertain = true;
        previous
    };
    let result = operation();
    let mut entries = revisions().lock().unwrap();
    let entry = entries.entry(source_id.into()).or_default();
    match result {
        Ok(()) => { entry.uncertain = false; Ok(()) }
        Err(error) => {
            entry.uncertain = previous || error.ambiguous;
            Err(if entry.uncertain { reason(CredentialState::Uncertain).into() } else { error.message() })
        }
    }
}

pub async fn set_token(app: AppHandle, source_id: String, token: String, host: Option<String>) -> Result<(), String> {
    mutate(app, source_id, Some(token), host).await
}

pub async fn delete_token(app: AppHandle, source_id: String) -> Result<(), String> {
    mutate(app, source_id, None, None).await
}

#[cfg(target_os = "linux")]
mod native {
    use super::*;
    use dbus::blocking::{Connection, stdintf::org_freedesktop_dbus::Properties};
    use dbus_secret_service::{EncryptionType, Error, SecretService};
    use std::time::Duration;

    pub(super) fn dbus_failure(name: Option<&str>) -> Failure {
        let (state, ambiguous) = match name {
            Some("org.freedesktop.DBus.Error.AccessDenied" | "org.freedesktop.Secret.Error.PermissionDenied") => (CredentialState::PermissionDenied, false),
            Some("org.freedesktop.Secret.Error.IsLocked") => (CredentialState::Locked, false),
            Some("org.freedesktop.DBus.Error.NoReply" | "org.freedesktop.DBus.Error.Timeout" | "org.freedesktop.DBus.Error.TimedOut" | "org.freedesktop.DBus.Error.Disconnected") => (CredentialState::Unavailable, true),
            Some("org.freedesktop.DBus.Error.ServiceUnknown" | "org.freedesktop.DBus.Error.NameHasNoOwner" | "org.freedesktop.DBus.Error.NoServer" | "org.freedesktop.DBus.Error.FileNotFound") => (CredentialState::Unavailable, false),
            _ => (CredentialState::Error, false),
        };
        Failure { state, ambiguous }
    }

    fn failure(error: Error) -> Failure {
        match error {
            Error::Locked | Error::Prompt => Failure::new(CredentialState::Locked),
            Error::Unavailable | Error::NoResult => Failure::new(CredentialState::Unavailable),
            Error::Dbus(error) => dbus_failure(error.name()),
            _ => Failure::new(CredentialState::Error),
        }
    }

    fn before_dispatch(error: Failure) -> Failure {
        Failure { ambiguous: false, ..error }
    }

    fn dispatched_failure(error: Error) -> Failure {
        let error = failure(error);
        Failure { ambiguous: error.ambiguous || matches!(error.state, CredentialState::Error | CredentialState::Unavailable), ..error }
    }

    pub fn ready() -> Result<(), Failure> {
        let connection = Connection::new_session().map_err(|error| dbus_failure(error.name()))?;
        let bus = connection.with_proxy("org.freedesktop.DBus", "/org/freedesktop/DBus", Duration::from_secs(2));
        let (owned,): (bool,) = bus.method_call("org.freedesktop.DBus", "NameHasOwner", ("org.freedesktop.secrets",))
            .map_err(|error| dbus_failure(error.name()))?;
        if !owned { return Err(Failure::new(CredentialState::Unavailable)); }
        let service = connection.with_proxy("org.freedesktop.secrets", "/org/freedesktop/secrets", Duration::from_secs(2));
        let (path,): (dbus::Path<'static>,) = service.method_call("org.freedesktop.Secret.Service", "ReadAlias", ("default",))
            .map_err(|error| dbus_failure(error.name()))?;
        if &*path == "/" { return Err(Failure::new(CredentialState::Unavailable)); }
        let collection = connection.with_proxy("org.freedesktop.secrets", path, Duration::from_secs(2));
        let locked: bool = collection.get("org.freedesktop.Secret.Collection", "Locked")
            .map_err(|error| dbus_failure(error.name()))?;
        if locked { return Err(Failure::new(CredentialState::Locked)); }
        Ok(())
    }

    fn connect() -> Result<SecretService, Failure> {
        ready().map_err(before_dispatch)?;
        SecretService::connect_with_max_prompt_timeout(EncryptionType::Dh, 0).map_err(|error| before_dispatch(failure(error)))
    }

    fn attributes(source_id: &str) -> HashMap<&str, &str> {
        HashMap::from([("service", SERVICE), ("username", source_id), ("target", "default")])
    }

    pub fn status(source_id: &str) -> Result<CredentialState, Failure> {
        let service = connect()?;
        let items = service.search_items(attributes(source_id)).map_err(failure)?;
        if !items.locked.is_empty() { return Err(Failure::new(CredentialState::Locked)); }
        match items.unlocked.len() {
            0 => Ok(CredentialState::Missing),
            1 => Ok(CredentialState::Saved),
            _ => Err(Failure::new(CredentialState::Error)),
        }
    }

    pub fn read(source_id: &str) -> Result<Option<String>, Failure> {
        let service = connect()?;
        let items = service.search_items(attributes(source_id)).map_err(failure)?;
        if !items.locked.is_empty() { return Err(Failure::new(CredentialState::Locked)); }
        if items.unlocked.len() > 1 { return Err(Failure::new(CredentialState::Error)); }
        let Some(item) = items.unlocked.first() else { return Ok(None); };
        String::from_utf8(item.get_secret().map_err(failure)?)
            .map(Some).map_err(|_| Failure::new(CredentialState::Error))
    }

    pub fn write(source_id: &str, token: &str) -> Result<(), Failure> {
        let service = connect()?;
        let items = service.search_items(attributes(source_id)).map_err(|error| before_dispatch(failure(error)))?;
        if !items.locked.is_empty() { return Err(Failure::new(CredentialState::Locked)); }
        if items.unlocked.len() > 1 { return Err(Failure::new(CredentialState::Error)); }
        if let Some(item) = items.unlocked.first() {
            return item.set_secret(token.as_bytes(), "text/plain").map_err(dispatched_failure);
        }
        let collection = service.get_default_collection().map_err(|error| before_dispatch(failure(error)))?;
        if collection.is_locked().map_err(|error| before_dispatch(failure(error)))? { return Err(Failure::new(CredentialState::Locked)); }
        collection.create_item("Skein token", attributes(source_id), token.as_bytes(), true, "text/plain")
            .map(|_| ()).map_err(dispatched_failure)
    }

    pub fn delete(source_id: &str) -> Result<(), Failure> {
        let service = connect()?;
        let items = service.search_items(attributes(source_id)).map_err(|error| before_dispatch(failure(error)))?;
        if !items.locked.is_empty() { return Err(Failure::new(CredentialState::Locked)); }
        if items.unlocked.len() > 1 { return Err(Failure::new(CredentialState::Error)); }
        if let Some(item) = items.unlocked.first() { item.delete().map_err(dispatched_failure)?; }
        Ok(())
    }
}

#[cfg(feature = "test-profile")]
pub fn drill() {
    if !std::env::args().any(|arg| arg == "--paperwing-credential-drill") { return; }
    use std::io::Read;
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Request { operation: String, source_id: String, token: Option<String>, expected: Option<String> }
    let result = (|| -> Result<serde_json::Value, String> {
        crate::test_profile::ensure()?;
        let mut input = Vec::new();
        std::io::stdin().take(128 * 1024 + 1).read_to_end(&mut input).map_err(|_| "Fixture input failed")?;
        if input.len() > 128 * 1024 { return Err("Fixture input exceeded limit".into()); }
        let request: Request = serde_json::from_slice(&input).map_err(|_| "Fixture input invalid")?;
        validate(&request.source_id)?;
        match request.operation.as_str() {
            "capability" => {
                let result = native::ready();
                Ok(serde_json::json!({ "backend": backend(), "supported": result.is_ok(), "reason": result.err().map(Failure::message) }))
            }
            "status" => {
                let state = native::status(&request.source_id).unwrap_or_else(|error| error.state);
                Ok(serde_json::json!({ "state": state, "reason": (!matches!(state, CredentialState::Saved | CredentialState::Missing)).then(|| reason(state)) }))
            }
            "set" => {
                let token = request.token.ok_or("Fixture token missing")?;
                if token.is_empty() { return Err("Fixture token empty".into()); }
                native::write(&request.source_id, &host::encode("github.com", &token)?).map_err(Failure::message)?;
                Ok(serde_json::json!({ "saved": true }))
            }
            "verify" => {
                let expected = request.expected.ok_or("Fixture expectation missing")?;
                let stored = native::read(&request.source_id).map_err(Failure::message)?
                    .map(|raw| host::read(raw, "github.com", |encoded| native::write(&request.source_id, encoded).map_err(Failure::message))).transpose()?;
                let matches = stored.as_ref().map(|token| token.token.as_str()) == Some(expected.as_str());
                Ok(serde_json::json!({ "matches": matches }))
            }
            "delete" => {
                native::delete(&request.source_id).map_err(Failure::message)?;
                Ok(serde_json::json!({ "deleted": true }))
            }
            "api" => {
                crate::test_profile::github_endpoint()?.ok_or("Isolated API endpoint required by credential drill")?;
                let source = crate::settings::Source { id: request.source_id, name: "Credential fixture".into(),
                    kind: "github".into(), host: "github.com".into(), orgs: Vec::new(), urls: Vec::new(), enabled: true, credential_managed: true };
                *SOURCES.get_or_init(|| Mutex::new(None)).lock().map_err(|_| "Fixture source configuration unavailable")? =
                    Some(HashMap::from([(source.id.clone(), source_configuration(&source))]));
                let login = tauri::async_runtime::block_on(crate::github::test_source(source, None))?;
                Ok(serde_json::json!({ "login": login }))
            }
            _ => Err("Fixture operation invalid".into()),
        }
    })();
    let output = match result { Ok(value) => serde_json::json!({ "ok": true, "result": value }),
        Err(error) => serde_json::json!({ "ok": false, "error": error }) };
    println!("{output}");
    std::process::exit(if output["ok"] == true { 0 } else { 1 });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_sources_round_trip_without_new_fields_and_credential_ownership_contains_no_secret() {
        let value = serde_json::json!({ "id": "manual", "name": "Manual fixture", "kind": "manual",
            "host": "", "orgs": [], "urls": [] });
        let mut source: crate::settings::Source = serde_json::from_value(value.clone()).unwrap();
        assert!(!source.credential_managed);
        assert_eq!(serde_json::to_value(&source).unwrap(), value);
        source.credential_managed = true;
        let managed = serde_json::to_value(source).unwrap();
        assert_eq!(managed["credentialManaged"], true);
        assert_eq!(managed.as_object().unwrap().len(), 7);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn typed_store_failures_distinguish_lock_denial_unavailability_and_unknown_errors() {
        for (name, state, ambiguous) in [
            ("org.freedesktop.Secret.Error.IsLocked", CredentialState::Locked, false),
            ("org.freedesktop.DBus.Error.AccessDenied", CredentialState::PermissionDenied, false),
            ("org.freedesktop.DBus.Error.ServiceUnknown", CredentialState::Unavailable, false),
            ("org.freedesktop.DBus.Error.NoReply", CredentialState::Unavailable, true),
            ("org.freedesktop.DBus.Error.Disconnected", CredentialState::Unavailable, true),
            ("provider.secret-bearing-unrecognized-error", CredentialState::Error, false),
        ] {
            let failure = native::dbus_failure(Some(name));
            assert_eq!(failure.state, state);
            assert_eq!(failure.ambiguous, ambiguous);
            assert!(!failure.message().contains(name));
        }
    }

    #[test]
    fn changed_revision_prevents_old_metadata_cache_writes_and_token_hashes_are_absent() {
        let id = "fixture-revision-proof";
        let previous = revision(id);
        let mut entries = revisions().lock().unwrap();
        entries.entry(id.into()).or_default().value += 1;
        drop(entries);
        let mut wrote = false;
        assert_eq!(if_current(id, previous, || { wrote = true; }), None);
        assert!(!wrote);
        assert_eq!(if_current(id, revision(id), || 42), Some(42));
        let payload = serde_json::to_value(Status { source_id: id.into(), backend: backend(),
            state: CredentialState::Uncertain, revision: revision(id), reason: Some(reason(CredentialState::Uncertain).into()) }).unwrap();
        assert_eq!(payload["state"], "uncertain");
        assert_eq!(payload.as_object().unwrap().len(), 5);
    }

    #[test]
    fn committed_but_timed_out_mutations_keep_uncertainty_until_an_explicit_success() {
        let source_id = "fixture-ambiguous-proof";
        let original = revision(source_id);
        advance_revision(source_id, || ());
        let mut committed = false;
        let result = complete_mutation(source_id, || {
            assert_eq!(revision(source_id), original + 1);
            committed = true;
            Err(Failure { state: CredentialState::Unavailable, ambiguous: true })
        });
        assert!(committed);
        assert_eq!(result, Err(reason(CredentialState::Uncertain).into()));
        assert!(revisions().lock().unwrap()[source_id].uncertain);
        assert_eq!(if_current(source_id, original, || "old metadata"), None);
        assert!(complete_mutation(source_id, || Err(Failure::new(CredentialState::Locked))).is_err());
        assert!(revisions().lock().unwrap()[source_id].uncertain);
        assert_eq!(complete_mutation(source_id, || Ok(())), Ok(()));
        assert!(!revisions().lock().unwrap()[source_id].uncertain);
    }
}

#[cfg(windows)]
mod native {
    use super::*;

    fn entry(source_id: &str) -> Result<keyring::Entry, Failure> {
        keyring::Entry::new(SERVICE, source_id).map_err(failure)
    }

    fn failure(error: keyring::Error) -> Failure {
        Failure::new(match error {
            keyring::Error::NoEntry => CredentialState::Missing,
            keyring::Error::NoStorageAccess(_) => CredentialState::PermissionDenied,
            _ => CredentialState::Error,
        })
    }

    pub fn ready() -> Result<(), Failure> { Ok(()) }

    pub fn read(source_id: &str) -> Result<Option<String>, Failure> {
        match entry(source_id)?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => {
                #[cfg(not(feature = "test-profile"))]
                if let Ok(legacy) = keyring::Entry::new("flock", source_id) {
                    match legacy.get_password() {
                        Ok(token) => { let _ = entry(source_id)?.set_password(&token); return Ok(Some(token)); }
                        Err(keyring::Error::NoEntry) => (),
                        Err(error) => return Err(failure(error)),
                    }
                }
                Ok(None)
            }
            Err(error) => Err(failure(error)),
        }
    }

    pub fn status(source_id: &str) -> Result<CredentialState, Failure> {
        read(source_id).map(|token| if token.is_some() { CredentialState::Saved } else { CredentialState::Missing })
    }

    pub fn write(source_id: &str, token: &str) -> Result<(), Failure> {
        entry(source_id)?.set_password(token).map_err(failure)
    }

    pub fn delete(source_id: &str) -> Result<(), Failure> {
        #[cfg(not(feature = "test-profile"))]
        if let Ok(legacy) = keyring::Entry::new("flock", source_id) { let _ = legacy.delete_credential(); }
        match entry(source_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(failure(error)),
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod native {
    use super::*;
    pub fn ready() -> Result<(), Failure> { Err(Failure::new(CredentialState::Unavailable)) }
    pub fn status(_: &str) -> Result<CredentialState, Failure> { ready().map(|_| CredentialState::Missing) }
    pub fn read(_: &str) -> Result<Option<String>, Failure> { ready().map(|_| None) }
    pub fn write(_: &str, _: &str) -> Result<(), Failure> { ready() }
    pub fn delete(_: &str) -> Result<(), Failure> { ready() }
}

#[cfg(test)]
#[path = "credentials_metadata_tests.rs"]
mod metadata_tests;

#[cfg(test)]
pub(crate) fn token_fixture(raw: &str, request_host: &str) -> Result<Option<String>, String> {
    host::read(raw.into(), "", |_| Err("Fixture metadata must already be bound".into()))?
        .for_host(request_host).map(Some)
}
