use super::timings::{Aggregate, TimingEvent, Timings};
use super::{leak, process, resources, round, scale};
use serde::Serialize;
use std::collections::BTreeMap;

pub(super) const OS_FAMILIES: &[&str] = &["windows", "linux"];
pub(super) const DRIVE_TYPES: &[&str] = &["ssd", "hdd", "unknown"];

#[derive(Default, Serialize)]
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
    resources: resources::ResourceSnapshot,
    scale: scale::ScaleData,
}

pub(super) fn serialize(
    scale: scale::ScaleData,
    timings: Timings,
    resources: resources::ResourceSnapshot,
    hardware: process::MachineHardware,
    git_version: String,
    session_uptime_ms: u64,
    denylist: &leak::Denylist,
) -> Result<String, &'static str> {
    let document = Document {
        version: 1,
        machine: Machine {
            os_family: if cfg!(windows) {
                OS_FAMILIES[0]
            } else {
                OS_FAMILIES[1]
            },
            windows_build: hardware.windows_build,
            logical_cores: round::count(hardware.logical_cores),
            total_ram_bytes: round::count(hardware.total_ram_bytes),
            system_drive_type: DRIVE_TYPES
                .iter()
                .copied()
                .find(|known| *known == hardware.system_drive_type)
                .unwrap_or(DRIVE_TYPES[2]),
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

pub(super) fn schema() -> serde_json::Value {
    let document = Document {
        version: 1,
        machine: Machine {
            skein_version: env!("CARGO_PKG_VERSION"),
            ..Machine::default()
        },
        timings: Timings {
            aggregates: BTreeMap::from([(String::new(), Aggregate::default())]),
            events: vec![TimingEvent::default()],
            ..Timings::default()
        },
        resources: resources::ResourceSnapshot {
            samples: vec![resources::ResourceSample::default()],
            ..resources::ResourceSnapshot::default()
        },
        scale: scale::ScaleData {
            repos: vec![scale::ScaleRepo::default()],
            ..scale::ScaleData::default()
        },
    };
    serde_json::to_value(document).expect("Diagnostics schema contains only serializable values")
}
