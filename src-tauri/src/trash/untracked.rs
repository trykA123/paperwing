use crate::commit::DiscardWrite;

pub(crate) async fn trash_untracked(
    app: tauri::AppHandle,
    write: DiscardWrite,
) -> Result<String, String> {
    #[cfg(any(windows, target_os = "linux"))]
    return tauri::async_runtime::spawn_blocking(move || {
        tauri::async_runtime::block_on(recycle(app, write))
    })
    .await
    .map_err(|_| "Trash move could not finish; inspect the Recycle Bin or Trash")?;
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (app, write);
        Err(
            "The Recycle Bin or desktop Trash is unavailable on this platform; file retained"
                .into(),
        )
    }
}

#[cfg(any(windows, target_os = "linux"))]
async fn recycle(app: tauri::AppHandle, write: DiscardWrite) -> Result<String, String> {
    let DiscardWrite {
        root,
        file,
        expected,
        index_state,
        ..
    } = write;
    let expected = expected.ok_or("Untracked file is missing")?;
    let settings = crate::settings::load_settings(app.clone())?;
    let safe = crate::compare::registered_write_root(&settings, &root, &file).await?;
    let _filesystem = crate::git::filesystem_gate()
        .try_write()
        .map_err(|_| "Git or another write is running; retry discard after it finishes")?;
    crate::commit::idle_check()?;
    index_state.validate().await?;
    if serde_json::to_value(crate::settings::load_settings(app)?)
        .map_err(|error| error.to_string())?
        != serde_json::to_value(&settings).map_err(|error| error.to_string())?
    {
        return Err("Settings changed during discard; file retained".into());
    }
    let bytes = safe.read(&file)?.ok_or("Untracked file is missing")?;
    if bytes.bytes != expected {
        return Err("File changed since the diff was read; file retained".into());
    }
    #[cfg(windows)]
    {
        Err("This Windows recycler cannot guarantee a Recycle Bin move; file retained. Untracked discard is unavailable until the shared recycler supports recycle-only deletion.".into())
    }
    #[cfg(target_os = "linux")]
    {
        let linux_root = crate::linux_guard::Root::reopen(&safe.linux_value()?)
            .map_err(|error| error.to_string())?;
        super::linux::recycle_file(&linux_root, &file, &expected, &super::linux::data_home()?)?;
        if index_state.intent_to_add() {
            let root_str = root.to_str().ok_or("Unsupported repository path")?;
            crate::commit::run(
                root_str,
                &["rm", "--cached", "--quiet", "--", &file],
                "Remove intent-to-add index entry",
                &[0],
                crate::git::OutputPolicy::Metadata,
                None,
                std::time::Duration::from_secs(45),
            )
            .await?;
        }
        Ok("Moved to the desktop Trash; restore it from Trash".into())
    }
}
