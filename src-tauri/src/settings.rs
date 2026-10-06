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
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join("settings.json");
    #[cfg(feature = "test-profile")]
    crate::test_profile::plain_file(&file)?;
    #[cfg(not(feature = "test-profile"))]
    if !file.exists() {
        if let Some(legacy) = dir.parent().map(|parent| parent.join(LEGACY_IDENTIFIER).join("settings.json")) {
            if legacy.is_file() { let _ = std::fs::copy(&legacy, &file); }
        }
    }
    Ok(file)
}

#[tauri::command]
pub fn load_settings(app: AppHandle) -> Result<Settings, String> {
    let file = settings_file(&app)?;
    if !file.exists() {
        #[cfg(feature = "test-profile")]
        return Err("Test profile settings must be prepared before launch".into());
        #[cfg(not(feature = "test-profile"))]
        return Ok(Settings::default());
    }
    let text = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
    let settings: Settings = serde_json::from_str(&text).map_err(|e| format!("{} is invalid: {e}", file.display()))?;
    #[cfg(feature = "test-profile")]
    crate::test_profile::settings(&settings)?;
    for source in &settings.sources { valid_id(&source.id)?; }
    crate::credentials::configure_sources(&app, &settings.sources, false);
    crate::git::configure_sources(redaction_sources(&settings.sources));
    Ok(settings)
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let _filesystem = crate::git::filesystem_gate().try_read().map_err(|_| "A recoverable write is in progress; retry saving settings")?;
    #[cfg(feature = "test-profile")]
    crate::test_profile::settings(&settings)?;
    for source in &settings.sources { valid_id(&source.id)?; }
    let file = settings_file(&app)?;
    let tmp = file.with_extension("json.tmp");
    #[cfg(feature = "test-profile")]
    crate::test_profile::plain_file(&tmp)?;
    let text = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &file).map_err(|e| e.to_string())?;
    crate::credentials::configure_sources(&app, &settings.sources, true);
    crate::git::configure_sources(redaction_sources(&settings.sources));
    Ok(())
}

pub fn get_token(source_id: &str) -> Result<Option<String>, String> {
    crate::credentials::get_token(source_id)
}

#[tauri::command]
pub async fn set_token(app: AppHandle, source_id: String, token: String) -> Result<(), String> {
    crate::credentials::set_token(app, source_id, token).await
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
