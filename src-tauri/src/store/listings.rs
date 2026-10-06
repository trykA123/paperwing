use super::error::Error;
use crate::github::Repo;
use rusqlite::{params, Connection, OptionalExtension};

pub const VERSION: u32 = 3;

#[derive(Clone, Debug, PartialEq)]
pub struct Listing {
    pub source_id: String,
    pub scope: String,
    pub login: Option<String>,
    pub fetched_at: u64,
    pub version: u32,
    pub repos: Vec<Repo>,
}

pub fn put(connection: &mut Connection, listing: &Listing) -> Result<(), Error> {
    let transaction = connection.transaction()?;
    remove(&transaction, &listing.source_id)?;
    let fetched_at = super::seconds(listing.fetched_at);
    transaction.execute(
        "INSERT INTO github_listings (source_id, scope, login, fetched_at, version)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            listing.source_id,
            listing.scope,
            listing.login,
            fetched_at,
            listing.version
        ],
    )?;
    let mut insert = transaction.prepare(
        "INSERT INTO repositories (source_id, position, id, org, name, full_name, description,
                                   url, default_branch, pushed_at, archived, fetched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )?;
    for (position, repo) in listing.repos.iter().enumerate() {
        insert.execute(params![
            listing.source_id,
            position as i64,
            repo.id,
            repo.org,
            repo.name,
            format!("{}/{}", repo.org, repo.name),
            repo.description,
            repo.url,
            repo.default_branch,
            repo.pushed_at,
            repo.archived,
            fetched_at
        ])?;
    }
    drop(insert);
    Ok(transaction.commit()?)
}

pub fn get(connection: &Connection, source_id: &str) -> Result<Option<Listing>, Error> {
    let header = connection
        .query_row(
            "SELECT scope, login, fetched_at, version FROM github_listings WHERE source_id = ?1",
            [source_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, u32>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((scope, login, fetched_at, version)) = header else {
        return Ok(None);
    };
    let mut statement = connection.prepare(
        "SELECT id, org, name, description, url, default_branch, pushed_at, archived
         FROM repositories WHERE source_id = ?1 ORDER BY position",
    )?;
    let repos = statement
        .query_map([source_id], |row| {
            Ok(Repo {
                id: row.get(0)?,
                source: source_id.to_string(),
                org: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                url: row.get(4)?,
                default_branch: row.get(5)?,
                pushed_at: row.get(6)?,
                archived: row.get(7)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(Some(Listing {
        source_id: source_id.to_string(),
        scope,
        login,
        fetched_at: u64::try_from(fetched_at).unwrap_or(u64::MAX),
        version,
        repos,
    }))
}

pub fn remove_other_login(
    connection: &Connection,
    source_id: &str,
    login: Option<&str>,
) -> Result<(), Error> {
    let stored = connection
        .query_row(
            "SELECT login FROM github_listings WHERE source_id = ?1",
            [source_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?;
    if stored.is_some_and(|stored| stored.as_deref() != login) {
        remove(connection, source_id)?;
    }
    Ok(())
}

pub fn remove(connection: &Connection, source_id: &str) -> Result<(), Error> {
    connection.execute(
        "DELETE FROM github_listings WHERE source_id = ?1",
        [source_id],
    )?;
    Ok(())
}
