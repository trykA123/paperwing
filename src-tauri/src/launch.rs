use crate::kernel::events::CoreEvent;
use crate::discover_job::chosen_folder;
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Runtime};

pub const LAUNCH_EVENT: &str = "launch-request";

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LaunchAction {
    OpenFolder {
        path: String,
    },
    #[serde(rename_all = "camelCase")]
    CompareFolders {
        left: String,
        right: String,
    },
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Ignored {
    pub arg: String,
    pub reason: String,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRequest {
    pub action: Option<LaunchAction>,
    pub ignored: Vec<Ignored>,
}

pub fn parse_args(args: &[String], cwd: &Path) -> LaunchRequest {
    let mut folders: Vec<String> = Vec::new();
    let mut ignored = Vec::new();
    for arg in args.iter().filter(|arg| !arg.is_empty()) {
        let arg = &unquote_drive_root(arg);
        match accept(arg, cwd, &folders) {
            Ok(path) => folders.push(path),
            Err(reason) => ignored.push(Ignored {
                arg: arg.clone(),
                reason,
            }),
        }
    }
    for extra in folders.split_off(folders.len().min(2)) {
        ignored.push(Ignored {
            arg: extra,
            reason: "Only two folders can be compared".into(),
        });
    }
    let mut folders = folders.into_iter();
    let action = match (folders.next(), folders.next()) {
        (Some(left), Some(right)) => Some(LaunchAction::CompareFolders { left, right }),
        (Some(path), None) => Some(LaunchAction::OpenFolder { path }),
        _ => None,
    };
    LaunchRequest { action, ignored }
}

fn unquote_drive_root(arg: &str) -> String {
    let bytes = arg.as_bytes();
    if cfg!(windows) && bytes.len() == 3 && bytes[0].is_ascii_alphabetic() && &arg[1..] == ":\"" {
        return format!("{}:\\", &arg[..1]);
    }
    arg.to_string()
}

fn accept(arg: &str, cwd: &Path, taken: &[String]) -> Result<String, String> {
    if arg.starts_with('-') {
        return Err("Unrecognised option".into());
    }
    let path = cwd.join(arg);
    let canonical =
        chosen_folder(path.to_str().ok_or("Unsupported path encoding")?).map_err(|reason| {
            match std::fs::metadata(&path) {
                Ok(metadata) if !metadata.is_dir() => "Only folders can be opened".to_string(),
                Err(_) => "Path does not exist".to_string(),
                Ok(_) => reason,
            }
        })?;
    let text = canonical
        .to_str()
        .ok_or("Unsupported path encoding")?
        .to_string();
    if taken.contains(&text) {
        return Err("Folder was listed twice".into());
    }
    Ok(text)
}

#[derive(Default)]
pub struct Pending(Mutex<Vec<LaunchRequest>>);

impl Pending {
    pub fn from_process() -> Self {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let cwd = std::env::current_dir().unwrap_or_default();
        let pending = Self::default();
        pending.push(parse_args(&args, &cwd));
        pending
    }

    fn push(&self, request: LaunchRequest) -> bool {
        if request.action.is_none() && request.ignored.is_empty() {
            return false;
        }
        self.0.lock().is_ok_and(|mut queue| {
            queue.push(request);
            true
        })
    }

    fn drain(&self) -> Vec<LaunchRequest> {
        self.0
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }
}

pub fn single_instance<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_single_instance::init(|app, argv, cwd| {
        let request = parse_args(argv.get(1..).unwrap_or_default(), Path::new(&cwd));
        if app.state::<Pending>().push(request) {
            let _ = crate::events::publish(app, CoreEvent::LaunchRequest);
        }
        focus_main(app);
    })
}

fn focus_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[tauri::command]
pub async fn launch_request(
    pending: tauri::State<'_, Pending>,
) -> Result<Vec<LaunchRequest>, String> {
    Ok(pending.drain())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Fixture;
    use std::fs;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string()).collect()
    }

    fn folders() -> (Fixture, String, String) {
        let fixture = Fixture::new("launch-args");
        fs::create_dir(fixture.0.join("one")).unwrap();
        fs::create_dir(fixture.0.join("two")).unwrap();
        fs::write(fixture.0.join("file.txt"), "x").unwrap();
        let one = crate::platform::canonical_path(&fixture.0.join("one"))
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let two = crate::platform::canonical_path(&fixture.0.join("two"))
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        (fixture, one, two)
    }

    #[test]
    fn no_arguments_is_a_normal_start() {
        let request = parse_args(&[], Path::new("/"));
        assert_eq!(
            request,
            LaunchRequest {
                action: None,
                ignored: vec![]
            }
        );
        assert!(!Pending::default().push(request));
    }

    #[test]
    fn one_folder_opens_it_resolving_relative_paths() {
        let (fixture, one, _) = folders();
        let request = parse_args(&strings(&["one"]), &fixture.0);
        assert_eq!(request.action, Some(LaunchAction::OpenFolder { path: one }));
    }

    #[test]
    fn two_folders_request_a_compare_in_order() {
        let (fixture, one, two) = folders();
        let request = parse_args(&strings(&[&two, &one]), &fixture.0);
        assert_eq!(
            request.action,
            Some(LaunchAction::CompareFolders {
                left: two,
                right: one
            })
        );
        assert!(request.ignored.is_empty());
    }

    #[test]
    fn files_missing_paths_and_options_are_ignored_with_reasons() {
        let (fixture, one, _) = folders();
        let request = parse_args(
            &strings(&["file.txt", "missing", "--flag", "one"]),
            &fixture.0,
        );
        assert_eq!(request.action, Some(LaunchAction::OpenFolder { path: one }));
        let reasons: Vec<_> = request
            .ignored
            .iter()
            .map(|item| item.reason.as_str())
            .collect();
        assert_eq!(
            reasons,
            [
                "Only folders can be opened",
                "Path does not exist",
                "Unrecognised option"
            ]
        );
    }

    #[test]
    fn repeated_and_surplus_folders_are_ignored() {
        let (fixture, one, two) = folders();
        fs::create_dir(fixture.0.join("three")).unwrap();
        let request = parse_args(&strings(&[&one, &one, &two, "three"]), &fixture.0);
        assert!(matches!(
            request.action,
            Some(LaunchAction::CompareFolders { .. })
        ));
        assert_eq!(request.ignored.len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_argument_is_ignored_with_a_reason() {
        let (fixture, one, _) = folders();
        std::os::unix::fs::symlink(&one, fixture.0.join("link")).unwrap();
        let request = parse_args(&strings(&["link"]), &fixture.0);
        assert_eq!(request.action, None);
        assert_eq!(request.ignored.len(), 1);
        assert!(request.ignored[0].reason.contains("Linked"));
    }

    #[cfg(windows)]
    #[test]
    fn a_verbatim_prefixed_argument_is_ignored_with_a_reason() {
        let (fixture, one, _) = folders();
        let verbatim = format!("\\\\?\\{one}");
        let request = parse_args(&strings(&[&verbatim]), &fixture.0);
        assert_eq!(request.action, None);
        assert_eq!(request.ignored.len(), 1);
        assert_eq!(request.ignored[0].arg, verbatim);
    }

    #[test]
    fn a_quoted_drive_root_argument_regains_its_separator_on_windows() {
        let fixed = unquote_drive_root("C:\"");
        assert_eq!(fixed, if cfg!(windows) { "C:\\" } else { "C:\"" });
        assert_eq!(unquote_drive_root("C:\\work"), "C:\\work");
    }

    #[test]
    fn pending_requests_drain_once() {
        let pending = Pending::default();
        let request = LaunchRequest {
            action: None,
            ignored: vec![Ignored {
                arg: "x".into(),
                reason: "y".into(),
            }],
        };
        assert!(pending.push(request.clone()));
        assert_eq!(pending.drain(), vec![request]);
        assert!(pending.drain().is_empty());
    }

    #[test]
    fn requests_serialise_as_tagged_camel_case() {
        let value = serde_json::to_value(LaunchRequest {
            action: Some(LaunchAction::CompareFolders {
                left: "a".into(),
                right: "b".into(),
            }),
            ignored: vec![],
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({ "action": { "kind": "compareFolders", "left": "a", "right": "b" }, "ignored": [] })
        );
    }
}
