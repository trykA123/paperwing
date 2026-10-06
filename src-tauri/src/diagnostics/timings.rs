use super::round;
use super::PREVIEW_ERROR;
use serde::Serialize;
use std::collections::BTreeMap;

pub(super) const PHASES: &[&str] = &[
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
pub(super) const OPERATIONS: &[&str] = &[
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
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Timings {
    pub(super) aggregates: BTreeMap<String, Aggregate>,
    pub(super) commands: BTreeMap<String, u64>,
    pub(super) events: Vec<TimingEvent>,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Aggregate {
    count: u64,
    total_ms: f64,
    max_ms: f64,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TimingEvent {
    phase: &'static str,
    operation: &'static str,
    duration_ms: f64,
}

pub(super) fn collect(snapshot: &serde_json::Value) -> Result<Timings, &'static str> {
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
