//! Schema v49: evidence-backed default for the global online-source gate.

use std::path::Path;

use rusqlite::Connection;

pub(crate) fn migrate_v49(
    conn: &Connection,
    existing_database: bool,
    cover_cache: &Path,
    portrait_cache: &Path,
) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 49 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    crate::db_grandfather::grandfather_online_sources_gate(
        &transaction,
        existing_database,
        cover_cache,
        portrait_cache,
    )?;
    transaction.pragma_update(None, "user_version", 49)?;
    transaction.commit()
}
