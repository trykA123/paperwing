use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Capability {
    pub supported: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub read_compare: Capability,
    pub edit: Capability,
    pub copy: Capability,
    pub recovery: Capability,
    pub trash: Capability,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    platform: &'static str,
    separator: &'static str,
    capabilities: Capabilities,
    credentials: crate::credentials::Capability,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootSupport {
    root: String,
    valid: bool,
    reason: Option<String>,
    identity: Option<String>,
    case_policy: &'static str,
    capabilities: Capabilities,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathIdentity {
    path: String,
    identity: Option<String>,
    exists: bool,
    reason: Option<String>,
}

fn available() -> Capability {
    Capability {
        supported: true,
        reason: None,
    }
}

fn unavailable(reason: &str) -> Capability {
    Capability {
        supported: false,
        reason: Some(reason.into()),
    }
}

pub(crate) fn unavailable_reason(operation: &str) -> String {
    if cfg!(target_os = "linux") {
        match operation {
            "edit" => "Linux editing requires the later recoverable write backend.",
            "copy" => "Linux copying requires the later recoverable copy backend.",
            "recovery" => "Linux recovery requires the later recovery backend.",
            "trash" => "Linux folder trash requires the later native trash backend.",
            _ => "Linux support for this operation is unavailable.",
        }
        .into()
    } else {
        format!("This platform does not support {operation}.")
    }
}

fn capabilities() -> Capabilities {
    if cfg!(windows) {
        Capabilities {
            read_compare: available(),
            edit: available(),
            copy: available(),
            recovery: available(),
            trash: available(),
        }
    } else {
        Capabilities {
            read_compare: if cfg!(target_os = "linux") {
                available()
            } else {
                unavailable("Native comparisons are unsupported on this platform.")
            },
            edit: unavailable(&unavailable_reason("edit")),
            copy: unavailable(&unavailable_reason("copy")),
            recovery: unavailable(&unavailable_reason("recovery")),
            trash: if cfg!(target_os = "linux") {
                available()
            } else {
                unavailable(&unavailable_reason("trash"))
            },
        }
    }
}

#[cfg(any(windows, target_os = "linux", test))]
fn capabilities_with_write_support(
    mut capabilities: Capabilities,
    support: Result<(), String>,
) -> Capabilities {
    if let Err(reason) = support {
        capabilities.edit = unavailable(&reason);
        capabilities.copy = unavailable(&reason);
        capabilities.recovery = unavailable(&reason);
        capabilities.trash = unavailable(&reason);
    }
    capabilities
}

fn root_capabilities(_path: &Path) -> Capabilities {
    let capabilities = capabilities();
    #[cfg(windows)]
    let capabilities = capabilities_with_write_support(capabilities, supported_volume(_path));
    #[cfg(target_os = "linux")]
    let capabilities = capabilities_with_write_support(
        capabilities,
        crate::linux_guard::Root::open(_path, &[])
            .and_then(|root| root.probe_write())
            .map_err(|error| error.to_string()),
    );
    capabilities
}

fn refused_capabilities(reason: &str) -> Capabilities {
    let capability = unavailable(reason);
    Capabilities {
        read_compare: capability.clone(),
        edit: capability.clone(),
        copy: capability.clone(),
        recovery: capability.clone(),
        trash: capability,
    }
}

#[tauri::command]
pub async fn platform_info() -> PlatformInfo {
    PlatformInfo {
        platform: if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "linux") {
            "linux"
        } else {
            "unsupported"
        },
        separator: if cfg!(windows) { "\\" } else { "/" },
        capabilities: capabilities(),
        credentials: crate::credentials::capability().await,
    }
}

pub(crate) fn native_root(root: &str) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    if !root.starts_with('/') && (root.as_bytes().get(1) == Some(&b':') || root.starts_with('\\')) {
        return Err("Saved root is foreign to Linux; explicitly reassign it to an existing native directory.".into());
    }
    if !cfg!(any(windows, target_os = "linux")) {
        return Err("Native roots are unsupported on this platform.".into());
    }
    crate::git::valid_path(root, true)?;
    for part in Path::new(root).components() {
        let text = part
            .as_os_str()
            .to_str()
            .ok_or("Unsupported root encoding")?;
        if text.eq_ignore_ascii_case(".git")
            || (cfg!(windows) && text.to_ascii_lowercase().starts_with("git~"))
        {
            return Err("Repository metadata cannot be a workspace root.".into());
        }
    }
    if !Path::new(root).is_dir() {
        return Err("Root must be an existing directory.".into());
    }
    Ok(())
}

#[cfg(windows)]
fn supported_volume(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetVolumeInformationW, GetVolumePathNameW,
    };
    const DRIVE_FIXED: u32 = 3;
    const FILE_READ_ONLY_VOLUME: u32 = 0x0008_0000;
    const FILE_SUPPORTS_TRANSACTIONS: u32 = 0x0020_0000;
    let input: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut volume = vec![0u16; 32768];
    if unsafe { GetVolumePathNameW(input.as_ptr(), volume.as_mut_ptr(), volume.len() as u32) } == 0
    {
        return Err("Native volume support could not be verified.".into());
    }
    if unsafe { GetDriveTypeW(volume.as_ptr()) } != DRIVE_FIXED {
        return Err("Only local fixed NTFS roots support this Windows backend.".into());
    }
    let mut filesystem = [0u16; 64];
    let mut flags = 0;
    if unsafe {
        GetVolumeInformationW(
            volume.as_ptr(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut flags,
            filesystem.as_mut_ptr(),
            filesystem.len() as u32,
        )
    } == 0
    {
        return Err("Native volume support could not be verified.".into());
    }
    let length = filesystem
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(filesystem.len());
    if String::from_utf16_lossy(&filesystem[..length]) != "NTFS"
        || flags & FILE_READ_ONLY_VOLUME != 0
    {
        return Err("A writable local NTFS volume is required by this Windows backend.".into());
    }
    if flags & FILE_SUPPORTS_TRANSACTIONS == 0 {
        return Err("Native NTFS transaction support is required by this Windows backend.".into());
    }
    Ok(())
}

pub(crate) fn canonical_path(path: &Path) -> Result<PathBuf, String> {
    let canonical = std::fs::canonicalize(path).map_err(|_| "Path is unavailable")?;
    #[cfg(windows)]
    {
        let text = canonical.to_str().ok_or("Unsupported path encoding")?;
        if let Some(unc) = text.strip_prefix("\\\\?\\UNC\\") {
            return Ok(PathBuf::from(format!("\\\\{unc}")));
        }
        if let Some(disk) = text.strip_prefix("\\\\?\\") {
            return Ok(PathBuf::from(disk));
        }
    }
    Ok(canonical)
}

pub(crate) fn physical_identity(path: &Path) -> Result<String, String> {
    crate::git::valid_path(path.to_str().ok_or("Unsupported path encoding")?, true)?;
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::symlink_metadata(path).map_err(|_| "Path is unavailable")?;
        Ok(format!("linux:{}:{}", metadata.dev(), metadata.ino()))
    }
    #[cfg(windows)]
    {
        let (volume, file) = crate::files::identity(path)?;
        Ok(format!("windows:{volume}:{file}"))
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    Err("Physical identity is unsupported on this platform.".into())
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub(crate) enum DestinationKey {
    Physical(String),
    Missing(PathBuf),
}

pub(crate) fn destination_key(path: &Path) -> Result<DestinationKey, String> {
    crate::git::valid_path(path.to_str().ok_or("Unsupported path encoding")?, false)?;
    match std::fs::symlink_metadata(path) {
        Ok(_) => physical_identity(path).map(DestinationKey::Physical),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            #[cfg(windows)]
            let path = PathBuf::from(
                path.to_str()
                    .ok_or("Unsupported path encoding")?
                    .to_lowercase(),
            );
            #[cfg(not(windows))]
            let path = path.to_path_buf();
            Ok(DestinationKey::Missing(path))
        }
        Err(_) => Err("Path is unavailable".into()),
    }
}

fn destination_keys_match(
    left: &Path,
    right: &Path,
    left_key: DestinationKey,
    right_key: DestinationKey,
) -> bool {
    match (left_key, right_key) {
        (DestinationKey::Physical(left), DestinationKey::Physical(right)) => left == right,
        (DestinationKey::Missing(_), DestinationKey::Missing(_)) => left == right,
        _ => false,
    }
}

pub(crate) fn same_destination(left: &Path, right: &Path) -> Result<bool, String> {
    Ok(destination_keys_match(
        left,
        right,
        destination_key(left)?,
        destination_key(right)?,
    ))
}

#[cfg(not(target_os = "linux"))]
#[tauri::command]
pub fn probe_root(root: String) -> RootSupport {
    probe_root_sync(root)
}

#[cfg(target_os = "linux")]
#[tauri::command]
pub async fn probe_root(root: String) -> RootSupport {
    probe_root_bounded(root, probe_root_sync).await
}

#[cfg(target_os = "linux")]
async fn probe_root_bounded(
    root: String,
    probe: impl FnOnce(String) -> RootSupport + Send + 'static,
) -> RootSupport {
    static SLOTS: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>> =
        std::sync::OnceLock::new();
    let Ok(permit) = SLOTS
        .get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(4)))
        .clone()
        .try_acquire_owned()
    else {
        return refused_root(
            root,
            "Linux root probes are busy. Retry after the current probes finish.",
        );
    };
    let failed_root = root.clone();
    match tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        probe(root)
    })
    .await
    {
        Ok(support) => support,
        Err(_) => refused_root(
            failed_root,
            "Linux root probe could not finish. Retry root selection.",
        ),
    }
}

#[cfg(target_os = "linux")]
fn refused_root(root: String, reason: &str) -> RootSupport {
    RootSupport {
        root,
        valid: false,
        reason: Some(reason.into()),
        identity: None,
        case_policy: "unknown",
        capabilities: refused_capabilities(reason),
    }
}

fn probe_root_sync(root: String) -> RootSupport {
    #[cfg(all(target_os = "linux", feature = "test-profile"))]
    if let Err(reason) = crate::test_profile::root_probe_delay(&root) {
        return refused_root(root, &reason);
    }

    match native_root(&root).and_then(|()| physical_identity(Path::new(&root))) {
        Ok(identity) => {
            let capabilities = root_capabilities(Path::new(&root));
            RootSupport {
                root,
                valid: true,
                reason: None,
                identity: Some(identity),
                case_policy: "unknown",
                capabilities,
            }
        }
        Err(reason) => RootSupport {
            root,
            valid: false,
            identity: None,
            case_policy: "unknown",
            capabilities: refused_capabilities(&reason),
            reason: Some(reason),
        },
    }
}

#[tauri::command]
pub fn path_identities(paths: Vec<String>) -> Vec<PathIdentity> {
    paths
        .into_iter()
        .map(|path| {
            let exists = std::fs::symlink_metadata(&path).is_ok();
            let identity =
                crate::git::valid_path(&path, false).and_then(
                    |()| match std::fs::symlink_metadata(&path) {
                        Ok(_) => physical_identity(Path::new(&path)).map(Some),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                        Err(_) => Err("Path is unavailable".into()),
                    },
                );
            match identity {
                Ok(identity) => PathIdentity {
                    path,
                    identity,
                    exists,
                    reason: None,
                },
                Err(reason) => PathIdentity {
                    path,
                    identity: None,
                    exists,
                    reason: Some(reason),
                },
            }
        })
        .collect()
}

#[cfg(test)]
pub(crate) struct Fixture(pub PathBuf);

#[cfg(test)]
impl Fixture {
    pub(crate) fn new(label: &str) -> Self {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("paperwing-09-{label}-{}-{id}", std::process::id()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .unwrap();
        }
        #[cfg(not(unix))]
        std::fs::create_dir(&path).unwrap();
        std::fs::write(
            path.join(".paperwing-test-root"),
            b"packet09 disposable fixture",
        )
        .unwrap();
        Self(path)
    }
}

#[cfg(test)]
impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(
            std::fs::read(self.0.join(".paperwing-test-root")).unwrap(),
            b"packet09 disposable fixture"
        );
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn linux_ipc_serialization_reports_native_reads_and_exact_write_refusals() {
        assert_eq!(
            serde_json::to_value(platform_info().await).unwrap(),
            serde_json::json!({
                "platform": "linux", "separator": "/", "credentials": serde_json::to_value(crate::credentials::capability().await).unwrap(), "capabilities": {
                    "readCompare": {"supported": true, "reason": null},
                    "edit": {"supported": false, "reason": "Linux editing requires the later recoverable write backend."},
                    "copy": {"supported": false, "reason": "Linux copying requires the later recoverable copy backend."},
                    "recovery": {"supported": false, "reason": "Linux recovery requires the later recovery backend."},
                    "trash": {"supported": true, "reason": null}
                }
            })
        );
        let fixture = Fixture::new("ipc");
        let root = fixture.0.to_str().unwrap().to_string();
        let identity = physical_identity(&fixture.0).unwrap();
        let volume = rustix::fs::fstatfs(std::fs::File::open(&fixture.0).unwrap()).unwrap();
        let capabilities = if volume.f_type == libc::EXT4_SUPER_MAGIC {
            serde_json::to_value(platform_info().await).unwrap()["capabilities"].clone()
        } else {
            serde_json::json!({
                "readCompare": {"supported": true, "reason": null},
                "edit": {"supported": false, "reason": "Linux writes currently require a proven local ext4 root"},
                "copy": {"supported": false, "reason": "Linux writes currently require a proven local ext4 root"},
                "recovery": {"supported": false, "reason": "Linux writes currently require a proven local ext4 root"},
                "trash": {"supported": false, "reason": "Linux writes currently require a proven local ext4 root"}
            })
        };
        assert_eq!(
            serde_json::to_value(probe_root(root.clone()).await).unwrap(),
            serde_json::json!({
                "root": root, "valid": true, "reason": null, "identity": identity, "casePolicy": "unknown",
                "capabilities": capabilities
            })
        );
        assert_eq!(
            serde_json::to_value(path_identities(vec![root.clone()])).unwrap(),
            serde_json::json!([
                {"path": root, "identity": identity, "exists": true, "reason": null}
            ])
        );
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn abandoned_root_probes_hold_the_four_task_limit_until_blocking_cleanup() {
        let fixture = Fixture::new("probe-limit");
        let root = fixture.0.to_str().unwrap().to_string();
        let mut releases = Vec::new();
        for _ in 0..4 {
            let (entered, ready) = tokio::sync::oneshot::channel();
            let (release, blocked) = std::sync::mpsc::channel();
            let task = tokio::spawn(probe_root_bounded(root.clone(), move |root| {
                let _ = entered.send(());
                blocked.recv().unwrap();
                probe_root_sync(root)
            }));
            ready.await.unwrap();
            task.abort();
            let _ = task.await;
            releases.push(release);
        }
        let refused = probe_root_bounded(root.clone(), |_| {
            panic!("Fifth blocking probe was admitted")
        })
        .await;
        assert!(!refused.valid);
        assert!(refused.reason.unwrap().contains("busy"));
        for release in releases {
            release.send(()).unwrap();
        }
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if probe_root(root.clone()).await.valid {
                break;
            }
            assert!(tokio::time::Instant::now() < deadline);
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    #[cfg(target_os = "linux")]
    #[tokio::test(flavor = "current_thread")]
    async fn stalled_linux_root_probe_leaves_runtime_responsive() {
        let fixture = Fixture::new("probe-responsive");
        let root = fixture.0.to_str().unwrap().to_string();
        let (entered, ready) = tokio::sync::oneshot::channel();
        let (started, start) = std::sync::mpsc::channel();
        let (tick, ticks) = std::sync::mpsc::channel();
        let (release, blocked) = std::sync::mpsc::channel();
        let controller = std::thread::spawn(move || {
            start.recv().unwrap();
            let responsive = ticks
                .recv_timeout(std::time::Duration::from_millis(500))
                .is_ok();
            release.send(()).unwrap();
            responsive
        });
        let probe = tokio::spawn(probe_root_bounded(root, move |root| {
            started.send(()).unwrap();
            let _ = entered.send(());
            blocked.recv().unwrap();
            probe_root_sync(root)
        }));
        ready.await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let _ = tick.send(());
        assert!(probe.await.unwrap().valid);
        assert!(controller.join().unwrap());
    }

    #[test]
    fn unsupported_windows_write_volume_preserves_native_read_capability() {
        let capabilities = Capabilities {
            read_compare: available(),
            edit: available(),
            copy: available(),
            recovery: available(),
            trash: available(),
        };
        let readonly =
            capabilities_with_write_support(capabilities.clone(), Err("Readonly volume".into()));
        assert_eq!(
            serde_json::to_value(readonly).unwrap(),
            serde_json::json!({
                "readCompare": {"supported":true, "reason":null},
                "edit": {"supported":false, "reason":"Readonly volume"},
                "copy": {"supported":false, "reason":"Readonly volume"},
                "recovery": {"supported":false, "reason":"Readonly volume"},
                "trash": {"supported":false, "reason":"Readonly volume"}
            })
        );
        assert_eq!(
            capabilities_with_write_support(capabilities.clone(), Ok(())),
            capabilities
        );
    }

    #[test]
    fn root_probes_refuse_missing_foreign_and_linked_roots_without_mutation() {
        let fixture = Fixture::new("roots");
        let before = std::fs::read_dir(&fixture.0).unwrap().count();
        let missing = fixture.0.join("missing");
        assert!(!probe_root_sync(missing.to_str().unwrap().into()).valid);
        let identity = path_identities(vec![missing.to_str().unwrap().into()]).remove(0);
        assert!(!identity.exists);
        assert_eq!(identity.identity, None);
        assert_eq!(identity.reason, None);
        assert_eq!(
            serde_json::to_value(identity).unwrap(),
            serde_json::json!({
                "path": missing.to_str().unwrap(), "identity": null, "exists": false, "reason": null
            })
        );
        #[cfg(target_os = "linux")]
        {
            let foreign = probe_root_sync("C:\\Dev\\repos".into());
            assert!(!foreign.valid);
            assert!(foreign.reason.unwrap().contains("explicitly reassign"));
            std::os::unix::fs::symlink(&fixture.0, fixture.0.join("linked")).unwrap();
            assert!(!probe_root_sync(fixture.0.join("linked").to_str().unwrap().into()).valid);
            assert!(path_identities(vec![fixture
                .0
                .join("linked/missing")
                .to_str()
                .unwrap()
                .into()])[0]
                .identity
                .is_none());
            std::fs::remove_file(fixture.0.join("linked")).unwrap();
        }
        assert_eq!(std::fs::read_dir(&fixture.0).unwrap().count(), before);
    }

    #[test]
    fn missing_destination_candidates_cannot_authorize_different_native_paths() {
        let upper = Path::new(r"C:\fixture\Folder");
        let lower = Path::new(r"C:\fixture\folder");
        let candidate = || DestinationKey::Missing(lower.to_path_buf());
        assert!(!destination_keys_match(
            upper,
            lower,
            candidate(),
            candidate()
        ));
        assert!(destination_keys_match(
            upper,
            upper,
            candidate(),
            candidate()
        ));
        assert!(destination_keys_match(
            upper,
            lower,
            DestinationKey::Physical("same-object".into()),
            DestinationKey::Physical("same-object".into())
        ));
        assert!(!destination_keys_match(
            upper,
            upper,
            DestinationKey::Physical("same-object".into()),
            candidate()
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn physical_aliases_share_identity_but_case_distinct_and_missing_paths_do_not() {
        let fixture = Fixture::new("identity");
        for name in ["Folder", "folder"] {
            std::fs::write(fixture.0.join(name), name).unwrap();
        }
        std::fs::hard_link(fixture.0.join("Folder"), fixture.0.join("alias")).unwrap();
        assert!(same_destination(&fixture.0.join("Folder"), &fixture.0.join("alias")).unwrap());
        assert!(!same_destination(&fixture.0.join("Folder"), &fixture.0.join("folder")).unwrap());
        let identities = path_identities(vec![
            fixture.0.join("Missing").to_str().unwrap().into(),
            fixture.0.join("missing").to_str().unwrap().into(),
        ]);
        assert!(identities
            .iter()
            .all(|identity| !identity.exists && identity.identity.is_none()));
        assert!(!same_destination(&fixture.0.join("Missing"), &fixture.0.join("missing")).unwrap());
        assert_eq!(
            probe_root_sync(fixture.0.to_str().unwrap().into()).case_policy,
            "unknown"
        );
        println!("Case-distinct native fixture verified; casefold-volume fixture unavailable on this host.");
    }
}
