use crate::kernel::events::{CoreEvent, EventBus, EventPayload};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub(crate) fn install<R: Runtime>(app: &AppHandle<R>) {
    app.manage(EventBus::default());
}

pub(crate) fn publish<R: Runtime>(app: &AppHandle<R>, event: CoreEvent) -> Result<(), String> {
    forward(app, &event)?;
    publish_bus(app, event);
    Ok(())
}

pub(crate) fn publish_payload<R: Runtime>(
    app: &AppHandle<R>,
    event: fn(EventPayload) -> CoreEvent,
    payload: &impl Serialize,
) -> Result<(), String> {
    let event = event(EventPayload::new(payload).map_err(|error| error.to_string())?);
    if let Some((name, _)) = frontend_event(&event) {
        emit_frontend(app, name, payload)?;
    }
    publish_bus(app, event);
    Ok(())
}

fn publish_bus<R: Runtime>(app: &AppHandle<R>, event: CoreEvent) {
    if let Some(bus) = app.try_state::<EventBus>() {
        let _ = bus.publish(event);
    }
}

fn frontend_event(event: &CoreEvent) -> Option<(&'static str, Option<&EventPayload>)> {
    let (name, payload) = match event {
        CoreEvent::DiscoverBatch(payload) => ("discover-batch", Some(payload)),
        CoreEvent::DiscoverDone(payload) => ("discover-done", Some(payload)),
        CoreEvent::SearchMatches(payload) => (crate::search_job::MATCHES_EVENT, Some(payload)),
        CoreEvent::SearchRepo(payload) => (crate::search_job::REPO_EVENT, Some(payload)),
        CoreEvent::SearchDone(payload) => (crate::search_job::DONE_EVENT, Some(payload)),
        CoreEvent::CloneProgress(payload) => ("clone-progress", Some(payload)),
        CoreEvent::CloneFinished => ("clone-finished", None),
        CoreEvent::LaunchRequest => (crate::launch::LAUNCH_EVENT, None),
        CoreEvent::CredentialChanged(payload) => ("credential-changed", Some(payload)),
        CoreEvent::GitActivity(payload) => ("git-activity", Some(payload)),
        CoreEvent::DiagnosticsProgress(payload) => ("diagnostics-progress", Some(payload)),
        CoreEvent::RepoChanged(payload) => ("repo-changed", Some(payload)),
        CoreEvent::WatchFailed(payload) => ("watch-failed", Some(payload)),
        _ => return None,
    };
    Some((name, payload))
}

fn forward<R: Runtime>(app: &AppHandle<R>, event: &CoreEvent) -> Result<(), String> {
    if let Some((name, payload)) = frontend_event(event) {
        match payload {
            Some(payload) => emit_frontend(app, name, payload)?,
            None => emit_frontend(app, name, &())?,
        }
    }
    Ok(())
}

fn emit_frontend<R: Runtime>(
    app: &AppHandle<R>,
    name: &str,
    payload: &impl Serialize,
) -> Result<(), String> {
    #[cfg(test)]
    if app.try_state::<EmitFailure>().is_some() {
        return Err("frontend window is gone".into());
    }
    app.emit(name, payload).map_err(|error| error.to_string())
}

#[cfg(test)]
pub(crate) struct EmitFailure;

#[cfg(test)]
mod tests;
