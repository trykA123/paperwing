use super::process::{processes, ProcessHistory};
use super::resources::{
    make_sample, update_peaks, ResourcePeaks, ResourceSample, ResourceSnapshot,
};
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

const SAMPLE_LIMIT: usize = 3600;

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
        let records = processes(&mut history.process_history);
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
