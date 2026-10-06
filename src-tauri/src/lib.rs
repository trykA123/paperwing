#[cfg(feature = "benchmark")]
mod benchmark;
#[cfg(feature = "test-profile")]
mod test_profile;
mod clone;
mod commit;
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
mod local;
mod paths;
mod platform;
#[cfg(not(windows))]
mod unsupported_files;
mod settings;
mod credentials;
mod trash;

use std::path::Path;
use tauri::Manager;

#[tauri::command]
fn paths_exist(paths: Vec<String>) -> Vec<bool> {
    paths.iter().map(|p| Path::new(p).exists()).collect()
}

#[tauri::command]
fn open_in_vscode(path: String) -> Result<(), String> {
    let dir = Path::new(&path);
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
    let builder = tauri::Builder::default().manage(compare::Service::default());
    #[cfg(windows)]
    let builder = builder.manage(files::Service::default());
    builder
        .on_page_load(|webview, payload| {
            if webview.label() == "main" && matches!(payload.event(), tauri::webview::PageLoadEvent::Started) {
                tauri::async_runtime::block_on(webview.state::<compare::Service>().release_sessions());
                #[cfg(windows)]
                tauri::async_runtime::block_on(webview.state::<files::Service>().release_tickets());
            }
        })
        .setup(|app| {
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
        .invoke_handler(tauri::generate_handler![
            #[cfg(feature = "benchmark")] benchmark::benchmark_record,
            #[cfg(feature = "benchmark")] benchmark::benchmark_snapshot,
            #[cfg(feature = "test-profile")] test_profile::benchmark_plan,
            #[cfg(feature = "test-profile")] test_profile::benchmark_finish,
            settings::load_settings,
            settings::save_settings,
            settings::set_token,
            settings::has_token,
            settings::delete_token,
            credentials::credential_status,
            credentials::source_revision,
            github::test_source,
            github::list_user_orgs,
            github::list_repos,
            github::list_cached_repos,
            github::get_commits,
            git::get_refs_many,
            git::activity_snapshot,
            git::clear_activity,
            git::cancel_activity,
            git::repository_tree,
            commit::repo_changes,
            commit::change_content,
            commit::stage_paths,
            commit::unstage_paths,
            commit::commit_staged,
            commit::create_branch,
            commit::push_branch,
            commit::delete_branch,
            trash::trash_set_folders,
            compare::comparison_open,
            compare::comparison_refresh,
            compare::comparison_close,
            compare::comparison_cancel,
            compare::comparison_files,
            compare::comparison_content,
            compare::comparison_commits,
            #[cfg(windows)] files::file_edit_open,
            #[cfg(not(windows))] unsupported_files::file_edit_open,
            #[cfg(windows)] files::file_edit_close,
            #[cfg(not(windows))] unsupported_files::file_edit_close,
            #[cfg(windows)] files::file_save,
            #[cfg(not(windows))] unsupported_files::file_save,
            #[cfg(windows)] files::copy_preview,
            #[cfg(not(windows))] unsupported_files::copy_preview,
            #[cfg(windows)] files::copy_apply,
            #[cfg(not(windows))] unsupported_files::copy_apply,
            #[cfg(windows)] files::copy_cancel,
            #[cfg(not(windows))] unsupported_files::copy_cancel,
            #[cfg(windows)] files::recovery_list,
            #[cfg(not(windows))] unsupported_files::recovery_list,
            #[cfg(windows)] files::recovery_undo,
            #[cfg(not(windows))] unsupported_files::recovery_undo,
            #[cfg(windows)] files::recovery_cleanup,
            #[cfg(not(windows))] unsupported_files::recovery_cleanup,
            #[cfg(windows)] files::recovery_resolve,
            #[cfg(not(windows))] unsupported_files::recovery_resolve,
            clone::start_clone,
            local::local_status,
            paths_exist,
            platform::platform_info,
            platform::probe_root,
            platform::path_identities,
            open_in_vscode,
        ])
        .run(context)
        .expect("error while running PaperWing");
}
