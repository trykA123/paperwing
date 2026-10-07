use super::*;
use crate::{platform::Fixture, providers::Runtime, store::Store};
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};

fn app_for(fixture: &Fixture, store: Store) -> tauri::App<MockRuntime> {
    let mut context = mock_context(noop_assets());
    context.config_mut().app.app_directories_override =
        Some(serde_json::from_value(serde_json::json!({"config": fixture.0})).unwrap());
    mock_builder()
        .manage(Startup::default())
        .manage(Runtime::new(store))
        .build(context)
        .unwrap()
}

#[tokio::test]
async fn settings_commands_restore_sqlite_flags_and_reenable_the_source_after_restart() {
    let name = "settings::provider_tests::settings_commands_restore_sqlite_flags_and_reenable_the_source_after_restart";
    if std::env::var("SKEIN_PROVIDER_SETTINGS_TEST").as_deref() != Ok(name) {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env("SKEIN_PROVIDER_SETTINGS_TEST", name)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let fixture = Fixture::new("provider-settings");
    let database = fixture.0.join("store.sqlite3");
    let store = Store::open(&database, &Default::default()).unwrap();
    let app = app_for(&fixture, store.clone());
    let mut source: Source = serde_json::from_value(serde_json::json!({"id":"provider-settings","name":"admin","kind":"ghe","host":"enterprise.invalid","enabled":false})).unwrap();
    commands::save_settings(
        app.handle().clone(),
        Settings {
            sources: vec![source.clone()],
            workspace: serde_json::Value::Null,
        },
    )
    .await
    .unwrap();
    let file = settings_file(app.handle()).unwrap();
    drop(app);
    store.close();
    source.enabled = true;
    persistence::save(
        &file,
        &Settings {
            sources: vec![source.clone()],
            workspace: serde_json::Value::Null,
        },
        false,
    )
    .unwrap();

    let store = Store::open(&database, &Default::default()).unwrap();
    let app = app_for(&fixture, store.clone());
    initialize(app.handle().clone()).unwrap();
    let loaded = commands::load_settings(app.handle().clone()).await.unwrap();
    assert!(!loaded.settings.sources[0].enabled);
    let runtime = app.state::<Runtime>();
    let config = crate::providers::configuration(&source, &source.host).unwrap();
    assert!(runtime
        .registry
        .acquire(&config, |_| panic!(
            "disabled provider must not be constructed"
        ))
        .is_err());

    commands::save_settings(
        app.handle().clone(),
        Settings {
            sources: vec![source.clone()],
            workspace: serde_json::Value::Null,
        },
    )
    .await
    .unwrap();
    let loaded = commands::load_settings(app.handle().clone()).await.unwrap();
    assert!(loaded.settings.sources[0].enabled);
    assert!(runtime
        .registry
        .acquire(&config, |_| panic!("enabled provider should already exist"))
        .is_ok());
    drop(app);
    store.close();
}
