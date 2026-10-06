use super::error::{Error, Reason};
use rusqlite::Connection;

pub struct Migration {
    pub version: u32,
    pub sql: &'static str,
}

pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    sql: include_str!("migrations/0001_initial.sql"),
}];

pub fn apply(connection: &mut Connection, migrations: &[Migration]) -> Result<(), Error> {
    let current: u32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let latest = migrations.last().map_or(0, |migration| migration.version);
    if current > latest {
        return Err(Error::Unusable(Reason::NewerSchema {
            found: current,
            supported: latest,
        }));
    }
    for migration in migrations.iter().filter(|item| item.version > current) {
        let transaction = connection.transaction()?;
        transaction
            .execute_batch(migration.sql)
            .map_err(Error::Migration)?;
        transaction
            .pragma_update(None, "user_version", migration.version)
            .map_err(Error::Migration)?;
        transaction.commit().map_err(Error::Migration)?;
    }
    Ok(())
}
