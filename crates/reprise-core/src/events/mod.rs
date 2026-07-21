//! Cross-process change log facade.

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

pub mod notifier;
pub use notifier::{Handle, Notifier};

pub const MAX_RETAINED_CHANGES: usize = 10_000;
pub const RETENTION_SECS: i64 = 7 * 24 * 60 * 60;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WriterToken(i64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Change {
    pub id: i64,
    pub entity: String,
    pub entity_id: String,
    pub operation: String,
    pub writer: WriterToken,
    pub at: i64,
}

pub fn writer_token() -> WriterToken {
    static TOKEN: OnceLock<WriterToken> = OnceLock::new();
    *TOKEN.get_or_init(|| WriterToken(fastrand::i64(..)))
}

pub(crate) fn record(
    conn: &Connection,
    entity: &str,
    entity_id: &str,
    operation: &str,
) -> Result<i64, rusqlite::Error> {
    record_at(
        conn,
        entity,
        entity_id,
        operation,
        writer_token(),
        unix_timestamp(),
    )
}

fn record_at(
    conn: &Connection,
    entity: &str,
    entity_id: &str,
    operation: &str,
    writer: WriterToken,
    at: i64,
) -> Result<i64, rusqlite::Error> {
    conn.execute(
        "INSERT INTO change_log (entity, entity_id, op, writer, at) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![entity, entity_id, operation, writer.0, at],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Runs `body` so its writes — the facade's mutation *and* any [`record`]
/// call — commit atomically. When `conn` is not already inside a transaction
/// a fresh one is opened and committed here; when the caller already holds one
/// (SQLite forbids a nested `BEGIN`) `body` simply joins it, and the event
/// then lands or rolls back with the caller's transaction. Either way a
/// facade never logs a change it did not also persist, and never persists a
/// change it did not log.
pub(crate) fn in_txn<T>(
    conn: &Connection,
    body: impl FnOnce(&Connection) -> Result<T, rusqlite::Error>,
) -> Result<T, rusqlite::Error> {
    if conn.is_autocommit() {
        let tx = conn.unchecked_transaction()?;
        let value = body(&tx)?;
        tx.commit()?;
        Ok(value)
    } else {
        body(conn)
    }
}

pub fn read_since(
    conn: &Connection,
    last_seen_id: i64,
    excluded_writer: Option<WriterToken>,
) -> Result<Vec<Change>, rusqlite::Error> {
    let sql = if excluded_writer.is_some() {
        "SELECT id, entity, entity_id, op, writer, at FROM change_log \
         WHERE id > ?1 AND writer != ?2 ORDER BY id"
    } else {
        "SELECT id, entity, entity_id, op, writer, at FROM change_log \
         WHERE id > ?1 ORDER BY id"
    };
    let mut statement = conn.prepare(sql)?;
    let map_row = |row: &rusqlite::Row<'_>| {
        Ok(Change {
            id: row.get(0)?,
            entity: row.get(1)?,
            entity_id: row.get(2)?,
            operation: row.get(3)?,
            writer: WriterToken(row.get(4)?),
            at: row.get(5)?,
        })
    };
    if let Some(writer) = excluded_writer {
        statement
            .query_map(params![last_seen_id, writer.0], map_row)?
            .collect()
    } else {
        statement.query_map([last_seen_id], map_row)?.collect()
    }
}

pub fn prune(conn: &Connection) -> Result<usize, rusqlite::Error> {
    prune_at(conn, unix_timestamp())
}

fn prune_at(conn: &Connection, now: i64) -> Result<usize, rusqlite::Error> {
    let newest_id: Option<i64> =
        conn.query_row("SELECT MAX(id) FROM change_log", [], |row| row.get(0))?;
    let Some(newest_id) = newest_id else {
        return Ok(0);
    };
    let first_retained_by_count =
        newest_id.saturating_sub(MAX_RETAINED_CHANGES.saturating_sub(1) as i64);
    let oldest_retained_at = now.saturating_sub(RETENTION_SECS);
    conn.execute(
        "DELETE FROM change_log WHERE id < ?1 AND at < ?2",
        params![first_retained_by_count, oldest_retained_at],
    )
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod facade_tests;
