//! Schema v71: inherit consent from any of the three retired artwork modules.

use rusqlite::Connection;

pub(crate) fn migrate_v71(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 71 {
        return Ok(());
    }

    let transaction = conn.unchecked_transaction()?;
    let inserted = transaction.execute(
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
    record_consent_merge_notice(&transaction, inserted)?;
    transaction.pragma_update(None, "user_version", 71)?;
    transaction.commit()
}

/// Repairs databases stamped by the original v71 migration, whose `AND`
/// predicate left the unified setting absent unless every retired module was
/// enabled.
pub(crate) fn migrate_v72(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 72 {
        return Ok(());
    }

    let transaction = conn.unchecked_transaction()?;
    let inserted = transaction.execute(
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
    record_consent_merge_notice(&transaction, inserted)?;
    transaction.pragma_update(None, "user_version", 72)?;
    transaction.commit()
}

fn record_consent_merge_notice(conn: &Connection, inserted: usize) -> Result<(), rusqlite::Error> {
    if inserted == 1 {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, '1') \
             ON CONFLICT(key) DO UPDATE SET value = '1'",
            [crate::library::settings::ARTWORK_CONSENT_MERGE_NOTICE_PENDING_KEY],
        )?;
    }
    Ok(())
}
