//! Persistence for sound profiles derived from stored spectrograms.

use rusqlite::{Connection, OptionalExtension};

use crate::db::{Db, DbError};
use crate::sound_features::{SoundFeatures, SOUND_FEATURES_FORMAT_VERSION};
use crate::spectrogram::{TrackSourceFingerprint, SPECTROGRAM_FORMAT_VERSION};

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
    write_sound_features(db.conn(), track_id, features)?;
    Ok(())
}

pub(crate) fn write_sound_features(
    conn: &Connection,
    track_id: i64,
    features: &SoundFeatures,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO track_sound_features (track_id, format_version, data) \
         VALUES (?1, ?2, ?3) ON CONFLICT(track_id) DO UPDATE SET \
         format_version = excluded.format_version, data = excluded.data",
        rusqlite::params![track_id, SOUND_FEATURES_FORMAT_VERSION, features.to_blob()],
    )?;
    Ok(())
}

pub(crate) fn set_track_sound_features_for_source(
    db: &Db,
    track_id: i64,
    source: TrackSourceFingerprint,
    features: &SoundFeatures,
) -> Result<crate::db_spectrogram::SpectrogramStoreOutcome, DbError> {
    let transaction = db.conn().unchecked_transaction()?;
    let current = transaction
        .query_row(
            "SELECT 1 FROM tracks t JOIN track_spectrograms s ON s.track_id = t.id \
             WHERE t.id = ?1 AND t.file_mtime = ?2 AND t.file_size = ?3 \
               AND t.device IS ?4 AND t.inode IS ?5 \
               AND s.source_mtime = t.file_mtime AND s.source_size = t.file_size \
               AND s.source_device IS t.device AND s.source_inode IS t.inode \
               AND s.format_version = ?6",
            rusqlite::params![
                track_id,
                source.mtime_seconds,
                source.size_bytes,
                source.device,
                source.inode,
                SPECTROGRAM_FORMAT_VERSION
            ],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !current {
        return Ok(crate::db_spectrogram::SpectrogramStoreOutcome::SourceChanged);
    }
    write_sound_features(&transaction, track_id, features)?;
    transaction.commit()?;
    Ok(crate::db_spectrogram::SpectrogramStoreOutcome::Stored)
}

pub fn get_track_sound_features(db: &Db, track_id: i64) -> Result<Option<SoundFeatures>, DbError> {
    let blob = db
        .conn()
        .query_row(
            "SELECT data FROM track_sound_features \
             WHERE track_id = ?1 AND format_version = ?2",
            rusqlite::params![track_id, SOUND_FEATURES_FORMAT_VERSION],
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

/// `(valid feature rows, present library tracks)` for the progressive Sound
/// panel empty state. Both counts come from the same database snapshot.
pub fn sound_feature_inventory(db: &Db) -> Result<(usize, usize), DbError> {
    let counts = db.conn().query_row(
        "SELECT \
           (SELECT COUNT(*) FROM track_sound_features f \
              JOIN tracks t ON t.id = f.track_id \
             WHERE f.format_version = ?1 AND t.missing_since IS NULL \
               AND t.removed_at IS NULL), \
           (SELECT COUNT(*) FROM tracks \
             WHERE missing_since IS NULL AND removed_at IS NULL)",
        [SOUND_FEATURES_FORMAT_VERSION],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
    )?;
    Ok((
        usize::try_from(counts.0).unwrap_or(usize::MAX),
        usize::try_from(counts.1).unwrap_or(usize::MAX),
    ))
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredSoundFeatures {
    pub track_id: i64,
    pub features: SoundFeatures,
}

pub(crate) fn all_track_sound_features(db: &Db) -> Result<Vec<StoredSoundFeatures>, DbError> {
    let mut statement = db.conn().prepare(
        "SELECT f.track_id, f.data FROM track_sound_features f \
         JOIN tracks t ON t.id = f.track_id \
         WHERE f.format_version = ?1 AND t.missing_since IS NULL AND t.removed_at IS NULL \
         ORDER BY f.track_id",
    )?;
    let rows = statement
        .query_map([SOUND_FEATURES_FORMAT_VERSION], |row| {
            let track_id = row.get::<_, i64>(0)?;
            let blob = row.get::<_, Vec<u8>>(1)?;
            // One unreadable row must not blind the whole library: skip it and
            // keep every profile that does decode.
            let Ok(features) = SoundFeatures::from_blob(&blob).inspect_err(|error| {
                tracing::warn!(%error, track_id, "skipping unreadable sound profile");
            }) else {
                return Ok(None);
            };
            Ok(Some(StoredSoundFeatures { track_id, features }))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows.into_iter().flatten().collect())
}

pub(crate) fn sound_feature_count(db: &Db) -> Result<usize, DbError> {
    let count = db.conn().query_row(
        "SELECT COUNT(*) FROM track_sound_features f \
         JOIN tracks t ON t.id = f.track_id \
         WHERE f.format_version = ?1 AND t.missing_since IS NULL AND t.removed_at IS NULL",
        [SOUND_FEATURES_FORMAT_VERSION],
        |row| row.get::<_, i64>(0),
    )?;
    usize::try_from(count).map_err(|error| {
        DbError::Sqlite(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Integer,
            Box::new(error),
        ))
    })
}
