use super::error::Error;
use rusqlite::Connection;

const MAX_TERMS: usize = 8;

fn match_expression(text: &str) -> Option<String> {
    let terms: Vec<String> = text
        .split_whitespace()
        .take(MAX_TERMS)
        .map(|term| format!("\"{}\"*", term.replace('"', "\"\"")))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

pub fn repository_names(
    connection: &Connection,
    text: &str,
    limit: usize,
) -> Result<Vec<String>, Error> {
    ranked(
        connection,
        "SELECT repositories.id FROM repository_search
         JOIN repositories ON repositories.rowid = repository_search.rowid
         WHERE repository_search MATCH ?1 ORDER BY rank LIMIT ?2",
        text,
        limit,
    )
}

pub fn commit_subjects(
    connection: &Connection,
    text: &str,
    limit: usize,
) -> Result<Vec<String>, Error> {
    ranked(
        connection,
        "SELECT commits.sha FROM commit_search
         JOIN commits ON commits.rowid = commit_search.rowid
         WHERE commit_search MATCH ?1 ORDER BY rank LIMIT ?2",
        text,
        limit,
    )
}

fn ranked(
    connection: &Connection,
    sql: &str,
    text: &str,
    limit: usize,
) -> Result<Vec<String>, Error> {
    let Some(expression) = match_expression(text) else {
        return Ok(Vec::new());
    };
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map(rusqlite::params![expression, limit as i64], |row| {
        row.get(0)
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}
