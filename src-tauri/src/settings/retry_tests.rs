use super::*;
use crate::platform::Fixture;
use persistence::faults::ReadFailure;
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};

fn run_isolated(name: &str) -> bool {
    if std::env::var("SKEIN_STARTUP_RETRY_TEST").as_deref() == Ok(name) {
        return true;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env("SKEIN_STARTUP_RETRY_TEST", name)
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

fn app_for(fixture: &Fixture) -> tauri::App<MockRuntime> {
    let mut context = mock_context(noop_assets());
    context.config_mut().app.app_directories_override =
        Some(serde_json::from_value(serde_json::json!({ "config": fixture.0 })).unwrap());
    mock_builder()
        .manage(Startup::default())
        .build(context)
        .unwrap()
}

fn saved_settings(file: &std::path::Path) -> Source {
    let source: Source = serde_json::from_value(serde_json::json!({
        "id": "startup-retry", "name": "admin", "kind": "github", "host": "github.com"
    }))
    .unwrap();
    let settings = Settings {
        sources: vec![source.clone()],
        workspace: serde_json::json!({ "root": "saved-root" }),
    };
    std::fs::write(file, serde_json::to_vec(&settings).unwrap()).unwrap();
    source
}

#[tokio::test]
async fn real_startup_retries_the_load_and_configures_recovered_sources_regression() {
    if !run_isolated("settings::retry_tests::real_startup_retries_the_load_and_configures_recovered_sources_regression") { return; }
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("settings-retry-success");
    let app = app_for(&fixture);
    let file = settings_file(app.handle()).unwrap();
    let source = saved_settings(&file);
    let _failure = ReadFailure::new(&file, 1);
    initialize(app.handle().clone()).unwrap();
    assert!(app.state::<Startup>().check().is_err());
    assert!(crate::credentials::metadata_revision(&source).is_err());

    let loaded = commands::load_settings(app.handle().clone()).await.unwrap();
    assert_eq!(loaded.settings.workspace["root"], "saved-root");
    assert_eq!(loaded.settings.sources[0].id, source.id);
    assert!(app.state::<Startup>().check().is_ok());
    assert!(crate::credentials::metadata_revision(&source).is_ok());
    assert!(serde_json::to_value(&loaded).unwrap()["startupError"].is_null());
    crate::credentials::configure_sources(app.handle(), &[], false);
    crate::git::configure_sources(Vec::new());
}

#[tokio::test]
async fn real_startup_returns_defaults_with_error_and_refuses_to_overwrite_valid_settings_regression(
) {
    if !run_isolated("settings::retry_tests::real_startup_returns_defaults_with_error_and_refuses_to_overwrite_valid_settings_regression") { return; }
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("settings-retry-failure");
    let app = app_for(&fixture);
    let file = settings_file(app.handle()).unwrap();
    saved_settings(&file);
    let original = std::fs::read(&file).unwrap();
    let _failure = ReadFailure::new(&file, 2);
    initialize(app.handle().clone()).unwrap();

    let loaded = commands::load_settings(app.handle().clone()).await.unwrap();
    assert!(loaded.settings.sources.is_empty());
    assert!(loaded.settings.workspace.is_null());
    assert!(serde_json::to_value(&loaded).unwrap()["startupError"]
        .as_str()
        .unwrap()
        .contains("injected settings read failure"));
    let error = commands::save_settings(app.handle().clone(), Settings::default())
        .await
        .unwrap_err();
    assert!(
        error.contains("Refusing to overwrite existing settings"),
        "{error}"
    );
    assert_eq!(std::fs::read(&file).unwrap(), original);
    assert!(!file.with_extension("json.bak").exists());
    assert!(app.state::<Startup>().check().is_err());

    let recovered = load_settings(app.handle().clone()).unwrap();
    assert_eq!(recovered.workspace["root"], "saved-root");
    assert!(app.state::<Startup>().check().is_ok());
    save_settings(app.handle().clone(), recovered).unwrap();
    crate::credentials::configure_sources(app.handle(), &[], false);
    crate::git::configure_sources(Vec::new());
}

#[tokio::test]
async fn real_startup_save_refuses_valid_main_even_before_the_load_retry_regression() {
    if !run_isolated("settings::retry_tests::real_startup_save_refuses_valid_main_even_before_the_load_retry_regression") { return; }
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("settings-save-before-retry");
    let app = app_for(&fixture);
    let file = settings_file(app.handle()).unwrap();
    saved_settings(&file);
    let original = std::fs::read(&file).unwrap();
    let _failure = ReadFailure::new(&file, 1);
    initialize(app.handle().clone()).unwrap();
    let error = commands::save_settings(app.handle().clone(), Settings::default())
        .await
        .unwrap_err();
    assert!(
        error.contains("Refusing to overwrite existing settings"),
        "{error}"
    );
    assert_eq!(std::fs::read(file).unwrap(), original);
}
