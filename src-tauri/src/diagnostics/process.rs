use std::collections::{HashMap, HashSet};
use std::time::Instant;

#[cfg(target_os = "linux")]
use crate::diagnostics::process_linux as process_platform;
#[cfg(windows)]
use crate::diagnostics::process_windows as process_platform;

pub(super) struct MachineHardware {
    pub logical_cores: u64,
    pub total_ram_bytes: u64,
    pub system_drive_type: &'static str,
    pub windows_build: Option<u64>,
}

pub(super) struct ProcessRecord {
    pub pid: u32,
    pub parent: u32,
    pub name: String,
    pub working_set_bytes: u64,
    pub private_bytes: u64,
    pub cpu_percent: f64,
    pub handle_count: Option<u64>,
    pub thread_count: u64,
}

#[derive(Default)]
pub(super) struct ProcessHistory {
    previous: HashMap<u32, u64>,
    observed_at: Option<Instant>,
    current: HashMap<u32, u64>,
}

impl ProcessHistory {
    pub(super) fn cpu_percent(&self, pid: u32, cumulative: u64, now: Instant) -> f64 {
        let (Some(previous), Some(observed_at)) = (self.previous.get(&pid), self.observed_at)
        else {
            return 0.0;
        };
        let elapsed = now.duration_since(observed_at).as_secs_f64();
        if elapsed <= 0.0 {
            return 0.0;
        }
        let ticks_per_second = process_platform::ticks_per_second();
        cumulative.saturating_sub(*previous) as f64 / ticks_per_second * 100.0 / elapsed
    }

    pub(super) fn observe(&mut self, pid: u32, cumulative: u64) {
        self.current.insert(pid, cumulative);
    }

    pub(super) fn finish(&mut self, live: HashSet<u32>, now: Instant) {
        self.current.retain(|pid, _| live.contains(pid));
        self.previous = std::mem::take(&mut self.current);
        self.observed_at = Some(now);
    }
}

#[cfg(target_os = "linux")]
pub fn hardware() -> MachineHardware {
    process_platform::hardware()
}

#[cfg(windows)]
pub fn hardware() -> MachineHardware {
    process_platform::hardware()
}

pub(super) fn processes(history: &mut ProcessHistory) -> Vec<ProcessRecord> {
    process_platform::processes(history)
}
