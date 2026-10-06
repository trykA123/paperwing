use crate::diagnostics::sampler::{
    descendant_pids, MachineHardware, ProcessHistory, ProcessRecord,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::Instant;

pub(super) fn processes(history: &mut ProcessHistory) -> Vec<ProcessRecord> {
    let now = Instant::now();
    let mut records = Vec::new();
    let mut pending = HashMap::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return records;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        let path = entry.path();
        let Ok(stat) = fs::read_to_string(path.join("stat")) else {
            continue;
        };
        let Some(parsed) = parse_stat(&stat) else {
            continue;
        };
        records.push(ProcessRecord {
            pid,
            parent: parsed.parent,
            name: parsed.name,
            working_set_bytes: 0,
            private_bytes: 0,
            cpu_percent: 0.0,
            handle_count: None,
            thread_count: 0,
        });
        pending.insert(pid, (parsed.cpu_ticks, path));
    }
    let self_pid = std::process::id();
    let descendants = descendant_pids(&records, &[self_pid]);
    records.retain_mut(|record| {
        if record.pid != self_pid && !descendants.contains(&record.pid) {
            return false;
        }
        let Some((cpu_ticks, path)) = pending.remove(&record.pid) else {
            return false;
        };
        let (working_set_bytes, private_bytes, thread_count) = process_memory(&path);
        record.working_set_bytes = working_set_bytes;
        record.private_bytes = private_bytes;
        record.thread_count = thread_count;
        record.cpu_percent = history.cpu_percent(record.pid, cpu_ticks, now);
        history.observe(record.pid, cpu_ticks);
        true
    });
    history.finish(
        records
            .iter()
            .map(|record| record.pid)
            .collect::<HashSet<_>>(),
        now,
    );
    records
}

pub(super) fn hardware() -> MachineHardware {
    MachineHardware {
        logical_cores: logical_cores(),
        total_ram_bytes: total_memory(),
        system_drive_type: drive_type(),
        windows_build: None,
    }
}

fn logical_cores() -> u64 {
    // SAFETY: sysconf reads a process-wide constant and does not dereference pointers.
    let cores = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };
    if cores > 0 {
        cores as u64
    } else {
        1
    }
}

fn parse_stat(stat: &str) -> Option<ParsedStat> {
    let close = stat.rfind(") ")?;
    let name = stat.get(stat.find('(')? + 1..close)?.to_string();
    let fields: Vec<_> = stat.get(close + 2..)?.split_whitespace().collect();
    let parent = fields.get(1)?.parse().ok()?;
    let user_ticks: u64 = fields.get(11)?.parse().ok()?;
    let system_ticks: u64 = fields.get(12)?.parse().ok()?;
    Some(ParsedStat {
        name,
        parent,
        cpu_ticks: user_ticks.saturating_add(system_ticks),
    })
}

struct ParsedStat {
    name: String,
    parent: u32,
    cpu_ticks: u64,
}

fn process_memory(path: &Path) -> (u64, u64, u64) {
    let mut resident_kib: u64 = 0;
    let mut threads: u64 = 0;
    if let Ok(status) = fs::read_to_string(path.join("status")) {
        for line in status.lines() {
            if let Some(value) = line.strip_prefix("VmRSS:") {
                resident_kib = parse_kib(value);
            } else if let Some(value) = line.strip_prefix("Threads:") {
                threads = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let private_kib: u64 = fs::read_to_string(path.join("smaps_rollup"))
        .ok()
        .map(|text| {
            text.lines()
                .filter_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    matches!(key, "Private_Clean" | "Private_Dirty" | "Private_Hugetlb")
                        .then(|| parse_kib(value))
                })
                .sum()
        })
        .unwrap_or_default();
    (
        resident_kib.saturating_mul(1024),
        private_kib.saturating_mul(1024),
        threads,
    )
}

fn parse_kib(value: &str) -> u64 {
    value
        .split_whitespace()
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn total_memory() -> u64 {
    fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                let value = line.strip_prefix("MemTotal:")?;
                Some(parse_kib(value).saturating_mul(1024))
            })
        })
        .unwrap_or_default()
}

fn drive_type() -> &'static str {
    let source = fs::read_to_string("/proc/mounts").ok().and_then(|text| {
        text.lines().find_map(|line| {
            let mut fields = line.split_whitespace();
            let source = fields.next()?;
            (fields.next()? == "/").then(|| source.to_string())
        })
    });
    let Some(device) = source.and_then(|source| {
        Path::new(&source)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    }) else {
        return "unknown";
    };
    let Ok(device_path) = fs::canonicalize(Path::new("/sys/class/block").join(device)) else {
        return "unknown";
    };
    for ancestor in device_path.ancestors() {
        let rotational = ancestor.join("queue/rotational");
        if let Ok(value) = fs::read_to_string(rotational) {
            return match value.trim() {
                "0" => "ssd",
                "1" => "hdd",
                _ => "unknown",
            };
        }
    }
    "unknown"
}

pub(super) fn ticks_per_second() -> f64 {
    // SAFETY: sysconf reads a process-wide constant and does not dereference pointers.
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if ticks > 0 {
        ticks as f64
    } else {
        100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_proc_stat_names_with_spaces_and_cpu_fields() {
        let stat = "42 (git worker) S 9 0 0 0 0 0 0 0 0 0 15 7 0 0";
        let parsed = parse_stat(stat).unwrap();
        assert_eq!(parsed.name, "git worker");
        assert_eq!(parsed.parent, 9);
        assert_eq!(parsed.cpu_ticks, 22);
    }
}
