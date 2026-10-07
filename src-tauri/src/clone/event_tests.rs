use super::*;

#[cfg(not(windows))]
#[test]
fn clone_progress_keeps_the_pre_boundary_payload_bytes() {
    use std::sync::{Arc, Mutex};
    use tauri::{
        Listener,
        test::{mock_builder, mock_context, noop_assets},
    };
    let app = mock_builder().build(mock_context(noop_assets())).unwrap();
    let received = Arc::new(Mutex::new(None));
    let sink = received.clone();
    app.listen("clone-progress", move |event| {
        *sink.lock().unwrap() = Some(event.payload().to_string())
    });
    let payload = Progress {
        id: "admin",
        phase: "receiving",
        pct: 33.3,
        msg: "progress".into(),
    };
    let expected = r#"{"id":"admin","phase":"receiving","pct":33.3,"msg":"progress"}"#;
    assert_eq!(serde_json::to_string(&payload).unwrap(), expected);
    crate::events::publish_payload(app.handle(), CoreEvent::CloneProgress, &payload).unwrap();
    assert_eq!(received.lock().unwrap().as_deref(), Some(expected));

    let bus = crate::kernel::events::EventBus::default();
    let mut subscriber = bus.subscribe();
    tauri::Manager::manage(&app, bus);
    crate::events::publish_payload(app.handle(), CoreEvent::CloneProgress, &payload).unwrap();
    assert_eq!(received.lock().unwrap().as_deref(), Some(expected));
    let event = subscriber.try_recv().unwrap();
    let encoded = serde_json::to_vec(&event).unwrap();
    let restored: CoreEvent = serde_json::from_slice(&encoded).unwrap();
    let CoreEvent::CloneProgress(restored) = restored else {
        panic!("expected clone-progress");
    };
    assert_eq!(serde_json::to_string(&restored).unwrap(), expected);
}
