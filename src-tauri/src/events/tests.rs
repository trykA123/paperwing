use super::*;

fn legacy_events() -> Vec<(&'static str, CoreEvent, Value)> {
    let payload = serde_json::json!({"id": 7, "sourceId": "admin", "fetchedAt": 3});
    vec![
        (
            "discover-batch",
            CoreEvent::DiscoverBatch(payload.clone()),
            payload.clone(),
        ),
        (
            "discover-done",
            CoreEvent::DiscoverDone(payload.clone()),
            payload.clone(),
        ),
        (
            "search-matches",
            CoreEvent::SearchMatches(payload.clone()),
            payload.clone(),
        ),
        (
            "search-repo",
            CoreEvent::SearchRepo(payload.clone()),
            payload.clone(),
        ),
        (
            "search-done",
            CoreEvent::SearchDone(payload.clone()),
            payload.clone(),
        ),
        (
            "clone-progress",
            CoreEvent::CloneProgress(payload.clone()),
            payload.clone(),
        ),
        ("clone-finished", CoreEvent::CloneFinished, Value::Null),
        ("launch-request", CoreEvent::LaunchRequest, Value::Null),
        (
            "credential-changed",
            CoreEvent::CredentialChanged(payload.clone()),
            payload.clone(),
        ),
        (
            "git-activity",
            CoreEvent::GitActivity(payload.clone()),
            payload.clone(),
        ),
        (
            "diagnostics-progress",
            CoreEvent::DiagnosticsProgress(payload.clone()),
            payload,
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
        assert_eq!(frontend_event(&received), Some((name, payload)));
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
