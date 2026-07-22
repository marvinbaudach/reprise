//! Cross-process change log facade.

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

pub mod notifier;
pub use notifier::{Handle, Notifier};

pub const MAX_RETAINED_CHANGES: usize = 10_000;
pub const RETENTION_SECS: i64 = 7 * 24 * 60 * 60;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub struct WriterToken(i64);

impl WriterToken {
    /// The raw per-process token value. Exposed so a frontend can surface it in
    /// diagnostic output (e.g. `reprise-cli events tail --json`). It only
    /// distinguishes which connection authored a change within one database's
    /// lifetime — it carries no meaning across separate processes.
    pub fn value(self) -> i64 {
        self.0
    }
}

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

/// The highest `change_log` id, or `0` when the log is empty. A single indexed
/// `MAX(id)` read — the cheap way for a frontend to seed its live-refresh cursor
/// at startup, instead of paging the whole log through `read_since(0, ..)` (up
/// to [`MAX_RETAINED_CHANGES`] rows) only to keep the last row's id.
pub fn latest_id(conn: &Connection) -> Result<i64, rusqlite::Error> {
    Ok(conn
        .query_row("SELECT MAX(id) FROM change_log", [], |row| {
            row.get::<_, Option<i64>>(0)
        })?
        .unwrap_or(0))
}

pub fn prune(conn: &Connection) -> Result<usize, rusqlite::Error> {
    prune_at(conn, unix_timestamp())
}

/// Prunes the change log on the [`crate::db::open_migrated`] boundary without
/// ever blocking or failing because another connection holds the write lock.
///
/// Two guards keep a fresh open cheap and contention-safe, closing the class of
/// bug where ~30 GTK `open_migrated(...).unwrap()` call sites could panic when a
/// long scan transaction outlived the 5s `busy_timeout`:
///
/// 1. A read-only eligibility probe runs first. WAL readers never block on a
///    writer, so this cannot stall; when nothing is actually old enough (and far
///    enough past the count floor) to delete, the function returns having
///    written nothing at all — an idle reopen is a genuine no-op, observable as
///    an unchanged `PRAGMA data_version` from any other connection.
/// 2. Only when a prune is genuinely due does it run the `DELETE`, and then with
///    `busy_timeout` temporarily set to `0` so lock contention surfaces
///    immediately as `SQLITE_BUSY` instead of waiting out the full timeout. A
///    busy/locked result is a silent skip (the next successful open prunes), and
///    the connection's default [`crate::db::DEFAULT_BUSY_TIMEOUT_MS`] is restored
///    on every path before returning.
pub(crate) fn prune_on_open(conn: &Connection) -> Result<(), rusqlite::Error> {
    prune_on_open_at(conn, unix_timestamp())
}

fn prune_on_open_at(conn: &Connection, now: i64) -> Result<(), rusqlite::Error> {
    // Cheap, non-blocking read: is any row actually eligible? If not, write
    // nothing (no data_version bump, no lock ever acquired).
    if !prune_needed(conn, now)? {
        return Ok(());
    }
    // A prune is due. Make the DELETE fail-fast under contention rather than
    // block for the full busy_timeout and surface a panic upstream.
    conn.pragma_update(None, "busy_timeout", 0)?;
    let outcome = prune_at(conn, now);
    // Restore the default on every path — success, busy, or hard error — before
    // returning; setting busy_timeout is a connection-local op that cannot
    // itself contend on the database lock.
    conn.pragma_update(None, "busy_timeout", crate::db::DEFAULT_BUSY_TIMEOUT_MS)?;
    match outcome {
        Ok(_) => Ok(()),
        // Contention: skip silently, the next successful open prunes.
        Err(error) if is_busy(&error) => Ok(()),
        Err(error) => Err(error),
    }
}

fn prune_at(conn: &Connection, now: i64) -> Result<usize, rusqlite::Error> {
    let Some((first_retained_by_count, oldest_retained_at)) = prune_bounds(conn, now)? else {
        return Ok(0);
    };
    conn.execute(
        "DELETE FROM change_log WHERE id < ?1 AND at < ?2",
        params![first_retained_by_count, oldest_retained_at],
    )
}

/// Whether at least one row is eligible for pruning at `now` — a pure read, so
/// it never blocks on a held write lock (WAL readers don't). Mirrors
/// [`prune_at`]'s predicate exactly via the shared [`prune_bounds`].
fn prune_needed(conn: &Connection, now: i64) -> Result<bool, rusqlite::Error> {
    let Some((first_retained_by_count, oldest_retained_at)) = prune_bounds(conn, now)? else {
        return Ok(false);
    };
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM change_log WHERE id < ?1 AND at < ?2)",
        params![first_retained_by_count, oldest_retained_at],
        |row| row.get(0),
    )
}

/// The `(count-floor id, age-floor timestamp)` a prune at `now` deletes strictly
/// below, or `None` when the log is empty. Shared by the eligibility probe and
/// the delete so the two can never drift on where the retention boundary sits.
fn prune_bounds(conn: &Connection, now: i64) -> Result<Option<(i64, i64)>, rusqlite::Error> {
    let newest_id: Option<i64> =
        conn.query_row("SELECT MAX(id) FROM change_log", [], |row| row.get(0))?;
    Ok(newest_id.map(|newest_id| {
        let first_retained_by_count =
            newest_id.saturating_sub(MAX_RETAINED_CHANGES.saturating_sub(1) as i64);
        let oldest_retained_at = now.saturating_sub(RETENTION_SECS);
        (first_retained_by_count, oldest_retained_at)
    }))
}

/// Whether a `rusqlite::Error` is a transient busy/locked failure — the two
/// codes a non-blocking prune treats as "skip, try next open".
fn is_busy(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(inner, _)
            if inner.code == rusqlite::ErrorCode::DatabaseBusy
                || inner.code == rusqlite::ErrorCode::DatabaseLocked
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
