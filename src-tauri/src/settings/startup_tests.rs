use super::*;
use crate::platform::Fixture;
use std::io::{Error, ErrorKind};

fn assert_defaults_and_ui_error(load: impl FnOnce() -> Result<Settings, String>, expected: &str) {
    let state = Startup::default();
    let mut configured = false;
    let result = initialize_with(&state, load, |sources| {
        assert!(sources.is_empty());
        configured = true;
    });
    assert!(result.is_ok(), "startup aborted: {result:?}");
    assert!(configured);
    assert!(state.check().unwrap_err().contains(expected));
}

#[test]
fn unreadable_settings_start_with_defaults_and_report_the_error_regression() {
    let fixture = Fixture::new("settings-init-denied");
    let file = fixture.0.join("settings.json");
    std::fs::write(&file, b"{}").unwrap();
    assert_defaults_and_ui_error(
        || {
            persistence::load_with(
                &file,
                |_| {
                    Err(Error::new(
                        ErrorKind::PermissionDenied,
                        "settings permission denied",
                    ))
                },
                |_, _| panic!("unreadable files must stay intact"),
            )
            .map(|loaded| loaded.0)
        },
        "settings permission denied",
    );
    assert_eq!(std::fs::read(&file).unwrap(), b"{}");
}

#[cfg(unix)]
#[test]
fn unreadable_settings_file_does_not_abort_initialization_regression() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new("settings-init-mode");
    let file = fixture.0.join("settings.json");
    std::fs::write(&file, b"{}").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
    let state = Startup::default();
    let mut configured = false;
    let result = initialize_with(
        &state,
        || persistence::load(&file).map(|loaded| loaded.0),
        |sources| {
            assert!(sources.is_empty());
            configured = true;
        },
    );
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(result.is_ok(), "startup aborted: {result:?}");
    assert!(configured);
    assert!(state.check().is_err());
}

#[test]
fn failed_broken_file_rename_starts_with_defaults_and_reports_the_error_regression() {
    let fixture = Fixture::new("settings-init-sharing");
    let file = fixture.0.join("settings.json");
    std::fs::write(&file, b"{broken").unwrap();
    assert_defaults_and_ui_error(
        || {
            persistence::load_with(
                &file,
                |path| std::fs::read(path),
                |from, _| {
                    assert_eq!(from, file);
                    Err(Error::new(
                        ErrorKind::PermissionDenied,
                        "settings sharing violation",
                    ))
                },
            )
            .map(|loaded| loaded.0)
        },
        "settings sharing violation",
    );
    assert_eq!(std::fs::read(&file).unwrap(), b"{broken");
}
