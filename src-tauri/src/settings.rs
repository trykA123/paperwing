mod persistence;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[cfg(not(feature = "test-profile"))]
const LEGACY_IDENTIFIER: &str = "dev.flock.app";

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub name: String,
    /// "github" | "ghe" | "manual"
    pub kind: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub orgs: Vec<String>,
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub credential_managed: bool,
}

fn is_false(value: &bool) -> bool { !value }

fn redaction_sources(sources: &[Source]) -> Vec<String> {
    sources.iter().filter(|source| source.kind != "manual" || source.credential_managed)
        .map(|source| source.id.clone()).collect()
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub sources: Vec<Source>,
    /// UI-owned state (sets, favourites, options); the backend only persists it.
    #[serde(default)]
    pub workspace: serde_json::Value,
}

pub fn valid_id(id: &str) -> Result<(), String> {
    let ok = !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if ok {
        Ok(())
    } else {
        Err(format!("Invalid source id: {id}"))
    }
}

fn settings_file(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(feature = "test-profile")]
    crate::test_profile::validate(app)?;
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    let file = dir.join("settings.json");
    #[cfg(feature = "test-profile")]
    crate::test_profile::plain_file(&file)?;
    Ok(file)
}

fn prepare_settings(app: &AppHandle) -> Result<PathBuf, String> {
    let file = settings_file(app)?;
    std::fs::create_dir_all(file.parent().ok_or("Settings directory is missing")?).map_err(|e| e.to_string())?;
    Ok(file)
}

pub fn load_settings(app: AppHandle) -> Result<Settings, String> {
    Ok(load_with_status(&app)?.settings)
}

fn load_with_status(app: &AppHandle) -> Result<Loaded, String> {
    if let Some(state) = app.try_state::<Startup>() { state.check()?; }
    let file = settings_file(app)?;
    #[cfg(feature = "test-profile")]
    if !file.exists() {
        return Err("Test profile settings must be prepared before launch".into());
    }
    let (settings, restored_from_backup) = persistence::load(&file)?;
    #[cfg(feature = "test-profile")]
    crate::test_profile::settings(&settings)?;
    for source in &settings.sources { valid_id(&source.id)?; }
    Ok(Loaded { settings, restored_from_backup })
}

fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    static SAVES: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _save = SAVES.lock().map_err(|_| "Settings persistence is unavailable")?;
    let _filesystem = crate::git::filesystem_gate().try_read().map_err(|_| "A recoverable write is in progress; retry saving settings")?;
    #[cfg(feature = "test-profile")]
    crate::test_profile::settings(&settings)?;
    for source in &settings.sources { valid_id(&source.id)?; }
    let file = prepare_settings(&app)?;
    crate::credentials::bind_before_host_edits(&settings.sources)?;
    persistence::save(&file, &settings)?;
    if let Some(state) = app.try_state::<Startup>() {
        *state.0.lock().map_err(|_| "Settings initialization is unavailable")? = None;
    }
    crate::credentials::configure_sources(&app, &settings.sources, true);
    crate::git::configure_sources(redaction_sources(&settings.sources));
    Ok(())
}

#[derive(Default)]
pub(crate) struct Startup(std::sync::Mutex<Option<String>>);

impl Startup {
    fn record(&self, error: String) {
        eprintln!("Settings initialization failed; starting with defaults: {error}");
        *self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(error);
    }

    fn check(&self) -> Result<(), String> {
        match self.0.lock().map_err(|_| "Settings initialization is unavailable")?.as_ref() {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }
}

fn initialize_with(
    state: &Startup,
    load: impl FnOnce() -> Result<Settings, String>,
    configure: impl FnOnce(&[Source]),
) -> Result<(), String> {
    let settings = match load() {
        Ok(settings) => settings,
        Err(error) => {
            state.record(error);
            Settings::default()
        }
    };
    configure(&settings.sources);
    Ok(())
}

fn prepare_initial_settings(app: &AppHandle) -> Result<Settings, String> {
    let file = prepare_settings(app)?;
    #[cfg(not(feature = "test-profile"))]
    if !file.exists() && !file.with_extension("json.bak").exists() {
        if let Some(legacy) = file.parent().and_then(|dir| dir.parent()).map(|parent| parent.join(LEGACY_IDENTIFIER).join("settings.json")) {
            if legacy.is_file() { let _ = std::fs::copy(&legacy, &file); }
        }
    }
    let _ = file;
    load_settings(app.clone())
}

pub(crate) fn initialize(app: AppHandle) -> Result<(), String> {
    initialize_with(&app.state::<Startup>(), || prepare_initial_settings(&app), |sources| {
        crate::credentials::configure_sources(&app, sources, false);
        crate::git::configure_sources(redaction_sources(sources));
    })
}

pub(crate) fn initialization_failed(app: &AppHandle, error: String) {
    app.state::<Startup>().record(error);
    crate::credentials::configure_sources(app, &[], false);
    crate::git::configure_sources(Vec::new());
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Loaded {
    #[serde(flatten)]
    settings: Settings,
    restored_from_backup: bool,
}

pub mod commands {
    use super::*;

    #[tauri::command]
    pub async fn load_settings(app: AppHandle) -> Result<Loaded, String> {
        tauri::async_runtime::spawn_blocking(move || load_with_status(&app)).await
            .map_err(|_| "Could not load settings".to_string())?
    }

    #[tauri::command]
    pub async fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
        tauri::async_runtime::spawn_blocking(move || super::save_settings(app, settings)).await
            .map_err(|_| "Could not save settings".to_string())?
    }
}

pub fn get_token(source_id: &str) -> Result<Option<String>, String> {
    crate::credentials::get_token(source_id)
}

#[tauri::command]
pub async fn set_token(app: AppHandle, source_id: String, token: String, host: Option<String>) -> Result<(), String> {
    crate::credentials::set_token(app, source_id, token, host).await
}

#[tauri::command]
pub async fn has_token(source_id: String) -> Result<bool, String> {
    crate::credentials::read(source_id).await.map(|token| token.is_some())
}

#[tauri::command]
pub async fn delete_token(app: AppHandle, source_id: String) -> Result<(), String> {
    crate::credentials::delete_token(app, source_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redaction_owners_include_managed_manual_sources() {
        let sources: Vec<Source> = serde_json::from_value(serde_json::json!([
            {"id":"plain-manual","name":"Manual","kind":"manual"},
            {"id":"managed-manual","name":"Managed","kind":"manual","credentialManaged":true},
            {"id":"github","name":"GitHub","kind":"github"},
            {"id":"enterprise","name":"Enterprise","kind":"ghe"}
        ])).unwrap();
        assert_eq!(redaction_sources(&sources), ["managed-manual", "github", "enterprise"]);
        let saved = serde_json::to_value(&sources).unwrap();
        assert!(saved[0].get("credentialManaged").is_none());
        assert_eq!(saved[1]["credentialManaged"], true);
    }
}

#[cfg(test)]
mod startup_tests;
