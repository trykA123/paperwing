use super::{commands, filesystem, Environment, CONTRACT_WARNING};
use crate::{
    commit::{DiscardRecovery, DiscardWrite},
    linux_guard::{mutation::OperationAuthority, Error, Root},
    linux_journal::Journal,
    paths::ReadRoot,
    settings::Settings,
};

struct Authority<'a> {
    environment: &'a Environment,
    settings: &'a Settings,
    safe: &'a ReadRoot,
    file: &'a str,
}
impl OperationAuthority for Authority<'_> {
    fn refresh(&mut self) -> Result<(), Error> {
        self.environment
            .revalidate(self.settings)
            .map_err(|_| Error::conflict("Registered settings changed; discard refused"))?;
        self.safe
            .resolve_cached(self.file, false, &mut crate::paths::ReadCache::default())
            .map_err(|_| Error::conflict("Discard path changed or became unsafe"))?;
        self.check()
    }
    fn check(&self) -> Result<(), Error> {
        if crate::clone::busy() {
            return Err(Error::conflict("Git operation started; discard refused"));
        }
        Ok(())
    }
}

pub(crate) async fn discard_restore(
    app: tauri::AppHandle,
    write: DiscardWrite,
) -> Result<DiscardRecovery, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let environment = commands::environment(&app)?;
        tauri::async_runtime::block_on(replace(&environment, write))
    })
    .await
    .map_err(|_| "Discard could not finish; inspect Recovery before retrying")?
}

pub(super) async fn replace(
    environment: &Environment,
    write: DiscardWrite,
) -> Result<DiscardRecovery, String> {
    let settings = environment.load()?;
    let safe = crate::compare::registered_write_root(&settings, &write.root, &write.file).await?;
    let _filesystem = filesystem()?;
    write.index_state.validate().await?;
    environment.revalidate(&settings)?;
    let value = safe.linux_value()?;
    let root = Root::reopen(&value).map_err(|error| error.to_string())?;
    let plan = root
        .preview_parents(&write.file)
        .map_err(|error| error.to_string())?;
    if !plan.missing.is_empty() {
        return Err("Discard does not create missing parent directories".into());
    }
    let expected = root
        .parent(&write.file, false)
        .and_then(|parent| parent.snapshot())
        .map_err(|error| error.to_string())?;
    if expected.bytes() != write.expected.as_deref() {
        return Err("File changed since the diff was read; refresh before discarding".into());
    }
    let journal = Journal::open_guarded(&environment.app_data, &[value])
        .map_err(|error| error.to_string())?;
    let mut authority = Authority {
        environment,
        settings: &settings,
        safe: &safe,
        file: &write.file,
    };
    let published = journal
        .replace_with_parents(&plan, &expected, &write.bytes, &mut authority)
        .map_err(|failure| {
            format!(
                "{}; recovery record: {}; applied: {}",
                failure.error,
                failure.error.record.as_deref().unwrap_or("none"),
                failure.error.applied
            )
        })?;
    Ok(DiscardRecovery {
        id: published.file.record,
        warning: Some(CONTRACT_WARNING.into()),
    })
}

#[cfg(test)]
mod tests;
