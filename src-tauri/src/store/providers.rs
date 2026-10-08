use super::{commits, comparisons, listings, Error, Store};
use std::collections::HashSet;
use std::sync::MutexGuard;

pub(crate) fn disabled_sources(store: &Store) -> Result<MutexGuard<'_, HashSet<String>>, Error> {
    store
        .disabled_sources
        .lock()
        .map_err(|_| Error::Unavailable)
}

pub(crate) fn configure(store: &Store, flags: Vec<(String, bool)>) -> Result<Vec<String>, Error> {
    let mut disabled = disabled_sources(store)?;
    let next: HashSet<_> = flags
        .into_iter()
        .filter_map(|(id, enabled)| (!enabled).then_some(id))
        .collect();
    let newly_disabled = next.difference(&disabled).cloned().collect();
    *disabled = next;
    Ok(newly_disabled)
}

pub(crate) fn purge(
    store: &Store,
    sources: Vec<String>,
    urls: Vec<(String, String)>,
) -> Result<(), Error> {
    let admission = store.clone();
    store.enqueue(move |connection| {
        let disabled = disabled_sources(&admission)?;
        let transaction = connection.transaction()?;
        for source_id in sources.into_iter().filter(|id| disabled.contains(id)) {
            transaction.execute("DELETE FROM ref_sets WHERE url IN (SELECT url FROM repositories WHERE source_id = ?1)", [&source_id])?;
            for (_, url) in urls.iter().filter(|(id, _)| id == &source_id) {
                transaction.execute("DELETE FROM ref_sets WHERE url = ?1", [url])?;
            }
            listings::remove(&transaction, &source_id)?;
            commits::remove_source(&transaction, &source_id)?;
            comparisons::remove_source(&transaction, &source_id)?;
        }
        Ok(transaction.commit()?)
    })?.blocking_recv().map_err(|_| Error::Unavailable)?
}

#[cfg(test)]
pub(crate) fn save(
    store: &Store,
    flags: Vec<(String, bool)>,
    urls: Vec<(String, String)>,
) -> Result<(), Error> {
    let disabled = configure(store, flags)?;
    purge(store, disabled, urls)
}
