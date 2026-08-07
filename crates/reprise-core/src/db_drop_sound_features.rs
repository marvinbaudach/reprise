//! Schema v60: removes the retired Sound Similarity profile cache.

use rusqlite::Connection;

const SCHEMA_V60: &str = r#"
DROP TABLE IF EXISTS track_sound_features;

DROP TRIGGER IF EXISTS invalidate_track_render_data;
CREATE TRIGGER invalidate_track_render_data
AFTER UPDATE OF file_mtime, file_size, device, inode ON tracks
WHEN OLD.file_mtime IS NOT NEW.file_mtime
  OR OLD.file_size IS NOT NEW.file_size
  OR OLD.device IS NOT NEW.device
  OR OLD.inode IS NOT NEW.inode
BEGIN
  DELETE FROM track_spectrograms WHERE track_id = NEW.id;
  UPDATE tracks SET waveform_peaks = NULL WHERE id = NEW.id;
END;
"#;

/// Schema v57 used to create the Sound Similarity cache. New databases still
/// advance through that historical version number, but no longer create the
/// table that v60 would immediately remove.
pub(crate) fn migrate_v57(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 57 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.pragma_update(None, "user_version", 57)?;
    transaction.commit()
}

pub(crate) fn migrate_v60(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 60 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V60)?;
    transaction.pragma_update(None, "user_version", 60)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    fn table_exists(conn: &Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1)",
            [name],
            |row| row.get(0),
        )
        .unwrap()
    }

    #[test]
    fn fresh_database_reaches_supported_schema_without_sound_features() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, crate::db::SUPPORTED_SCHEMA_VERSION);
        assert!(!table_exists(&conn, "track_sound_features"));
    }

    #[test]
    fn v59_upgrade_drops_only_sound_features_from_render_data() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        conn.execute_batch(
            "CREATE TABLE track_sound_features (
               track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
               format_version INTEGER NOT NULL CHECK (format_version > 0),
               data BLOB NOT NULL
             );
             DROP TRIGGER invalidate_track_render_data;
             CREATE TRIGGER invalidate_track_render_data
             AFTER UPDATE OF file_mtime, file_size, device, inode ON tracks
             WHEN OLD.file_mtime IS NOT NEW.file_mtime
               OR OLD.file_size IS NOT NEW.file_size
               OR OLD.device IS NOT NEW.device
               OR OLD.inode IS NOT NEW.inode
             BEGIN
               DELETE FROM track_spectrograms WHERE track_id = NEW.id;
               DELETE FROM track_sound_features WHERE track_id = NEW.id;
               UPDATE tracks SET waveform_peaks = NULL WHERE id = NEW.id;
             END;",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks \
             (id, path, title, added_at, file_mtime, file_size, waveform_peaks) \
             VALUES (1, '/fixture.flac', 'Fixture', 0, 10, 20, X'0102')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO track_spectrograms \
             (track_id, source_mtime, source_size, format_version, data) \
             VALUES (1, 10, 20, 1, X'')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO track_sound_features (track_id, format_version, data) \
             VALUES (1, 1, X'')",
            [],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 59).unwrap();

        super::migrate_v60(&conn).unwrap();

        assert!(!table_exists(&conn, "track_sound_features"));
        conn.execute("UPDATE tracks SET file_mtime = 11 WHERE id = 1", [])
            .unwrap();
        let spectrogram_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM track_spectrograms", [], |row| {
                row.get(0)
            })
            .unwrap();
        let waveform: Option<Vec<u8>> = conn
            .query_row(
                "SELECT waveform_peaks FROM tracks WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(spectrogram_count, 0);
        assert_eq!(waveform, None);
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 60);
    }
}
