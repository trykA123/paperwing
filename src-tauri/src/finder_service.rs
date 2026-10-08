use crate::finder::{validate, FinderRequest};
use crate::finder_job::{run_job, Emit, Job, Outbound};
use crate::kernel::events::CoreEvent;
use crate::search_service::Service;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn finder_start(
    app: AppHandle,
    service: State<'_, Service>,
    request: FinderRequest,
) -> Result<u64, String> {
    validate(&request)?;
    let options = crate::settings::search_options(&app).await?;
    let (id, cancel) = service.register()?;
    let slot = service.slot(id);
    let stop = cancel.clone();
    let send: Emit = Arc::new(move |outbound| emit_outbound(&app, &stop, outbound));
    tauri::async_runtime::spawn(async move {
        let _slot = slot;
        if let Err(error) = run_job(
            request,
            Job {
                id,
                cancel,
                send,
                options,
            },
        )
        .await
        {
            eprintln!("file finder {id} failed: {}", crate::git::safe(&error));
        }
    });
    Ok(id)
}

fn emit_outbound<R: tauri::Runtime>(app: &AppHandle<R>, cancel: &AtomicBool, outbound: Outbound) {
    let sent = match outbound {
        Outbound::Matches(payload) => {
            crate::events::publish_payload(app, CoreEvent::FinderMatches, &payload)
        }
        Outbound::Done(payload) => {
            crate::events::publish_payload(app, CoreEvent::FinderDone, &payload)
        }
    };
    if sent.is_err() {
        cancel.store(true, Ordering::Relaxed);
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;
    use tauri::test::{mock_builder, mock_context, noop_assets};

    #[test]
    fn a_frontend_delivery_failure_cancels_file_finder_work() {
        let app = mock_builder()
            .manage(crate::events::EmitFailure)
            .build(mock_context(noop_assets()))
            .unwrap();
        let cancel = AtomicBool::new(false);
        emit_outbound(
            app.handle(),
            &cancel,
            Outbound::Matches(crate::finder_job::MatchesPayload {
                id: 47,
                sequence: 1,
                matches: Vec::new(),
            }),
        );
        assert!(cancel.load(Ordering::Relaxed));
    }
}
