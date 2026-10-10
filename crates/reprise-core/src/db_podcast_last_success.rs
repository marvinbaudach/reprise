//! Schema v92: when a subscription was last fetched successfully.
//!
//! `last_fetch_at` is the refresh scheduler's clock. A failure that gives up
//! (or a forced retry that fails) moves it too, so it cannot say how old the
//! cached episodes are. `last_success_at` moves only when a fetch succeeded, an
//! unchanged feed (`not_modified`) included, because then the cache is current.
//!
//! Rows whose last attempt succeeded start from that attempt's time. Every
//! other row stays NULL. That includes a row in a retryable failure, whose
//! `last_fetch_at` may still hold the last success; the schema cannot tell it
//! apart from a failure that moved the clock, so the backfill does not guess.

use rusqlite::Connection;

const VERSION: i64 = 92;

pub(crate) fn migrate_v92(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= VERSION {
        return Ok(());
    }
    let transaction = crate::db_migrations::begin_step(conn)?;
    // A version wound back over this schema (the repair path, and the tests
    // that rewind) already has the column, and SQLite has no
    // `ADD COLUMN IF NOT EXISTS`.
    if !has_column(&transaction)? {
        transaction.execute_batch(
            "ALTER TABLE podcast_subscriptions ADD COLUMN last_success_at INTEGER;",
        )?;
    }
    transaction.execute(
        "UPDATE podcast_subscriptions
            SET last_success_at = last_fetch_at
          WHERE last_outcome IN ('ok', 'not_modified')",
        [],
    )?;
    transaction.pragma_update(None, "user_version", VERSION)?;
    transaction.commit()
}

fn has_column(conn: &Connection) -> Result<bool, rusqlite::Error> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('podcast_subscriptions')
                       WHERE name = 'last_success_at')",
        [],
        |row| row.get(0),
    )
}

#[cfg(test)]
#[path = "db_podcast_last_success_migration_tests.rs"]
mod tests;
