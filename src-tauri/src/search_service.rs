use crate::kernel::events::CoreEvent;
use crate::search::{plan, Mode, SearchRequest};
use crate::search_grep::perl_supported;
use crate::search_job::{run_with_engine, Capabilities, Emit, Job, Outbound};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State as TauriState};

const CONCURRENCY: usize = 4;
const MAX_RUNNING: usize = 4;

#[derive(Default)]
pub struct Service {
    next: AtomicU64,
    active: Arc<Mutex<HashMap<u64, Arc<AtomicBool>>>>,
}

impl Service {
    fn register(&self) -> Result<(u64, Arc<AtomicBool>), String> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| "Search registry is unavailable")?;
        if active.len() >= MAX_RUNNING {
            return Err("Too many searches are running; cancel one first".into());
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let flag = Arc::new(AtomicBool::new(false));
        active.insert(id, flag.clone());
        Ok((id, flag))
    }

    fn cancel_all(&self) -> usize {
        let active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        active
            .values()
            .for_each(|flag| flag.store(true, Ordering::Relaxed));
        active.len()
    }

    fn cancel(&self, id: u64) -> bool {
        let flag = self
            .active
            .lock()
            .ok()
            .and_then(|active| active.get(&id).cloned());
        flag.is_some_and(|flag| {
            flag.store(true, Ordering::Relaxed);
            true
        })
    }
}

struct Slot {
    active: Arc<Mutex<HashMap<u64, Arc<AtomicBool>>>>,
    id: u64,
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        active.remove(&self.id);
    }
}

fn emit_outbound<R: tauri::Runtime>(app: &AppHandle<R>, cancel: &AtomicBool, outbound: Outbound) {
    let sent = match outbound {
        Outbound::Matches(payload) => {
            crate::events::publish_payload(app, CoreEvent::SearchMatches, &payload)
        }
        Outbound::Repo(payload) => {
            crate::events::publish_payload(app, CoreEvent::SearchRepo, &payload)
        }
        Outbound::Done(payload) => {
            crate::events::publish_payload(app, CoreEvent::SearchDone, &payload)
        }
    };
    if sent.is_err() {
        cancel.store(true, Ordering::Relaxed);
    }
}

pub(crate) fn configure_request(
    request: &mut SearchRequest,
    options: crate::settings::SearchOptions,
) -> crate::search_engine::Engine {
    request.untracked |= options.search_files == crate::settings::SearchFiles::TrackedAndUntracked;
    options.search_engine.into()
}

#[tauri::command]
pub async fn search_capabilities() -> Capabilities {
    Capabilities {
        perl: perl_supported().await,
    }
}

#[tauri::command]
pub async fn search_start(
    app: AppHandle,
    service: TauriState<'_, Service>,
    mut request: SearchRequest,
) -> Result<u64, String> {
    let options = crate::settings::search_options(&app).await?;
    let engine = configure_request(&mut request, options);
    plan(&request)?;
    if request.mode == Mode::Perl && !perl_supported().await {
        return Err("This Git build has no Perl-compatible regex support".into());
    }
    let (id, cancel) = service.register()?;
    let slot = Slot {
        active: service.active.clone(),
        id,
    };
    let stop = cancel.clone();
    let send: Emit = Arc::new(move |outbound| {
        emit_outbound(&app, &stop, outbound);
    });
    tauri::async_runtime::spawn(async move {
        let _slot = slot;
        let job = Job {
            id,
            concurrency: CONCURRENCY,
            cancel,
            send,
        };
        if let Err(error) = run_with_engine(request, job, engine).await {
            eprintln!("search {id} failed: {error}");
        }
    });
    Ok(id)
}

#[tauri::command]
pub async fn search_cancel(service: TauriState<'_, Service>, id: u64) -> Result<bool, String> {
    Ok(service.cancel(id))
}

#[tauri::command]
pub async fn search_cancel_all(service: TauriState<'_, Service>) -> Result<usize, String> {
    Ok(service.cancel_all())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn a_frontend_emit_failure_cancels_the_search() {
        use tauri::test::{mock_builder, mock_context, noop_assets};
        let app = mock_builder()
            .manage(crate::events::EmitFailure)
            .build(mock_context(noop_assets()))
            .unwrap();
        let bus = crate::kernel::events::EventBus::default();
        let _subscriber = bus.subscribe();
        tauri::Manager::manage(&app, bus);
        let cancel = AtomicBool::new(false);
        emit_outbound(
            app.handle(),
            &cancel,
            Outbound::Matches(crate::search_job::MatchesPayload {
                id: 1,
                repo: "admin".into(),
                matches: Vec::new(),
            }),
        );
        assert!(cancel.load(Ordering::Relaxed));
    }

    #[test]
    fn service_limits_and_cancels_searches() {
        let service = Service::default();
        let started: Vec<_> = (0..MAX_RUNNING)
            .map(|_| service.register().unwrap())
            .collect();
        assert!(service.register().is_err());
        assert!(service.cancel(started[0].0));
        assert!(started[0].1.load(Ordering::Relaxed));
        assert!(!service.cancel(999));
        assert_eq!(service.cancel_all(), MAX_RUNNING);
        assert!(started.iter().all(|(_, flag)| flag.load(Ordering::Relaxed)));
    }
}
