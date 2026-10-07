use super::{process::ProcessRecord, round};
use serde::Serialize;
use std::collections::HashSet;

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSample {
    pub t: u64,
    pub skein: ProcessMetrics,
    pub webview2: GroupMetrics,
    pub git: GroupMetrics,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMetrics {
    pub working_set_bytes: u64,
    pub private_bytes: u64,
    pub cpu_percent: f64,
    pub handle_count: Option<u64>,
    pub thread_count: u64,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMetrics {
    pub count: u64,
    pub working_set_bytes: u64,
    pub cpu_percent: f64,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePeaks {
    pub skein: ProcessMetricsPeak,
    pub webview2: GroupMetricsPeak,
    pub git: GroupMetricsPeak,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMetricsPeak {
    pub working_set_bytes: u64,
    pub private_bytes: u64,
    pub cpu_percent: f64,
    pub handle_count: u64,
    pub thread_count: u64,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMetricsPeak {
    pub count: u64,
    pub working_set_bytes: u64,
    pub cpu_percent: f64,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSnapshot {
    pub samples: Vec<ResourceSample>,
    pub peaks: ResourcePeaks,
}

impl ResourceSnapshot {
    pub fn rounded(mut self) -> Self {
        for sample in &mut self.samples {
            round_process(&mut sample.skein);
            round_group(&mut sample.webview2);
            round_group(&mut sample.git);
        }
        self.peaks.skein.working_set_bytes = round::count(self.peaks.skein.working_set_bytes);
        self.peaks.skein.private_bytes = round::count(self.peaks.skein.private_bytes);
        self.peaks.skein.handle_count = round::count(self.peaks.skein.handle_count);
        self.peaks.skein.thread_count = round::count(self.peaks.skein.thread_count);
        self.peaks.webview2.count = round::count(self.peaks.webview2.count);
        self.peaks.webview2.working_set_bytes = round::count(self.peaks.webview2.working_set_bytes);
        self.peaks.git.count = round::count(self.peaks.git.count);
        self.peaks.git.working_set_bytes = round::count(self.peaks.git.working_set_bytes);
        self
    }
}

fn round_process(value: &mut ProcessMetrics) {
    value.working_set_bytes = round::count(value.working_set_bytes);
    value.private_bytes = round::count(value.private_bytes);
    value.handle_count = value.handle_count.map(round::count);
    value.thread_count = round::count(value.thread_count);
}

fn round_group(value: &mut GroupMetrics) {
    value.count = round::count(value.count);
    value.working_set_bytes = round::count(value.working_set_bytes);
}

pub(super) fn make_sample(t: u64, records: &[ProcessRecord], self_pid: u32) -> ResourceSample {
    let descendants = descendant_pids(records, &[self_pid]);
    let webview: Vec<_> = records
        .iter()
        .filter(|record| descendants.contains(&record.pid) && is_webview(&record.name))
        .collect();
    let git_roots: Vec<_> = records
        .iter()
        .filter(|record| descendants.contains(&record.pid) && is_git(&record.name))
        .map(|record| record.pid)
        .collect();
    let mut git_pids = descendant_pids(records, &git_roots);
    git_pids.extend(git_roots);
    let current = records.iter().find(|record| record.pid == self_pid);
    ResourceSample {
        t,
        skein: current.map_or_else(ProcessMetrics::default, process_metrics),
        webview2: group_metrics(webview),
        git: group_metrics(
            records
                .iter()
                .filter(|record| git_pids.contains(&record.pid))
                .collect(),
        ),
    }
}

pub(super) fn descendant_pids(records: &[ProcessRecord], roots: &[u32]) -> HashSet<u32> {
    let mut result = HashSet::new();
    let mut frontier = roots.to_vec();
    while let Some(parent) = frontier.pop() {
        for record in records.iter().filter(|record| record.parent == parent) {
            if result.insert(record.pid) {
                frontier.push(record.pid);
            }
        }
    }
    result
}

fn process_metrics(record: &ProcessRecord) -> ProcessMetrics {
    ProcessMetrics {
        working_set_bytes: record.working_set_bytes,
        private_bytes: record.private_bytes,
        cpu_percent: record.cpu_percent,
        handle_count: record.handle_count,
        thread_count: record.thread_count,
    }
}

impl Default for ProcessMetrics {
    fn default() -> Self {
        Self {
            working_set_bytes: 0,
            private_bytes: 0,
            cpu_percent: 0.0,
            handle_count: None,
            thread_count: 0,
        }
    }
}

fn group_metrics(records: Vec<&ProcessRecord>) -> GroupMetrics {
    GroupMetrics {
        count: records.len() as u64,
        working_set_bytes: records.iter().map(|record| record.working_set_bytes).sum(),
        cpu_percent: records.iter().map(|record| record.cpu_percent).sum(),
    }
}

pub(super) fn update_peaks(peaks: &mut ResourcePeaks, sample: &ResourceSample) {
    peaks.skein.working_set_bytes = peaks
        .skein
        .working_set_bytes
        .max(sample.skein.working_set_bytes);
    peaks.skein.private_bytes = peaks.skein.private_bytes.max(sample.skein.private_bytes);
    peaks.skein.cpu_percent = peaks.skein.cpu_percent.max(sample.skein.cpu_percent);
    peaks.skein.handle_count = peaks
        .skein
        .handle_count
        .max(sample.skein.handle_count.unwrap_or_default());
    peaks.skein.thread_count = peaks.skein.thread_count.max(sample.skein.thread_count);
    update_group_peak(&mut peaks.webview2, &sample.webview2);
    update_group_peak(&mut peaks.git, &sample.git);
}

fn update_group_peak(peak: &mut GroupMetricsPeak, sample: &GroupMetrics) {
    peak.count = peak.count.max(sample.count);
    peak.working_set_bytes = peak.working_set_bytes.max(sample.working_set_bytes);
    peak.cpu_percent = peak.cpu_percent.max(sample.cpu_percent);
}

fn is_webview(name: &str) -> bool {
    #[cfg(windows)]
    {
        name.eq_ignore_ascii_case("msedgewebview2.exe")
    }
    #[cfg(target_os = "linux")]
    {
        let _ = name;
        false
    }
}

fn is_git(name: &str) -> bool {
    #[cfg(windows)]
    {
        name.eq_ignore_ascii_case("git.exe")
    }
    #[cfg(target_os = "linux")]
    {
        name == "git" || name.starts_with("git-")
    }
}
