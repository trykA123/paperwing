use super::error::Error;
use super::{commits, listings, refs};
use rusqlite::Connection;

const TARGET_PERCENT: u64 = 90;
const BATCH: i64 = 8;

enum Entry {
    Listing(String),
    Commits(i64),
    Refs(String),
}

pub fn used_bytes(connection: &Connection) -> Result<u64, Error> {
    let pragma = |name: &str| -> Result<u64, Error> {
        let value: i64 = connection.query_row(&format!("PRAGMA {name}"), [], |row| row.get(0))?;
        Ok(u64::try_from(value).unwrap_or_default())
    };
    let pages = pragma("page_count")?.saturating_sub(pragma("freelist_count")?);
    Ok(pages * pragma("page_size")?)
}

pub fn enforce(connection: &mut Connection, max_bytes: u64) -> Result<(), Error> {
    if used_bytes(connection)? <= max_bytes {
        return Ok(());
    }
    let target = max_bytes / 100 * TARGET_PERCENT;
    while used_bytes(connection)? > target {
        let oldest = oldest_entries(connection)?;
        if oldest.is_empty() {
            break;
        }
        let transaction = connection.transaction()?;
        for entry in &oldest {
            match entry {
                Entry::Listing(source) => listings::remove(&transaction, source)?,
                Entry::Commits(id) => commits::remove_set(&transaction, *id)?,
                Entry::Refs(url) => refs::remove(&transaction, url)?,
            }
        }
        transaction.commit()?;
    }
    connection.execute_batch("PRAGMA incremental_vacuum; PRAGMA wal_checkpoint(TRUNCATE);")?;
    Ok(())
}

fn oldest_entries(connection: &Connection) -> Result<Vec<Entry>, Error> {
    let mut statement = connection.prepare(
        "SELECT 'listing', source_id, fetched_at FROM github_listings
         UNION ALL SELECT 'commits', CAST(id AS TEXT), fetched_at FROM commit_sets
         UNION ALL SELECT 'refs', url, fetched_at FROM ref_sets
         ORDER BY 3 ASC LIMIT ?1",
    )?;
    let rows = statement.query_map([BATCH], |row| {
        let (kind, key): (String, String) = (row.get(0)?, row.get(1)?);
        Ok(match kind.as_str() {
            "listing" => Entry::Listing(key),
            "commits" => Entry::Commits(key.parse().unwrap_or_default()),
            _ => Entry::Refs(key),
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}
