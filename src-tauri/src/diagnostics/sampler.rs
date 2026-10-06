use crate::diagnostics::round;
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use crate::diagnostics::process_linux as process_platform;
#[cfg(windows)]
use crate::diagnostics::process_windows as process_platform;

const SAMPLE_LIMIT: usize = 3600;

#[derive(Clone, Serialize)]
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

#[derive(Clone, Serialize)]
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

#[derive(Clone, Serialize)]
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

pub struct MachineHardware {
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

struct History {
    started: Instant,
    process_history: ProcessHistory,
    samples: VecDeque<ResourceSample>,
    peaks: ResourcePeaks,
}

struct Shared {
    history: Mutex<History>,
    changed: Condvar,
    stop: AtomicBool,
}

pub struct Sampler {
    shared: Arc<Shared>,
}

impl Sampler {
    pub fn start() -> Result<Self, &'static str> {
        let shared = Arc::new(Shared {
            history: Mutex::new(History {
                started: Instant::now(),
                process_history: ProcessHistory::default(),
                samples: VecDeque::new(),
                peaks: ResourcePeaks::default(),
            }),
            changed: Condvar::new(),
            stop: AtomicBool::new(false),
        });
        let worker = shared.clone();
        thread::Builder::new()
            .name("diagnostics-sampler".into())
            .spawn(move || sample_loop(worker))
            .map_err(|_| "Diagnostics sampler could not start")?;
        Ok(Self { shared })
    }

    pub fn snapshot(&self) -> ResourceSnapshot {
        let history = self
            .shared
            .history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        ResourceSnapshot {
            samples: history.samples.iter().cloned().collect(),
            peaks: history.peaks.clone(),
        }
    }

    pub fn uptime_ms(&self) -> u64 {
        let history = self
            .shared
            .history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        history
            .started
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64
    }

    #[cfg(all(test, target_os = "linux"))]
    pub(super) fn wait_for(
        &self,
        timeout: Duration,
        predicate: impl Fn(&ResourceSnapshot) -> bool,
    ) -> bool {
        let history = self
            .shared
            .history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let result = self
            .shared
            .changed
            .wait_timeout_while(history, timeout, |history| {
                let snapshot = ResourceSnapshot {
                    samples: history.samples.iter().cloned().collect(),
                    peaks: history.peaks.clone(),
                };
                !predicate(&snapshot)
            });
        result
            .map(|(history, _)| {
                let snapshot = ResourceSnapshot {
                    samples: history.samples.iter().cloned().collect(),
                    peaks: history.peaks.clone(),
                };
                predicate(&snapshot)
            })
            .unwrap_or(false)
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

fn sample_loop(shared: Arc<Shared>) {
    let mut next_sample = Instant::now();
    while !shared.stop.load(Ordering::Relaxed) {
        let mut history = shared
            .history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let records = process_platform::processes(&mut history.process_history);
        let elapsed = history
            .started
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        let sample = make_sample(elapsed, &records, std::process::id());
        update_peaks(&mut history.peaks, &sample);
        if history.samples.len() == SAMPLE_LIMIT {
            history.samples.pop_front();
        }
        history.samples.push_back(sample);
        shared.changed.notify_all();
        drop(history);
        next_sample += Duration::from_secs(1);
        let now = Instant::now();
        if next_sample < now {
            next_sample = now + Duration::from_secs(1);
        }
        thread::park_timeout(next_sample.saturating_duration_since(Instant::now()));
    }
}

fn make_sample(t: u64, records: &[ProcessRecord], self_pid: u32) -> ResourceSample {
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

fn update_peaks(peaks: &mut ResourcePeaks, sample: &ResourceSample) {
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

#[cfg(target_os = "linux")]
pub fn hardware() -> MachineHardware {
    process_platform::hardware()
}

#[cfg(windows)]
pub fn hardware() -> MachineHardware {
    process_platform::hardware()
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn linux_process_sampling_tracks_a_child_git_process() {
        let fixture = crate::test_support::tmp_root()
            .join(format!("diagnostics-sampler-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&fixture);
        std::fs::create_dir_all(&fixture).unwrap();
        let sleep = std::env::var_os("PATH")
            .and_then(|path| {
                std::env::split_paths(&path)
                    .map(|dir| dir.join("sleep"))
                    .find(|path| path.is_file())
            })
            .unwrap();
        std::os::unix::fs::symlink(sleep, fixture.join("git")).unwrap();
        let sampler = Sampler::start().unwrap();
        let mut child = std::process::Command::new(fixture.join("git"))
            .arg("30")
            .spawn()
            .unwrap();
        let saw_child = sampler.wait_for(Duration::from_secs(4), |snapshot| {
            snapshot
                .samples
                .last()
                .is_some_and(|sample| sample.git.count > 0)
        });
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(saw_child);
        assert!(sampler.wait_for(Duration::from_secs(4), |snapshot| snapshot
            .samples
            .last()
            .is_some_and(|sample| sample.git.count == 0)));
        let _ = std::fs::remove_dir_all(fixture);
    }
}
