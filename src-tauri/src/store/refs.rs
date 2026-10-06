use super::error::Error;
use rusqlite::{params, Connection, OptionalExtension};

pub const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq)]
pub struct RefEntry {
    pub name: String,
    pub sha: Option<String>,
    pub label: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RefSet {
    pub url: String,
    pub scope: String,
    pub ref_epoch: u64,
    pub fetched_at: u64,
    pub branches: Vec<RefEntry>,
    pub tags: Vec<RefEntry>,
}

pub fn put(connection: &mut Connection, set: &RefSet) -> Result<(), Error> {
    let transaction = connection.transaction()?;
    remove(&transaction, &set.url)?;
    transaction.execute(
        "INSERT INTO ref_sets (url, scope, ref_epoch, fetched_at, version)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            set.url,
            set.scope,
            super::seconds(set.ref_epoch),
            super::seconds(set.fetched_at),
            VERSION
        ],
    )?;
    let mut insert = transaction.prepare(
        "INSERT INTO refs (url, kind, position, name, sha, label) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for (kind, entries) in [("branch", &set.branches), ("tag", &set.tags)] {
        for (position, entry) in entries.iter().enumerate() {
            insert.execute(params![
                set.url,
                kind,
                position as i64,
                entry.name,
                entry.sha,
                entry.label
            ])?;
        }
    }
    drop(insert);
    Ok(transaction.commit()?)
}

pub fn get(connection: &Connection, url: &str) -> Result<Option<RefSet>, Error> {
    let header = connection
        .query_row(
            "SELECT scope, ref_epoch, fetched_at, version FROM ref_sets WHERE url = ?1",
            [url],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, u32>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((scope, epoch, fetched_at, version)) = header else {
        return Ok(None);
    };
    if version != VERSION {
        return Ok(None);
    }
    let mut statement = connection.prepare(
        "SELECT name, sha, label FROM refs WHERE url = ?1 AND kind = ?2 ORDER BY position",
    )?;
    let mut entries = |kind: &str| -> Result<Vec<RefEntry>, Error> {
        let rows = statement.query_map(params![url, kind], |row| {
            Ok(RefEntry {
                name: row.get(0)?,
                sha: row.get(1)?,
                label: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    };
    Ok(Some(RefSet {
        url: url.to_string(),
        scope,
        ref_epoch: u64::try_from(epoch).unwrap_or_default(),
        fetched_at: u64::try_from(fetched_at).unwrap_or(u64::MAX),
        branches: entries("branch")?,
        tags: entries("tag")?,
    }))
}

pub fn remove(connection: &Connection, url: &str) -> Result<(), Error> {
    connection.execute("DELETE FROM ref_sets WHERE url = ?1", [url])?;
    Ok(())
}
