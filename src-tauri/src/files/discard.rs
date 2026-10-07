use super::*;
use crate::commit::{DiscardRecovery, DiscardWrite};

pub(crate) async fn discard_restore(
    app: tauri::AppHandle,
    write: DiscardWrite,
) -> Result<DiscardRecovery, String> {
    tauri::async_runtime::spawn_blocking(move || {
        tauri::async_runtime::block_on(replace(app, write))
    })
    .await
    .map_err(|_| "Discard could not finish; inspect Recovery before retrying")?
}

async fn replace(app: tauri::AppHandle, write: DiscardWrite) -> Result<DiscardRecovery, String> {
    if !ENABLE_WRITES {
        return Err("Write safety review is pending".into());
    }
    let settings = crate::settings::load_settings(app.clone())?;
    let safe = crate::compare::registered_write_root(&settings, &write.root, &write.file).await?;
    let root_identity = identity(&write.root)?;
    let _filesystem = crate::git::filesystem_gate()
        .try_write()
        .map_err(|_| "Git or another write is running; retry discard after it finishes")?;
    crate::commit::idle_check()?;
    write.index_state.validate().await?;
    if serde_json::to_value(crate::settings::load_settings(app.clone())?)
        .map_err(|error| error.to_string())?
        != serde_json::to_value(&settings).map_err(|error| error.to_string())?
    {
        return Err("Settings changed during discard; retry".into());
    }
    safe.resolve_cached(&write.file, false, &mut crate::paths::ReadCache::default())?;
    let _metadata = safe
        .metadata_locations()
        .iter()
        .filter(|path| path.is_dir())
        .map(|path| crate::file_guard::pin_metadata(path))
        .collect::<Result<Vec<_>, _>>()?;
    let directory = recovery_directory(&app)?;
    if fs::canonicalize(&directory)
        .map_err(io)?
        .starts_with(fs::canonicalize(&write.root).map_err(io)?)
    {
        return Err("Recovery storage must be outside the repository".into());
    }
    let record = Journal::open(&directory)?.replace_authorized(
        &write.root,
        &write.file,
        write.expected.as_deref(),
        &write.bytes,
        WritePolicy {
            identity: Some(root_identity),
            recovery: false,
        },
    )?;
    Ok(DiscardRecovery {
        id: record.id,
        warning: record.warning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discarded_crlf_file_with_spaces_can_be_undone_byte_for_byte() {
        let base = crate::test_support::tmp_root().join(format!("discard-windows-{}", unique()));
        fs::create_dir_all(base.join("repo")).unwrap();
        fs::create_dir(base.join("recovery")).unwrap();
        let root = base.join("repo");
        let before = b"before\r\nlast";
        let after = b"index\r\nlast";
        fs::write(root.join("with spaces.txt"), before).unwrap();
        let mut journal = Journal::open(&base.join("recovery")).unwrap();
        let record = journal
            .replace_authorized(
                &root,
                "with spaces.txt",
                Some(before),
                after,
                WritePolicy::default(),
            )
            .unwrap();
        assert_eq!(fs::read(root.join("with spaces.txt")).unwrap(), after);
        assert_eq!(record.stage, "applied");
        assert_eq!(journal.undo(&record.id).unwrap().stage, "undone");
        assert_eq!(fs::read(root.join("with spaces.txt")).unwrap(), before);
        drop(journal);
        fs::remove_dir_all(base).unwrap();
    }
}
