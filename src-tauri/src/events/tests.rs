use super::*;
use serde_json::Value;

#[cfg(not(windows))]
#[test]
fn publishing_with_no_in_process_subscribers_still_delivers_to_the_frontend() {
    use std::sync::{Arc, Mutex};
    use tauri::{
        test::{mock_builder, mock_context, noop_assets},
        Listener,
    };
    let app = mock_builder().build(mock_context(noop_assets())).unwrap();
    install(app.handle());
    let delivered = Arc::new(Mutex::new(false));
    let sink = delivered.clone();
    app.listen("clone-finished", move |_| *sink.lock().unwrap() = true);
    publish(app.handle(), CoreEvent::CloneFinished).unwrap();
    assert!(*delivered.lock().unwrap());
}

#[cfg(not(windows))]
#[test]
fn terminal_events_reach_the_frontend_before_any_subscriber_runs() {
    use std::sync::{Arc, Mutex};
    use tauri::{
        test::{mock_builder, mock_context, noop_assets},
        Listener,
    };
    let app = mock_builder().build(mock_context(noop_assets())).unwrap();
    let bus = EventBus::default();
    let mut subscriber = bus.subscribe();
    app.manage(bus);
    let received = Arc::new(Mutex::new(Vec::new()));
    for name in ["search-done", "discover-done", "clone-finished"] {
        let received = received.clone();
        app.listen(name, move |_| received.lock().unwrap().push(name));
    }
    for id in 0..5000 {
        publish_payload(
            app.handle(),
            CoreEvent::SearchDone,
            &serde_json::json!({"id": id}),
        )
        .unwrap();
        publish_payload(
            app.handle(),
            CoreEvent::DiscoverDone,
            &serde_json::json!({"id": id}),
        )
        .unwrap();
        publish(app.handle(), CoreEvent::CloneFinished).unwrap();
    }
    let received = received.lock().unwrap();
    for name in ["search-done", "discover-done", "clone-finished"] {
        assert_eq!(
            received.iter().filter(|event| **event == name).count(),
            5000,
            "{name}"
        );
    }
    assert!(matches!(
        subscriber.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_))
    ));
}

fn legacy_events() -> Vec<(&'static str, CoreEvent, Value)> {
    let value = serde_json::json!({"id": 7, "sourceId": "admin", "fetchedAt": 3});
    let payload = EventPayload::new(&value).unwrap();
    vec![
        (
            "discover-batch",
            CoreEvent::DiscoverBatch(payload.clone()),
            value.clone(),
        ),
        (
            "discover-done",
            CoreEvent::DiscoverDone(payload.clone()),
            value.clone(),
        ),
        (
            "search-matches",
            CoreEvent::SearchMatches(payload.clone()),
            value.clone(),
        ),
        (
            "search-repo",
            CoreEvent::SearchRepo(payload.clone()),
            value.clone(),
        ),
        (
            "search-done",
            CoreEvent::SearchDone(payload.clone()),
            value.clone(),
        ),
        (
            "clone-progress",
            CoreEvent::CloneProgress(payload.clone()),
            value.clone(),
        ),
        ("clone-finished", CoreEvent::CloneFinished, Value::Null),
        ("launch-request", CoreEvent::LaunchRequest, Value::Null),
        (
            "credential-changed",
            CoreEvent::CredentialChanged(payload.clone()),
            value.clone(),
        ),
        (
            "git-activity",
            CoreEvent::GitActivity(payload.clone()),
            value.clone(),
        ),
        (
            "diagnostics-progress",
            CoreEvent::DiagnosticsProgress(payload.clone()),
            value.clone(),
        ),
        (
            "repo-changed",
            CoreEvent::RepoChanged(payload.clone()),
            value.clone(),
        ),
        (
            "watch-failed",
            CoreEvent::WatchFailed(payload),
            value,
        ),
    ]
}

#[tokio::test]
async fn every_previous_frontend_event_name_and_payload_survives_the_bus() {
    let bus = EventBus::default();
    let mut receiver = bus.subscribe();
    for (name, event, payload) in legacy_events() {
        let encoded = serde_json::to_vec(&event).unwrap();
        let decoded = serde_json::from_slice(&encoded).unwrap();
        bus.publish(decoded).unwrap();
        let received = receiver.recv().await.unwrap();
        let (actual_name, actual_payload) = frontend_event(&received).unwrap();
        assert_eq!(actual_name, name);
        assert_eq!(serde_json::to_value(actual_payload).unwrap(), payload);
    }
}

#[cfg(not(windows))]
#[tokio::test]
async fn the_tauri_forwarder_delivers_every_legacy_event_to_listeners() {
    use tauri::{
        test::{mock_builder, mock_context, noop_assets},
        Listener,
    };
    let app = mock_builder().build(mock_context(noop_assets())).unwrap();
    install(app.handle());
    for (name, event, payload) in legacy_events() {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let listener = app.listen(name, move |event| {
            sender.send(event.payload().to_string()).unwrap();
        });
        publish(app.handle(), event).unwrap();
        let received = tokio::time::timeout(std::time::Duration::from_secs(5), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&received).unwrap(), payload);
        app.unlisten(listener);
    }
}
