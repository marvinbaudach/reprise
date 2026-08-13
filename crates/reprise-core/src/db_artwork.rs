//! Schema v71: inherit consent from any of the three retired artwork modules.

use rusqlite::Connection;

pub(crate) fn migrate_v71(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 71 {
        return Ok(());
    }

    let transaction = conn.unchecked_transaction()?;
    transaction.execute(
        "INSERT OR IGNORE INTO settings (key, value) SELECT ?1, '1' WHERE \
         COALESCE((SELECT value FROM settings WHERE key = ?2), '0') = '1' OR \
         COALESCE((SELECT value FROM settings WHERE key = ?3), '0') = '1' OR \
         COALESCE((SELECT value FROM settings WHERE key = ?4), '0') = '1'",
        rusqlite::params![
            crate::modules::enabled_key(&crate::modules::ARTWORK_MODULE),
            crate::db_grandfather::LEGACY_COVER_DOWNLOAD_KEY,
            crate::db_grandfather::LEGACY_ARTIST_PORTRAITS_KEY,
            crate::db_grandfather::LEGACY_SOURCE_IMAGES_KEY,
        ],
    )?;
    transaction.pragma_update(None, "user_version", 71)?;
    transaction.commit()
}
