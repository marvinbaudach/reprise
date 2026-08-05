//! Persistence for sound profiles derived from stored spectrograms.

use rusqlite::{Connection, OptionalExtension};

use crate::db::{Db, DbError};
use crate::sound_features::SoundFeatures;
use crate::spectrogram::SPECTROGRAM_FORMAT_VERSION;

const SCHEMA_V56: &str = r#"
CREATE TABLE IF NOT EXISTS track_sound_features (
  track_id       INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
  format_version INTEGER NOT NULL CHECK (format_version > 0),
  data           BLOB NOT NULL
);

DROP TRIGGER IF EXISTS invalidate_track_render_data;
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
END;
"#;

/// Ensures the Sound Similarity part of schema v56.
///
/// Another branch already assigned v56 to an independent migration. This step
/// therefore checks its own schema instead of trusting `user_version`; a
/// database stamped by either v56 shape repairs to their union on open.
pub(crate) fn migrate_v56(conn: &Connection) -> Result<(), rusqlite::Error> {
    if table_exists(conn, "track_sound_features")?
        && trigger_mentions(conn, "invalidate_track_render_data", "track_sound_features")?
    {
        return Ok(());
    }
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V56)?;
    transaction.pragma_update(None, "user_version", version.max(56))?;
    transaction.commit()
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool, rusqlite::Error> {
    conn.query_row(
        "SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1",
        [table],
        |_| Ok(()),
    )
    .optional()
    .map(|row| row.is_some())
}

fn trigger_mentions(
    conn: &Connection,
    trigger: &str,
    fragment: &str,
) -> Result<bool, rusqlite::Error> {
    let sql = conn
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type = 'trigger' AND name = ?1",
            [trigger],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(sql.is_some_and(|sql| sql.contains(fragment)))
}

pub fn set_track_sound_features(
    db: &Db,
    track_id: i64,
    features: &SoundFeatures,
) -> Result<(), DbError> {
    db.conn().execute(
        "INSERT INTO track_sound_features (track_id, format_version, data) \
         VALUES (?1, ?2, ?3) ON CONFLICT(track_id) DO UPDATE SET \
         format_version = excluded.format_version, data = excluded.data",
        rusqlite::params![track_id, SPECTROGRAM_FORMAT_VERSION, features.to_blob()],
    )?;
    Ok(())
}

pub fn get_track_sound_features(db: &Db, track_id: i64) -> Result<Option<SoundFeatures>, DbError> {
    let blob = db
        .conn()
        .query_row(
            "SELECT data FROM track_sound_features \
             WHERE track_id = ?1 AND format_version = ?2",
            rusqlite::params![track_id, SPECTROGRAM_FORMAT_VERSION],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?;
    blob.map(|blob| {
        SoundFeatures::from_blob(&blob).map_err(|error| {
            DbError::Sqlite(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Blob,
                Box::new(error),
            ))
        })
    })
    .transpose()
}
