#[cfg(feature = "benchmark")]
mod benchmark;
#[cfg(feature = "diagnostics")]
mod diagnostics;
#[cfg(feature = "test-profile")]
mod test_profile;
#[cfg(any(test, feature = "test-profile"))]
mod env_names;
mod commands;
pub mod kernel;
mod events;
mod providers;
mod clone;
mod branch_cleanup;
mod commit;
mod tags;
mod compare;
mod file_guard;
#[cfg(target_os = "linux")]
mod linux_guard;
#[cfg(target_os = "linux")]
mod linux_journal;
#[cfg(target_os = "linux")]
mod linux_diff;
mod files;
mod git;
mod github;
mod history;
mod local;
mod ordered;
mod object_id;
mod paths;
mod platform;
#[cfg(not(any(windows, target_os = "linux")))]
mod unsupported_files;
#[cfg(target_os = "linux")]
mod linux_files;
mod settings;
mod stash;
mod store;
mod credentials;
mod discover;
mod discover_job;
mod launch;
mod finder;
mod finder_job;
mod finder_service;
mod search;
mod search_builtin;
mod search_engine;
mod search_files;
mod search_pattern;
mod search_grep;
mod search_job;
mod search_rows;
mod search_service;
mod search_walk;
mod trash;
#[cfg(test)]
mod test_support;

use std::path::Path;
use tauri::Manager;

#[tauri::command]
async fn open_in_vscode(path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || open_editor(&path)).await
        .map_err(|_| "Could not open VS Code".to_string())?
}

fn open_editor(path: &str) -> Result<(), String> {
    git::valid_path(path, true)?;
    let dir = Path::new(path);
    if !dir.is_dir() {
        return Err(format!("{path} does not exist yet"));
    }
    let mut cmd = std::process::Command::new(if cfg!(windows) { "code.cmd" } else { "code" });
    cmd.arg(dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not start VS Code (is `code` on PATH?): {e}"))
}

pub fn run() {
    let context = tauri::generate_context!();
    #[cfg(feature = "test-profile")]
    test_profile::information(&context.config().identifier);
    #[cfg(feature = "test-profile")]
    if let Err(error) = test_profile::preflight(&context.config().identifier) {
        eprintln!("Test profile refused: {error}");
        std::process::exit(2);
    }
    #[cfg(feature = "test-profile")]
    credentials::drill();
    let builder = tauri::Builder::default()
        .plugin(launch::single_instance())
        .manage(launch::Pending::from_process())
        .manage(settings::Startup::default())
        .manage(discover_job::Service::default())
        .manage(search_service::Service::default())
        .manage(compare::Service::default());
    #[cfg(windows)]
    let builder = builder.manage(files::Service::default());
    #[cfg(target_os = "linux")]
    let builder = builder.manage(linux_files::Service::default());
    builder
        .on_page_load(|webview, payload| {
            if webview.label() == "main" && matches!(payload.event(), tauri::webview::PageLoadEvent::Started) {
                let closing = webview.state::<compare::Service>().release_sessions();
                tauri::async_runtime::spawn(closing);
                #[cfg(windows)]
                tauri::async_runtime::block_on(webview.state::<files::Service>().release_tickets());
                #[cfg(target_os = "linux")]
                tauri::async_runtime::block_on(webview.state::<linux_files::Service>().release_tickets());
            }
        })
        .setup(|app| {
            #[cfg(feature = "diagnostics")]
            app.manage(diagnostics::Service::start()?);
            #[cfg(target_os = "linux")]
            app.state::<compare::Service>().configure_diff(app.path().app_data_dir()?)?;
            #[cfg(feature = "test-profile")]
            {
                let webview = test_profile::validate(app.handle())?;
                let (trace, sample) = test_profile::trace()?;
                benchmark::initialize(&trace, sample)?;
                let config = app.config().app.windows.first().ok_or("Test window configuration missing")?;
                tauri::WebviewWindowBuilder::from_config(app.handle(), config)?
                    .data_directory(webview).build()?;
            }
            events::install(app.handle());
            let store = store::Store::start(app.path().app_data_dir()?, app.path().app_cache_dir().ok());
            providers::install(app.handle(), store.clone());
            app.manage(store);
            let handle = app.handle().clone();
            if let Err(error) = tauri::async_runtime::block_on(tauri::async_runtime::spawn_blocking(move || settings::initialize(handle)))
                .map_err(|error| error.to_string()).and_then(|result| result) {
                settings::initialization_failed(app.handle(), error);
            }
            git::attach(app.handle().clone());
            #[cfg(windows)]
            if let Some(window) = app.get_webview_window("main") {
                let icon = app.default_window_icon().ok_or("Bundled application icon is missing")?;
                window.set_icon(icon.clone())?;
            }
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(commands::compose())
        .build(context)
        .expect("error while building Skein")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                app.state::<store::Store>().mark_clean();
            }
        });
}
