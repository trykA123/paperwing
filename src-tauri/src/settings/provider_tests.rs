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

fn run_isolated(short_name: &str) -> bool {
    let name = format!("settings::provider_tests::{short_name}");
    if std::env::var("SKEIN_PROVIDER_SETTINGS_TEST").as_deref() == Ok(&name) {
        return true;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &name, "--nocapture", "--test-threads=1"])
        .env("SKEIN_PROVIDER_SETTINGS_TEST", &name)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    false
}

fn saved_flags(app: &tauri::App<MockRuntime>) {
    let sources = serde_json::from_value(serde_json::json!([
        {"id":"disabled-file","name":"admin","kind":"ghe","host":"disabled.invalid","enabled":false},
        {"id":"enabled-file","name":"admin","kind":"ghe","host":"enabled.invalid","enabled":true}
    ])).unwrap();
    persistence::save(
        &settings_file(app.handle()).unwrap(),
        &Settings {
            sources,
            workspace: serde_json::Value::Null,
        },
        false,
    )
    .unwrap();
}

#[test]
fn load_settings_uses_file_flags_when_the_store_is_unavailable() {
    if !run_isolated("load_settings_uses_file_flags_when_the_store_is_unavailable") {
        return;
    }
    let fixture = Fixture::new("settings-unavailable-store");
    let app = app_for(&fixture, Store::disabled());
    saved_flags(&app);
    let loaded = load_settings(app.handle().clone()).unwrap();
    assert!(!loaded.sources[0].enabled);
    assert!(loaded.sources[1].enabled);
}

#[test]
fn load_settings_does_not_wait_for_store_open() {
    if !run_isolated("load_settings_does_not_wait_for_store_open") {
        return;
    }
    let fixture = Fixture::new("settings-pending-store");
    let app = app_for(&fixture, Store::pending());
    saved_flags(&app);
    let handle = app.handle().clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        sender.send(load_settings(handle)).unwrap();
    });
    let loaded = receiver
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("settings load waited for store open")
        .unwrap();
    worker.join().unwrap();
    assert!(!loaded.sources[0].enabled);
    assert!(loaded.sources[1].enabled);
}

#[test]
fn startup_configures_file_sources_without_waiting_for_store_open() {
    if !run_isolated("startup_configures_file_sources_without_waiting_for_store_open") {
        return;
    }
    let fixture = Fixture::new("startup-pending-store");
    let app = app_for(&fixture, Store::pending());
    saved_flags(&app);
    let handle = app.handle().clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        sender.send(initialize(handle)).unwrap();
    });
    receiver
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("startup waited for store open")
        .unwrap();
    worker.join().unwrap();
    assert!(app.state::<Startup>().check().is_ok());
    assert!(app
        .state::<Runtime>()
        .registry
        .check_enabled("disabled-file")
        .is_err());
    assert!(app
        .state::<Runtime>()
        .registry
        .check_enabled("enabled-file")
        .is_ok());
}

#[test]
fn the_store_migrates_away_the_provider_flags_table() {
    let fixture = Fixture::new("store-provider-flags-migration");
    let path = fixture.0.join("store.sqlite3");
    let options = crate::store::Options {
        migrations: &crate::store::Options::default().migrations[..2],
        ..Default::default()
    };
    let previous = Store::open(&path, &options).unwrap();
    previous.close();
    let store = Store::open(&path, &Default::default()).unwrap();
    assert!(store.recovered_from().is_none());
    let count: i64 = store
        .read_blocking(|connection| {
            Ok(connection.query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = 'providers'",
                [],
                |row| row.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(count, 0);
    store.close();
}

#[tokio::test]
async fn settings_commands_keep_file_flags_and_reenable_the_source_after_restart() {
    let name = "settings::provider_tests::settings_commands_keep_file_flags_and_reenable_the_source_after_restart";
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

    source.enabled = true;
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
    let config = crate::providers::configuration(&source, &source.host).unwrap();
    assert!(runtime
        .registry
        .acquire(&config, |_| panic!("enabled provider should already exist"))
        .is_ok());
    drop(app);
    store.close();
}
