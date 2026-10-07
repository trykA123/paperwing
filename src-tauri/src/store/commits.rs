use super::error::Error;
use rusqlite::Connection;
#[cfg(test)]
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

#[cfg(test)]
pub const VERSION: u32 = 1;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct CommitRow {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub date: String,
    pub parents: Vec<String>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub struct CommitSet {
    pub source_id: String,
    pub scope: String,
    pub repository: String,
    pub branch: String,
    pub ref_epoch: u64,
    pub fetched_at: u64,
    pub commits: Vec<CommitRow>,
}

#[cfg(test)]
pub fn put(connection: &mut Connection, set: &CommitSet) -> Result<(), Error> {
    let transaction = connection.transaction()?;
    let epoch = super::seconds(set.ref_epoch);
    transaction.execute(
        "DELETE FROM commit_sets
         WHERE source_id = ?1 AND repository = ?2 AND branch = ?3 AND ref_epoch = ?4",
        params![set.source_id, set.repository, set.branch, epoch],
    )?;
    transaction.execute(
        "INSERT INTO commit_sets (source_id, scope, repository, branch, ref_epoch, fetched_at, version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            set.source_id,
            set.scope,
            set.repository,
            set.branch,
            epoch,
            super::seconds(set.fetched_at),
            VERSION
        ],
    )?;
    let set_id = transaction.last_insert_rowid();
    let mut insert = transaction.prepare(
        "INSERT INTO commits (set_id, position, sha, subject, message, author, date, parents)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for (position, commit) in set.commits.iter().enumerate() {
        let subject = commit.message.lines().next().unwrap_or_default();
        let parents = serde_json::to_string(&commit.parents)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        insert.execute(params![
            set_id,
            position as i64,
            commit.sha,
            subject,
            commit.message,
            commit.author,
            commit.date,
            parents
        ])?;
    }
    drop(insert);
    Ok(transaction.commit()?)
}

#[cfg(test)]
pub struct Key<'a> {
    pub source_id: &'a str,
    pub repository: &'a str,
    pub branch: &'a str,
    pub ref_epoch: u64,
}

#[cfg(test)]
pub fn get(connection: &Connection, key: &Key) -> Result<Option<CommitSet>, Error> {
    let header = connection
        .query_row(
            "SELECT id, scope, fetched_at, version FROM commit_sets
             WHERE source_id = ?1 AND repository = ?2 AND branch = ?3 AND ref_epoch = ?4",
            params![
                key.source_id,
                key.repository,
                key.branch,
                super::seconds(key.ref_epoch)
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, u32>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((id, scope, fetched_at, version)) = header else {
        return Ok(None);
    };
    if version != VERSION {
        return Ok(None);
    }
    let mut statement = connection.prepare(
        "SELECT sha, message, author, date, parents FROM commits WHERE set_id = ?1 ORDER BY position",
    )?;
    let commits = statement
        .query_map([id], |row| {
            let parents: String = row.get(4)?;
            Ok(CommitRow {
                sha: row.get(0)?,
                message: row.get(1)?,
                author: row.get(2)?,
                date: row.get(3)?,
                parents: serde_json::from_str(&parents).unwrap_or_default(),
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(Some(CommitSet {
        source_id: key.source_id.to_string(),
        scope,
        repository: key.repository.to_string(),
        branch: key.branch.to_string(),
        ref_epoch: key.ref_epoch,
        fetched_at: u64::try_from(fetched_at).unwrap_or(u64::MAX),
        commits,
    }))
}

pub fn remove_source(connection: &Connection, source_id: &str) -> Result<(), Error> {
    connection.execute("DELETE FROM commit_sets WHERE source_id = ?1", [source_id])?;
    Ok(())
}

pub fn remove_set(connection: &Connection, id: i64) -> Result<(), Error> {
    connection.execute("DELETE FROM commit_sets WHERE id = ?1", [id])?;
    Ok(())
}
