#[cfg(feature = "benchmark")]
mod benchmark;
#[cfg(feature = "diagnostics")]
mod diagnostics;
#[cfg(feature = "test-profile")]
mod test_profile;
#[cfg(any(test, feature = "test-profile"))]
mod env_names;
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
mod search;
mod search_grep;
mod search_job;
mod search_rows;
mod search_service;
mod trash;
#[cfg(test)]
mod test_support;

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
    let builder = tauri::Builder::default()
        .plugin(launch::single_instance())
        .manage(launch::Pending::from_process())
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
                tauri::async_runtime::block_on(webview.state::<compare::Service>().release_sessions());
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
            app.manage(store::Store::start(app.path().app_data_dir()?, app.path().app_cache_dir().ok()));
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
            #[cfg(feature = "diagnostics")] diagnostics::diagnostics_status,
            #[cfg(feature = "diagnostics")] diagnostics::diagnostics_preview,
            #[cfg(feature = "diagnostics")] diagnostics::diagnostics_cancel,
            #[cfg(feature = "diagnostics")] diagnostics::diagnostics_export,
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
            github::pulls::pull_for_branch,
            github::pulls::open_pull_request,
            github::releases::create_github_release,
            git::get_refs_many,
            git::activity_snapshot,
            git::clear_activity,
            git::cancel_activity,
            git::repository_tree,
            history::repository_history,
            commit::repo_changes,
            commit::change_content,
            commit::stage_paths,
            commit::unstage_paths,
            commit::commit_staged,
            commit::create_branch,
            commit::push_branch,
            commit::delete_branch,
            tags::list_tags,
            tags::create_tag,
            tags::push_tag,
            tags::delete_tag,
            tags::delete_remote_tag,
            stash::stash_list,
            stash::stash_push,
            stash::stash_apply,
            stash::stash_pop,
            stash::stash_drop,
            stash::stash_show,
            stash::switch_with_stash,
            branch_cleanup::merged_branches,
            branch_cleanup::delete_merged_branches,
            branch_cleanup::delete_remote_branches,
            trash::trash_set_folders,
            compare::comparison_open,
            compare::comparison_refresh,
            compare::comparison_close,
            compare::comparison_cancel,
            compare::comparison_files,
            compare::comparison_content,
            compare::comparison_commits,
            #[cfg(windows)] files::file_edit_open,
            #[cfg(target_os = "linux")] linux_files::file_edit_open,
            #[cfg(not(any(windows, target_os = "linux")))] unsupported_files::file_edit_open,
            #[cfg(windows)] files::file_edit_close,
            #[cfg(target_os = "linux")] linux_files::file_edit_close,
            #[cfg(not(any(windows, target_os = "linux")))] unsupported_files::file_edit_close,
            #[cfg(windows)] files::file_save,
            #[cfg(target_os = "linux")] linux_files::file_save,
            #[cfg(not(any(windows, target_os = "linux")))] unsupported_files::file_save,
            #[cfg(windows)] files::copy_preview,
            #[cfg(target_os = "linux")] linux_files::copy_preview,
            #[cfg(not(any(windows, target_os = "linux")))] unsupported_files::copy_preview,
            #[cfg(windows)] files::copy_apply,
            #[cfg(target_os = "linux")] linux_files::copy_apply,
            #[cfg(not(any(windows, target_os = "linux")))] unsupported_files::copy_apply,
            #[cfg(windows)] files::copy_cancel,
            #[cfg(target_os = "linux")] linux_files::copy_cancel,
            #[cfg(not(any(windows, target_os = "linux")))] unsupported_files::copy_cancel,
            #[cfg(windows)] files::recovery_list,
            #[cfg(target_os = "linux")] linux_files::recovery_list,
            #[cfg(not(any(windows, target_os = "linux")))] unsupported_files::recovery_list,
            #[cfg(windows)] files::recovery_undo,
            #[cfg(target_os = "linux")] linux_files::recovery_undo,
            #[cfg(not(any(windows, target_os = "linux")))] unsupported_files::recovery_undo,
            #[cfg(windows)] files::recovery_cleanup,
            #[cfg(target_os = "linux")] linux_files::recovery_cleanup,
            #[cfg(not(any(windows, target_os = "linux")))] unsupported_files::recovery_cleanup,
            #[cfg(windows)] files::recovery_resolve,
            #[cfg(target_os = "linux")] linux_files::recovery_resolve,
            #[cfg(not(any(windows, target_os = "linux")))] unsupported_files::recovery_resolve,
            clone::start_clone,
            local::local_status,
            paths_exist,
            platform::platform_info,
            platform::probe_root,
            platform::path_identities,
            open_in_vscode,
            launch::launch_request,
            discover_job::discover_start,
            discover_job::discover_cancel,
            discover_job::discover_cancel_all,
            search_service::search_start,
            search_service::search_cancel,
            search_service::search_cancel_all,
            search_service::search_capabilities,
        ])
        .build(context)
        .expect("error while building Skein")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                app.state::<store::Store>().mark_clean();
            }
        });
}
