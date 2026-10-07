use super::{valid_id, Settings};
#[cfg(target_os = "linux")]
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

static WRITES: Mutex<()> = Mutex::new(());

fn parse(bytes: &[u8]) -> Result<Settings, String> {
    let settings: Settings = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    for source in &settings.sources {
        valid_id(&source.id)?;
    }
    Ok(settings)
}

pub(super) fn load(file: &Path) -> Result<(Settings, bool), String> {
    let _guard = WRITES
        .lock()
        .map_err(|_| "Settings persistence is unavailable")?;
    load_with(file, read_file, |from, to| std::fs::rename(from, to))
}

fn read_file(path: &Path) -> std::io::Result<Vec<u8>> {
    #[cfg(test)]
    faults::check_read(path)?;
    std::fs::read(path)
}

#[cfg(test)]
pub(super) mod faults;

pub(super) fn load_with(
    file: &Path,
    read: impl Fn(&Path) -> std::io::Result<Vec<u8>>,
    rename: impl Fn(&Path, &Path) -> std::io::Result<()>,
) -> Result<(Settings, bool), String> {
    let main = match read(file) {
        Ok(bytes) => match parse(&bytes) {
            Ok(settings) => return Ok((settings, false)),
            Err(_) => Some(bytes),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.to_string()),
    };
    let backup = file.with_extension("json.bak");
    match read(&backup) {
        Ok(bytes) => {
            if let Ok(settings) = parse(&bytes) {
                return Ok((settings, true));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    if main.is_some() {
        keep_broken_with(file, rename)?;
    }
    Ok((Settings::default(), false))
}

fn keep_broken(file: &Path) -> Result<(), String> {
    keep_broken_with(file, |from, to| std::fs::rename(from, to))
}

fn keep_broken_with(file: &Path, rename: impl Fn(&Path, &Path) -> std::io::Result<()>) -> Result<(), String> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let kept = file.with_file_name(format!("settings.json.broken-{timestamp}"));
    rename(file, &kept).map_err(|error| error.to_string())?;
    sync_directory(file)
}

pub(super) fn save(file: &Path, settings: &Settings, preserve_valid: bool) -> Result<(), String> {
    let _guard = WRITES
        .lock()
        .map_err(|_| "Settings persistence is unavailable")?;
    let text = serde_json::to_vec_pretty(settings).map_err(|error| error.to_string())?;
    match std::fs::read(file) {
        Ok(previous) => {
            if parse(&previous).is_ok() {
                if preserve_valid {
                    return Err("Refusing to overwrite existing settings after a startup load error. Retry loading settings or restart the app.".into());
                }
                durable_replace(&file.with_extension("json.bak"), &previous)?;
            } else {
                keep_broken(file)?;
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    durable_replace(file, &text)
}

fn durable_replace(file: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = file.with_file_name(format!(
        "{}.tmp",
        file.file_name()
            .ok_or("Settings filename is missing")?
            .to_string_lossy()
    ));
    #[cfg(feature = "test-profile")]
    crate::test_profile::plain_file(&tmp)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp)
        .map_err(|error| error.to_string())?;
    output
        .write_all(bytes)
        .and_then(|()| output.sync_all())
        .map_err(|error| error.to_string())?;
    drop(output);
    replace(&tmp, file)?;
    sync_directory(file)
}

#[cfg(not(windows))]
fn replace(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::rename(from, to).map_err(|error| error.to_string())
}

#[cfg(windows)]
fn replace(from: &Path, to: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: Both paths are valid NUL-terminated buffers for the duration of the call.
    let replaced = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

fn sync_directory(file: &Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    File::open(file.parent().ok_or("Settings directory is missing")?)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| error.to_string())?;
    let _ = file;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Fixture;

    #[test]
    fn torn_main_loads_the_last_good_backup_and_retains_bad_bytes() {
        let fixture = Fixture::new("settings-backup");
        let file = fixture.0.join("settings.json");
        let settings = |generation| Settings {
            sources: vec![],
            workspace: serde_json::json!({"generation":generation}),
        };
        save(&file, &settings(1), false).unwrap();
        save(&file, &settings(2), false).unwrap();
        std::fs::write(&file, b"{torn").unwrap();
        let (loaded, restored) = load(&file).unwrap();
        assert_eq!(loaded.workspace["generation"], 1);
        assert!(restored);
        assert_eq!(std::fs::read(file).unwrap(), b"{torn");
    }

    #[test]
    fn both_broken_files_start_with_defaults_and_keep_the_broken_main() {
        let fixture = Fixture::new("settings-broken");
        let file = fixture.0.join("settings.json");
        std::fs::write(&file, b"{main").unwrap();
        std::fs::write(file.with_extension("json.bak"), b"{backup").unwrap();
        let (loaded, restored) = load(&file).unwrap();
        assert!(loaded.sources.is_empty());
        assert!(loaded.workspace.is_null());
        assert!(!restored);
        let kept = std::fs::read_dir(&fixture.0)
            .unwrap()
            .flatten()
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("settings.json.broken-")
            })
            .unwrap();
        assert_eq!(std::fs::read(kept.path()).unwrap(), b"{main");
        assert_eq!(
            std::fs::read(file.with_extension("json.bak")).unwrap(),
            b"{backup"
        );
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    use crate::platform::Fixture;

    #[test]
    fn saving_after_corruption_preserves_the_backup_and_a_broken_copy() {
        let fixture = Fixture::new("settings-save-corrupt");
        let file = fixture.0.join("settings.json");
        let settings = Settings {
            sources: vec![],
            workspace: serde_json::json!({"root":"saved"}),
        };
        save(&file, &settings, false).unwrap();
        save(&file, &Settings::default(), false).unwrap();
        std::fs::write(&file, b"torn").unwrap();
        save(&file, &Settings::default(), false).unwrap();
        assert_eq!(
            parse(&std::fs::read(file.with_extension("json.bak")).unwrap())
                .unwrap()
                .workspace["root"],
            "saved"
        );
        assert!(std::fs::read_dir(&fixture.0)
            .unwrap()
            .flatten()
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with("settings.json.broken-")));
        assert!(!load(&file).unwrap().1);
    }

    #[test]
    fn missing_main_loads_backup_but_empty_install_uses_defaults() {
        let fixture = Fixture::new("settings-missing");
        let file = fixture.0.join("settings.json");
        assert!(!load(&file).unwrap().1);
        std::fs::write(
            file.with_extension("json.bak"),
            br#"{"workspace":{"root":"saved"}}"#,
        )
        .unwrap();
        let (settings, restored) = load(&file).unwrap();
        assert!(restored);
        assert_eq!(settings.workspace["root"], "saved");
    }
}
