use super::Error;
use rusqlite::{params, Connection, OptionalExtension};

const VERSION: u32 = 1;

#[derive(Clone)]
pub(crate) struct Key {
    pub host: String,
    pub owner: String,
    pub repository: String,
    pub base: String,
    pub head: String,
}

#[derive(Clone)]
pub(crate) struct Access {
    pub source_id: String,
    pub scope: String,
}

pub(crate) struct Entry {
    pub key: Key,
    pub body: String,
    pub fetched_at: u64,
}

pub(crate) fn get(
    connection: &Connection,
    key: &Key,
    access: &Access,
) -> Result<Option<String>, Error> {
    Ok(connection
        .query_row(
            "SELECT c.body FROM github_comparisons c
         JOIN github_comparison_scopes s ON s.comparison_id = c.id
         WHERE c.host = ?1 AND c.owner = ?2 AND c.repository = ?3
         AND c.base_sha = ?4 AND c.head_sha = ?5 AND c.version = ?6
         AND s.source_id = ?7 AND s.scope = ?8",
            params![
                key.host,
                key.owner,
                key.repository,
                key.base,
                key.head,
                VERSION,
                access.source_id,
                access.scope
            ],
            |row| row.get(0),
        )
        .optional()?)
}

pub(crate) fn put(
    connection: &mut Connection,
    entry: &Entry,
    access: &Access,
) -> Result<(), Error> {
    #[cfg(test)]
    tests::writing(&access.source_id);
    let transaction = connection.transaction()?;
    let id = insert_result(&transaction, entry)?;
    transaction.execute(
        "INSERT INTO github_comparison_scopes (comparison_id, source_id, scope)
         VALUES (?1, ?2, ?3) ON CONFLICT (comparison_id, source_id) DO UPDATE SET scope = excluded.scope",
        params![id, access.source_id, access.scope],
    )?;
    Ok(transaction.commit()?)
}

fn insert_result(connection: &Connection, entry: &Entry) -> Result<i64, Error> {
    let key = &entry.key;
    connection.execute(
        "INSERT OR IGNORE INTO github_comparisons
         (host, owner, repository, base_sha, head_sha, fetched_at, version, body)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            key.host,
            key.owner,
            key.repository,
            key.base,
            key.head,
            super::seconds(entry.fetched_at),
            VERSION,
            entry.body
        ],
    )?;
    let id: i64 = connection.query_row(
        "SELECT id FROM github_comparisons WHERE host = ?1 AND owner = ?2
         AND repository = ?3 AND base_sha = ?4 AND head_sha = ?5",
        params![key.host, key.owner, key.repository, key.base, key.head],
        |row| row.get(0),
    )?;
    Ok(id)
}

pub(crate) fn remove_source(connection: &Connection, source_id: &str) -> Result<(), Error> {
    connection.execute(
        "DELETE FROM github_comparison_scopes WHERE source_id = ?1",
        [source_id],
    )?;
    connection.execute("DELETE FROM github_comparisons WHERE id NOT IN (SELECT comparison_id FROM github_comparison_scopes)", [])?;
    Ok(())
}

pub(crate) fn remove_set(connection: &Connection, id: i64) -> Result<(), Error> {
    connection.execute("DELETE FROM github_comparisons WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
