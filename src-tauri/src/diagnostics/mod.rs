mod document;
mod leak;
mod process;
#[cfg(target_os = "linux")]
mod process_linux;
#[cfg(windows)]
mod process_windows;
mod repository_paths;
mod resources;
mod round;
mod sampler;
mod scale;
#[cfg(test)]
mod tests;
mod timings;
mod vocabulary;

use crate::{benchmark, git};
use document::serialize as document_json;
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;
use timings::collect as timings;

const SETTINGS_ERROR: &str = "Diagnostics settings could not be read";
const PREVIEW_ERROR: &str = "Diagnostics preview could not be created";

pub struct Service {
    sampler: sampler::Sampler,
    cancellation: Arc<AtomicBool>,
    collecting: AtomicBool,
    preview: Mutex<Option<String>>,
}

impl Service {
    pub fn start() -> Result<Self, std::io::Error> {
        let sampler = sampler::Sampler::start().map_err(std::io::Error::other)?;
        Ok(Self {
            sampler,
            cancellation: Arc::new(AtomicBool::new(false)),
            collecting: AtomicBool::new(false),
            preview: Mutex::new(None),
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    sampling: bool,
    samples: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    phase: &'static str,
    completed: u64,
    total: u64,
}

struct CollectionGuard<'a>(&'a AtomicBool);

impl Drop for CollectionGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

#[tauri::command]
pub async fn diagnostics_status(service: State<'_, Service>) -> Result<Status, String> {
    let samples = service.sampler.snapshot().samples.len() as u64;
    Ok(Status {
        sampling: true,
        samples,
    })
}

#[tauri::command]
pub async fn diagnostics_preview(
    app: AppHandle,
    service: State<'_, Service>,
) -> Result<String, String> {
    service
        .collecting
        .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
        .map_err(|_| "Diagnostics collection is already running".to_string())?;
    let _guard = CollectionGuard(&service.collecting);
    service.cancellation.store(false, Ordering::Relaxed);
    *service
        .preview
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    let settings = read_settings(app.clone()).await?;
    let paths = scale::registered_paths(&settings);
    let denylist = leak::Denylist::from_settings(&settings, &paths);
    let progress_app = app.clone();
    let progress = Arc::new(move |completed, total| {
        let _ = progress_app.emit(
            "diagnostics-progress",
            Progress {
                phase: "scale",
                completed,
                total,
            },
        );
    });
    let scale = scale::collect(&settings, service.cancellation.clone(), progress)
        .await
        .map_err(str::to_string)?;
    if service.cancellation.load(Ordering::Relaxed) {
        return Err("Diagnostics collection cancelled".into());
    }
    let git_version = git_version().await.map_err(str::to_string)?;
    let snapshot = benchmark::benchmark_snapshot().map_err(|_| PREVIEW_ERROR)?;
    let timings = timings(&snapshot).map_err(str::to_string)?;
    if service.cancellation.load(Ordering::Relaxed) {
        return Err("Diagnostics collection cancelled".into());
    }
    let preview = document_json(
        scale,
        timings,
        service.sampler.snapshot().rounded(),
        process::hardware(),
        git_version,
        service.sampler.uptime_ms(),
        &denylist,
    )
    .map_err(str::to_string)?;
    *service
        .preview
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(preview.clone());
    Ok(preview)
}

#[tauri::command]
pub async fn diagnostics_cancel(service: State<'_, Service>) -> Result<bool, String> {
    service.cancellation.store(true, Ordering::Relaxed);
    *service
        .preview
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    Ok(service.collecting.load(Ordering::Relaxed))
}

#[tauri::command]
pub async fn diagnostics_export(
    app: AppHandle,
    service: State<'_, Service>,
) -> Result<bool, String> {
    let preview = service
        .preview
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .ok_or_else(|| "Generate a diagnostics preview before saving".to_string())?;
    let settings = read_settings(app.clone()).await?;
    let denylist = leak::Denylist::from_settings(&settings, &scale::registered_paths(&settings));
    leak::serialize_checked(
        &serde_json::from_str::<serde_json::Value>(&preview).map_err(|_| PREVIEW_ERROR)?,
        &denylist,
    )
    .map_err(str::to_string)?;
    let name = format!("skein-diagnostics-{}.json", timestamp());
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_file_name(name)
        .add_filter("JSON", &["json"])
        .save_file(move |path| {
            let _ = sender.send(path);
        });
    let selected = receiver
        .await
        .map_err(|_| "Diagnostics save dialog failed".to_string())?;
    let Some(selected) = selected else {
        return Ok(false);
    };
    let path = selected
        .into_path()
        .map_err(|_| "Diagnostics save location is invalid".to_string())?;
    tauri::async_runtime::spawn_blocking(move || std::fs::write(path, preview))
        .await
        .map_err(|_| "Diagnostics file could not be written".to_string())?
        .map_err(|_| "Diagnostics file could not be written".to_string())?;
    Ok(true)
}

async fn read_settings(app: AppHandle) -> Result<crate::settings::Settings, String> {
    tauri::async_runtime::spawn_blocking(move || crate::settings::load_settings(app))
        .await
        .map_err(|_| SETTINGS_ERROR.to_string())?
        .map_err(|_| SETTINGS_ERROR.to_string())
}

async fn git_version() -> Result<String, &'static str> {
    let request = git::Request {
        args: &["--version"],
        context: "diagnostics-version",
        timeout: Duration::from_secs(10),
        expected: &[0],
        policy: git::OutputPolicy::Metadata,
    };
    let captured = git::execute(request, None)
        .await
        .map_err(|_| PREVIEW_ERROR)?;
    if captured.code != Some(0) {
        return Err(PREVIEW_ERROR);
    }
    let text = String::from_utf8_lossy(&captured.stdout);
    let version = text.split_whitespace().find_map(|word| {
        let value = word.trim_start_matches(|character: char| !character.is_ascii_digit());
        let end = value
            .find(|character: char| !character.is_ascii_digit() && character != '.')
            .unwrap_or(value.len());
        let version = value.get(..end)?.trim_end_matches('.');
        (!version.is_empty()
            && version
                .chars()
                .all(|character| character.is_ascii_digit() || character == '.'))
        .then(|| version.to_string())
    });
    version.ok_or(PREVIEW_ERROR)
}

fn timestamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (seconds / 86_400) as i64;
    let within_day = seconds % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let hour = within_day / 3_600;
    let minute = within_day % 3_600 / 60;
    let second = within_day % 60;
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}
