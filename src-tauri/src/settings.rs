use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const KEYRING_SERVICE: &str = "paperwing";
/// Names from before the rename; read so existing settings and tokens carry over.
const LEGACY_KEYRING_SERVICE: &str = "flock";
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
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join("settings.json");
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
        return Ok(Settings::default());
    }
    let text = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
    let settings: Settings = serde_json::from_str(&text).map_err(|e| format!("{} is invalid: {e}", file.display()))?;
    crate::git::configure_sources(settings.sources.iter().map(|source| source.id.clone()).collect());
    Ok(settings)
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let _filesystem = crate::git::filesystem_gate().try_read().map_err(|_| "A recoverable write is in progress; retry saving settings")?;
    crate::git::configure_sources(settings.sources.iter().map(|source| source.id.clone()).collect());
    let file = settings_file(&app)?;
    let tmp = file.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &file).map_err(|e| e.to_string())
}

fn entry(source_id: &str) -> Result<keyring::Entry, String> {
    valid_id(source_id)?;
    keyring::Entry::new(KEYRING_SERVICE, source_id).map_err(|e| e.to_string())
}

pub fn get_token(source_id: &str) -> Option<String> {
    if let Some(token) = entry(source_id).ok().and_then(|current| current.get_password().ok()) { return Some(token); }
    valid_id(source_id).ok()?;
    let token = keyring::Entry::new(LEGACY_KEYRING_SERVICE, source_id).ok()?.get_password().ok()?;
    if let Ok(current) = entry(source_id) { let _ = current.set_password(&token); }
    Some(token)
}

#[tauri::command]
pub fn set_token(source_id: String, token: String) -> Result<(), String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("Token is empty".into());
    }
    entry(&source_id)?.set_password(token).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn has_token(source_id: String) -> bool {
    get_token(&source_id).is_some()
}

#[tauri::command]
pub fn delete_token(source_id: String) -> Result<(), String> {
    let current = entry(&source_id)?;
    if let Ok(legacy) = keyring::Entry::new(LEGACY_KEYRING_SERVICE, &source_id) { let _ = legacy.delete_credential(); }
    match current.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
