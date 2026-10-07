use crate::core::events::{CoreEvent, EventBus};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub(crate) fn install<R: Runtime>(app: &AppHandle<R>) {
    let bus = EventBus::default();
    let mut receiver = bus.subscribe();
    app.manage(bus);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    if let Err(error) = forward(&app, &event) {
                        eprintln!("Core event forwarding failed: {error}");
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                    eprintln!("Core event forwarder missed {count} events");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

pub(crate) fn publish<R: Runtime>(app: &AppHandle<R>, event: CoreEvent) -> Result<(), String> {
    if let Some(bus) = app.try_state::<EventBus>() {
        return bus
            .publish(event)
            .map(|_| ())
            .map_err(|error| error.to_string());
    }
    forward(app, &event)
}

pub(crate) fn publish_payload<R: Runtime>(
    app: &AppHandle<R>,
    event: fn(Value) -> CoreEvent,
    payload: &impl Serialize,
) -> Result<(), String> {
    publish(
        app,
        event(serde_json::to_value(payload).map_err(|error| error.to_string())?),
    )
}

fn frontend_event(event: &CoreEvent) -> Option<(&'static str, Value)> {
    let (name, payload) = match event {
        CoreEvent::DiscoverBatch(payload) => ("discover-batch", payload.clone()),
        CoreEvent::DiscoverDone(payload) => ("discover-done", payload.clone()),
        CoreEvent::SearchMatches(payload) => (crate::search_job::MATCHES_EVENT, payload.clone()),
        CoreEvent::SearchRepo(payload) => (crate::search_job::REPO_EVENT, payload.clone()),
        CoreEvent::SearchDone(payload) => (crate::search_job::DONE_EVENT, payload.clone()),
        CoreEvent::CloneProgress(payload) => ("clone-progress", payload.clone()),
        CoreEvent::CloneFinished => ("clone-finished", Value::Null),
        CoreEvent::LaunchRequest => (crate::launch::LAUNCH_EVENT, Value::Null),
        CoreEvent::CredentialChanged(payload) => ("credential-changed", payload.clone()),
        CoreEvent::GitActivity(payload) => ("git-activity", payload.clone()),
        CoreEvent::DiagnosticsProgress(payload) => ("diagnostics-progress", payload.clone()),
        _ => return None,
    };
    Some((name, payload))
}

fn forward<R: Runtime>(app: &AppHandle<R>, event: &CoreEvent) -> Result<(), String> {
    if let Some((name, payload)) = frontend_event(event) {
        app.emit(name, payload).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
