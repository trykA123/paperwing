mod clone;
mod commit;
mod compare;
mod file_guard;
mod files;
mod git;
mod github;
mod local;
mod paths;
mod settings;
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
            settings::load_settings,
            settings::save_settings,
            settings::set_token,
            settings::has_token,
            settings::delete_token,
            github::test_source,
            github::list_user_orgs,
            github::list_repos,
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
            #[cfg(windows)] files::file_edit_close,
            #[cfg(windows)] files::file_save,
            #[cfg(windows)] files::copy_preview,
            #[cfg(windows)] files::copy_apply,
            #[cfg(windows)] files::copy_cancel,
            #[cfg(windows)] files::recovery_list,
            #[cfg(windows)] files::recovery_undo,
            #[cfg(windows)] files::recovery_cleanup,
            #[cfg(windows)] files::recovery_resolve,
            clone::start_clone,
            local::local_status,
            paths_exist,
            open_in_vscode,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PaperWing");
}
