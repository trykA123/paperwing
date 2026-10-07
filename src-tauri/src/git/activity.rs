use super::{Activity, ActivityOutput};
use crate::kernel::events::CoreEvent;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::AppHandle;

const EVENT_INTERVAL: Duration = Duration::from_millis(100);
const ENTRY_LIMIT: usize = 64;

static ACTIVITY: OnceLock<Mutex<VecDeque<Stored>>> = OnceLock::new();
static APPLICATION: OnceLock<AppHandle> = OnceLock::new();
#[cfg(test)]
pub(super) static TEST_EVENTS: Mutex<Vec<serde_json::Value>> = Mutex::new(Vec::new());

struct Stored {
    activity: Activity,
    sent: usize,
    emitted: Instant,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Delta {
    id: String,
    context: String,
    argv: Vec<String>,
    started_at: u128,
    sequence: u64,
    elapsed_ms: u128,
    state: String,
    exit_code: Option<i32>,
    truncated: bool,
    stdout_bytes: usize,
    stderr_bytes: usize,
    from: usize,
    lines: Vec<ActivityOutput>,
}

pub(super) enum Emit {
    Sent,
    Deferred(Instant),
}

fn entries() -> &'static Mutex<VecDeque<Stored>> {
    ACTIVITY.get_or_init(|| Mutex::new(VecDeque::new()))
}

pub(super) fn attach(app: AppHandle) {
    let _ = APPLICATION.set(app);
}

fn delta(activity: &Activity, from: usize) -> Delta {
    Delta {
        id: activity.id.clone(),
        context: activity.context.clone(),
        argv: activity.argv.clone(),
        started_at: activity.started_at,
        sequence: activity.sequence,
        elapsed_ms: activity.elapsed_ms,
        state: activity.state.clone(),
        exit_code: activity.exit_code,
        truncated: activity.truncated,
        stdout_bytes: activity.stdout_bytes,
        stderr_bytes: activity.stderr_bytes,
        from,
        lines: activity.output[from..].to_vec(),
    }
}

fn emit(delta: &Delta) {
    #[cfg(test)]
    TEST_EVENTS
        .lock()
        .unwrap()
        .push(serde_json::to_value(delta).unwrap());
    if let Some(app) = APPLICATION.get() {
        let _ = crate::events::publish_payload(app, CoreEvent::GitActivity, delta);
    }
}

pub(super) fn publish(activity: &Activity, urgent: bool) -> Emit {
    let now = Instant::now();
    let mut stored = entries().lock().unwrap();
    let outgoing = match stored
        .iter_mut()
        .find(|entry| entry.activity.id == activity.id)
    {
        Some(entry) => {
            let due = entry.emitted + EVENT_INTERVAL;
            if !urgent && now < due {
                return Emit::Deferred(due);
            }
            let outgoing = delta(activity, entry.sent.min(activity.output.len()));
            *entry = Stored {
                activity: activity.clone(),
                sent: activity.output.len(),
                emitted: now,
            };
            outgoing
        }
        None => {
            while stored.len() >= ENTRY_LIMIT {
                let Some(index) = stored
                    .iter()
                    .position(|entry| entry.activity.state != "running")
                else {
                    break;
                };
                stored.remove(index);
            }
            stored.push_back(Stored {
                activity: activity.clone(),
                sent: activity.output.len(),
                emitted: now,
            });
            delta(activity, 0)
        }
    };
    drop(stored);
    emit(&outgoing);
    Emit::Sent
}

pub(super) fn snapshot() -> Vec<Activity> {
    entries()
        .lock()
        .unwrap()
        .iter()
        .map(|entry| entry.activity.clone())
        .collect()
}

pub(super) fn find_running(id: &str) -> Option<Activity> {
    let stored = ACTIVITY.get()?.lock().ok()?;
    stored
        .iter()
        .find(|entry| entry.activity.id == id && entry.activity.state == "running")
        .map(|entry| entry.activity.clone())
}

pub(super) fn retain_active(is_registered: impl Fn(&str) -> bool) -> Vec<Activity> {
    let mut stored = entries().lock().unwrap();
    stored.retain(|entry| entry.activity.state == "running" || is_registered(&entry.activity.id));
    stored.iter().map(|entry| entry.activity.clone()).collect()
}
