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

#[tokio::test]
async fn clean_startup_command_retries_a_transient_load_failure_regression() {
    if !run_isolated(
        "settings::retry_tests::clean_startup_command_retries_a_transient_load_failure_regression",
    ) {
        return;
    }
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("settings-command-retry");
    let app = app_for(&fixture);
    let file = settings_file(app.handle()).unwrap();
    saved_settings(&file);
    initialize(app.handle().clone()).unwrap();
    assert!(app.state::<Startup>().check().is_ok());
    let _failure = ReadFailure::new(&file, 1);

    let loaded = commands::load_settings(app.handle().clone()).await.unwrap();
    assert_eq!(loaded.settings.workspace["root"], "saved-root");
    assert!(loaded.startup_error.is_none());
    assert!(app.state::<Startup>().check().is_ok());
}

#[tokio::test]
async fn clean_startup_command_returns_defaults_and_guards_saves_after_two_failures_regression() {
    if !run_isolated("settings::retry_tests::clean_startup_command_returns_defaults_and_guards_saves_after_two_failures_regression") {
        return;
    }
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("settings-command-fallback");
    let app = app_for(&fixture);
    let file = settings_file(app.handle()).unwrap();
    let source = saved_settings(&file);
    let original = std::fs::read(&file).unwrap();
    initialize(app.handle().clone()).unwrap();
    let _failure = ReadFailure::new(&file, 2);

    let loaded = commands::load_settings(app.handle().clone()).await.unwrap();
    assert!(loaded.settings.sources.is_empty());
    assert!(loaded.settings.workspace.is_null());
    assert!(loaded
        .startup_error
        .unwrap()
        .contains("injected settings read failure"));
    assert!(app.state::<Startup>().check().is_err());
    let error = commands::save_settings(
        app.handle().clone(),
        Settings {
            sources: vec![source],
            workspace: serde_json::Value::Null,
        },
    )
    .await
    .unwrap_err();
    assert!(
        error.contains("Refusing to overwrite existing settings"),
        "{error}"
    );
    assert_eq!(std::fs::read(&file).unwrap(), original);
    assert!(!file.with_extension("json.bak").exists());
}

#[tokio::test]
async fn startup_autosave_preserves_torn_main_and_valid_backup_regression() {
    if !run_isolated(
        "settings::retry_tests::startup_autosave_preserves_torn_main_and_valid_backup_regression",
    ) {
        return;
    }
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("settings-preserve-backup");
    let app = app_for(&fixture);
    let file = settings_file(app.handle()).unwrap();
    let backup = file.with_extension("json.bak");
    saved_settings(&backup);
    let original = std::fs::read(&backup).unwrap();
    std::fs::write(&file, b"{torn").unwrap();
    app.state::<Startup>().record("startup load failed".into());

    let error = commands::save_settings(app.handle().clone(), Settings::default())
        .await
        .unwrap_err();
    assert!(
        error.contains("Refusing to overwrite existing settings"),
        "{error}"
    );
    assert_eq!(std::fs::read(&file).unwrap(), b"{torn");
    assert_eq!(std::fs::read(&backup).unwrap(), original);
    assert_eq!(std::fs::read_dir(&fixture.0).unwrap().count(), 3);
    assert!(app.state::<Startup>().check().is_err());
}

#[tokio::test]
async fn startup_autosave_preserves_torn_main_and_unreadable_backup_regression() {
    if !run_isolated("settings::retry_tests::startup_autosave_preserves_torn_main_and_unreadable_backup_regression") {
        return;
    }
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("settings-preserve-unreadable-backup");
    let app = app_for(&fixture);
    let file = settings_file(app.handle()).unwrap();
    let backup = file.with_extension("json.bak");
    std::fs::write(&file, b"{torn").unwrap();
    std::fs::write(&backup, b"{unreadable").unwrap();
    app.state::<Startup>().record("startup load failed".into());
    let _failure = ReadFailure::new(&backup, 1);

    let error = commands::save_settings(app.handle().clone(), Settings::default())
        .await
        .unwrap_err();
    assert!(error.contains("injected settings read failure"), "{error}");
    assert_eq!(std::fs::read(&file).unwrap(), b"{torn");
    assert_eq!(std::fs::read(&backup).unwrap(), b"{unreadable");
    assert_eq!(std::fs::read_dir(&fixture.0).unwrap().count(), 3);
    assert!(app.state::<Startup>().check().is_err());
}

#[tokio::test]
async fn internal_load_failures_leave_startup_clean_and_saves_working_regression() {
    if !run_isolated("settings::retry_tests::internal_load_failures_leave_startup_clean_and_saves_working_regression") {
        return;
    }
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("settings-internal-load");
    let app = app_for(&fixture);
    let file = settings_file(app.handle()).unwrap();
    let source = saved_settings(&file);
    initialize(app.handle().clone()).unwrap();
    {
        let _failure = ReadFailure::new(&file, 2);
        assert!(load_settings(app.handle().clone()).is_err());
    }
    assert!(app.state::<Startup>().check().is_ok());
    commands::save_settings(
        app.handle().clone(),
        Settings {
            sources: vec![source],
            workspace: serde_json::json!({ "root": "edited-root" }),
        },
    )
    .await
    .unwrap();
    assert!(String::from_utf8(std::fs::read(&file).unwrap())
        .unwrap()
        .contains("edited-root"));
}
