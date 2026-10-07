use super::{commits, listings, Error, Store};
use rusqlite::{Connection, OptionalExtension};
use std::collections::HashMap;

pub(crate) fn enabled(connection: &Connection, source_id: &str) -> Result<bool, Error> {
    Ok(connection
        .query_row(
            "SELECT enabled FROM providers WHERE source_id = ?1",
            [source_id],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(true))
}

pub(crate) fn flags(store: &Store) -> Result<HashMap<String, bool>, Error> {
    store.read_blocking(|connection| {
        let mut query = connection.prepare("SELECT source_id, enabled FROM providers")?;
        let flags = query
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        Ok(flags)
    })
}

pub(crate) fn save(
    store: &Store,
    flags: Vec<(String, bool)>,
    urls: Vec<(String, String)>,
) -> Result<(), Error> {
    store.enqueue(move |connection| {
        let transaction = connection.transaction()?;
        for (source_id, enabled) in flags {
            transaction.execute("INSERT INTO providers(source_id, enabled) VALUES (?1, ?2) ON CONFLICT(source_id) DO UPDATE SET enabled = excluded.enabled", (&source_id, enabled))?;
            if !enabled {
                transaction.execute("DELETE FROM ref_sets WHERE url IN (SELECT url FROM repositories WHERE source_id = ?1)", [&source_id])?;
                for (_, url) in urls.iter().filter(|(id, _)| id == &source_id) {
                    transaction.execute("DELETE FROM ref_sets WHERE url = ?1", [url])?;
                }
                listings::remove(&transaction, &source_id)?;
                commits::remove_source(&transaction, &source_id)?;
            }
        }
        Ok(transaction.commit()?)
    })?.blocking_recv().map_err(|_| Error::Unavailable)?
}
