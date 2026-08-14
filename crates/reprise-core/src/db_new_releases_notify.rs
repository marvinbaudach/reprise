//! Schema migration for one-time New Releases desktop notifications.

use rusqlite::Connection;

pub(crate) fn migrate_v74(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 74 {
        return Ok(());
    }

    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(
        "ALTER TABLE new_releases
         ADD COLUMN notified_released_at INTEGER;",
    )?;
    transaction.pragma_update(None, "user_version", 74)?;
    transaction.commit()
}
