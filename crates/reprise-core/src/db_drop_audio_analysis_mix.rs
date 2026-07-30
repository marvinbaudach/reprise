//! Schema v27: drops the tables retired when the Song Analysis (Audio
//! Character), Create Similar Mix, and Related Artist Discovery features were
//! removed.
//!
//! The v18 (`track_audio_analysis`) and v23 (`mix_drafts`/`mix_draft_tracks`)
//! CREATE steps stay in place as immutable history — every database still
//! walks the same version sequence — and this later step reclaims those tables
//! on every database, fresh or existing, that ran them. `tracks.waveform_peaks`
//! is a separate column on `tracks`, deliberately untouched: the seek-bar
//! waveform still reads it.

use rusqlite::Connection;

const SCHEMA_V27: &str = r#"
DROP TABLE IF EXISTS mix_draft_tracks;
DROP TABLE IF EXISTS mix_drafts;
DROP TABLE IF EXISTS track_audio_analysis;
"#;

pub(crate) fn migrate_v27(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 27 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V27)?;
    transaction.pragma_update(None, "user_version", 27)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    fn table_exists(conn: &rusqlite::Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [name],
            |row| row.get(0),
        )
        .unwrap()
    }

    #[test]
    fn fresh_migrate_drops_retired_tables_and_reaches_current_schema() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, crate::db::SUPPORTED_SCHEMA_VERSION);
        assert!(!table_exists(&conn, "track_audio_analysis"));
        assert!(!table_exists(&conn, "mix_drafts"));
        assert!(!table_exists(&conn, "mix_draft_tracks"));
    }

    #[test]
    fn migration_is_idempotent() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        super::migrate_v27(&conn).unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, crate::db::SUPPORTED_SCHEMA_VERSION);
    }
}
