mod leak;
#[cfg(target_os = "linux")]
mod process_linux;
#[cfg(windows)]
mod process_windows;
mod round;
mod sampler;
mod scale;
#[cfg(test)]
mod tests;

use crate::{benchmark, git};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

const PHASES: &[&str] = &[
    "git.queue",
    "git.process",
    "compare.queue",
    "compare.prepare",
    "compare.inventory",
    "compare.metadata",
    "compare.history",
    "ipc.open",
    "ipc.refresh",
    "ipc.files",
    "ipc.content",
    "ui.request",
    "ui.files-ready",
    "ui.first-render",
    "ui.complete",
    "editor.import",
    "editor.construct",
    "editor.diff",
];
const OPERATIONS: &[&str] = &[
    "other",
    "version",
    "rev-parse",
    "status",
    "ls-files",
    "ls-tree",
    "cat-file",
    "diff",
    "log",
    "show",
    "fetch",
    "clone",
    "checkout",
    "switch",
    "pull",
    "push",
    "add",
    "reset",
    "commit",
    "branch",
    "config",
    "for-each-ref",
    "symbolic-ref",
    "rev-list",
    "remote",
    "stash",
    "check-ignore",
    "check-attr",
];
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Machine {
    os_family: &'static str,
    windows_build: Option<u64>,
    logical_cores: u64,
    total_ram_bytes: u64,
    system_drive_type: &'static str,
    git_version: String,
    skein_version: &'static str,
    session_uptime_ms: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Document {
    version: u8,
    machine: Machine,
    timings: Timings,
    resources: sampler::ResourceSnapshot,
    scale: scale::ScaleData,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Timings {
    aggregates: BTreeMap<String, Aggregate>,
    commands: BTreeMap<String, u64>,
    events: Vec<TimingEvent>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Aggregate {
    count: u64,
    total_ms: f64,
    max_ms: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TimingEvent {
    phase: &'static str,
    operation: &'static str,
    duration_ms: f64,
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
        sampler::hardware(),
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

fn document_json(
    scale: scale::ScaleData,
    timings: Timings,
    resources: sampler::ResourceSnapshot,
    hardware: sampler::MachineHardware,
    git_version: String,
    session_uptime_ms: u64,
    denylist: &leak::Denylist,
) -> Result<String, &'static str> {
    let document = Document {
        version: 1,
        machine: Machine {
            os_family: if cfg!(windows) { "windows" } else { "linux" },
            windows_build: hardware.windows_build,
            logical_cores: round::count(hardware.logical_cores),
            total_ram_bytes: round::count(hardware.total_ram_bytes),
            system_drive_type: match hardware.system_drive_type {
                "ssd" => "ssd",
                "hdd" => "hdd",
                _ => "unknown",
            },
            git_version,
            skein_version: env!("CARGO_PKG_VERSION"),
            session_uptime_ms,
        },
        timings,
        resources,
        scale,
    };
    leak::serialize_checked(&document, denylist)
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

fn timings(snapshot: &serde_json::Value) -> Result<Timings, &'static str> {
    let aggregates = snapshot
        .get("aggregates")
        .and_then(serde_json::Value::as_object)
        .ok_or(PREVIEW_ERROR)?;
    let mut checked_aggregates = BTreeMap::new();
    for (key, value) in aggregates {
        let (phase, operation) = key.split_once('/').ok_or(PREVIEW_ERROR)?;
        if !PHASES.contains(&phase) || !OPERATIONS.contains(&operation) {
            return Err(PREVIEW_ERROR);
        }
        let count = value
            .get("count")
            .and_then(serde_json::Value::as_u64)
            .ok_or(PREVIEW_ERROR)?;
        let total_ms = finite(
            value
                .get("totalMs")
                .and_then(serde_json::Value::as_f64)
                .ok_or(PREVIEW_ERROR)?,
        )?;
        let max_ms = finite(
            value
                .get("maxMs")
                .and_then(serde_json::Value::as_f64)
                .ok_or(PREVIEW_ERROR)?,
        )?;
        checked_aggregates.insert(
            format!("{phase}/{operation}"),
            Aggregate {
                count: round::count(count),
                total_ms,
                max_ms,
            },
        );
    }
    let commands = snapshot
        .get("commands")
        .and_then(serde_json::Value::as_object)
        .ok_or(PREVIEW_ERROR)?;
    let mut checked_commands = BTreeMap::new();
    for (operation, value) in commands {
        if !OPERATIONS.contains(&operation.as_str()) {
            return Err(PREVIEW_ERROR);
        }
        checked_commands.insert(
            operation.clone(),
            round::count(value.as_u64().ok_or(PREVIEW_ERROR)?),
        );
    }
    let events = snapshot
        .get("events")
        .and_then(serde_json::Value::as_array)
        .ok_or(PREVIEW_ERROR)?;
    let mut checked_events = Vec::with_capacity(events.len());
    for event in events {
        let phase = event
            .get("phase")
            .and_then(serde_json::Value::as_str)
            .ok_or(PREVIEW_ERROR)?;
        let operation = event
            .get("operation")
            .and_then(serde_json::Value::as_str)
            .ok_or(PREVIEW_ERROR)?;
        if !PHASES.contains(&phase) || !OPERATIONS.contains(&operation) {
            return Err(PREVIEW_ERROR);
        }
        let duration_ms = finite(
            event
                .get("durationMs")
                .and_then(serde_json::Value::as_f64)
                .ok_or(PREVIEW_ERROR)?,
        )?;
        checked_events.push(TimingEvent {
            phase: PHASES
                .iter()
                .find(|known| **known == phase)
                .ok_or(PREVIEW_ERROR)?,
            operation: OPERATIONS
                .iter()
                .find(|known| **known == operation)
                .ok_or(PREVIEW_ERROR)?,
            duration_ms,
        });
    }
    Ok(Timings {
        aggregates: checked_aggregates,
        commands: checked_commands,
        events: checked_events,
    })
}

fn finite(value: f64) -> Result<f64, &'static str> {
    (value.is_finite() && value >= 0.0)
        .then_some(value)
        .ok_or(PREVIEW_ERROR)
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
