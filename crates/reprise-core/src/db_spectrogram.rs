//! Persistence for source-bound spectrogram rendering data.

use rusqlite::{Connection, OptionalExtension};

use crate::db::{Db, DbError};
use crate::sound_features::{derive_sound_features, SOUND_FEATURES_FORMAT_VERSION};
use crate::spectrogram::{TrackSourceFingerprint, TrackSpectrogram, SPECTROGRAM_FORMAT_VERSION};
use crate::waveform::TrackRenderData;
const SCHEMA_V55: &str = r#"
CREATE TABLE IF NOT EXISTS track_spectrograms (
  track_id       INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
  source_mtime   INTEGER NOT NULL CHECK (source_mtime >= 0),
  source_size    INTEGER NOT NULL CHECK (source_size >= 0),
  source_device  INTEGER,
  source_inode   INTEGER,
  format_version INTEGER NOT NULL CHECK (format_version > 0),
  data           BLOB NOT NULL,
  CHECK (length(data) % 24 = 0)
);

CREATE TRIGGER IF NOT EXISTS invalidate_track_render_data
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

pub(crate) fn migrate_v55(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 55 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V55)?;
    transaction.pragma_update(None, "user_version", 55)?;
    transaction.commit()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpectrogramStoreOutcome {
    Stored,
    SourceChanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRenderDataTrack {
    pub track_id: i64,
    pub path: String,
    pub source: TrackSourceFingerprint,
    pub spectrogram: Option<TrackSpectrogram>,
}

pub fn set_track_spectrogram(
    db: &Db,
    track_id: i64,
    source: TrackSourceFingerprint,
    spectrogram: &TrackSpectrogram,
) -> Result<SpectrogramStoreOutcome, DbError> {
    let transaction = db.conn().unchecked_transaction()?;
    let current = source_fingerprint(&transaction, track_id)?;
    if current != Some(source) {
        return Ok(SpectrogramStoreOutcome::SourceChanged);
    }
    write_spectrogram(&transaction, track_id, source, spectrogram)?;
    crate::db_sound_features::write_sound_features(
        &transaction,
        track_id,
        &derive_sound_features(spectrogram),
    )?;
    transaction.commit()?;
    Ok(SpectrogramStoreOutcome::Stored)
}

pub fn set_track_render_data(
    db: &Db,
    track_id: i64,
    source: TrackSourceFingerprint,
    data: &TrackRenderData,
) -> Result<SpectrogramStoreOutcome, DbError> {
    let transaction = db.conn().unchecked_transaction()?;
    let current = source_fingerprint(&transaction, track_id)?;
    if current != Some(source) {
        return Ok(SpectrogramStoreOutcome::SourceChanged);
    }
    transaction.execute(
        "UPDATE tracks SET waveform_peaks = ?1 WHERE id = ?2",
        rusqlite::params![data.waveform_peaks, track_id],
    )?;
    write_spectrogram(&transaction, track_id, source, &data.spectrogram)?;
    crate::db_sound_features::write_sound_features(
        &transaction,
        track_id,
        &derive_sound_features(&data.spectrogram),
    )?;
    transaction.commit()?;
    Ok(SpectrogramStoreOutcome::Stored)
}

fn write_spectrogram(
    conn: &Connection,
    track_id: i64,
    source: TrackSourceFingerprint,
    spectrogram: &TrackSpectrogram,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO track_spectrograms \
         (track_id, source_mtime, source_size, source_device, source_inode, \
          format_version, data) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
         ON CONFLICT(track_id) DO UPDATE SET \
           source_mtime=excluded.source_mtime, source_size=excluded.source_size, \
           source_device=excluded.source_device, source_inode=excluded.source_inode, \
           format_version=excluded.format_version, data=excluded.data",
        rusqlite::params![
            track_id,
            source.mtime_seconds,
            source.size_bytes,
            source.device,
            source.inode,
            SPECTROGRAM_FORMAT_VERSION,
            spectrogram.cells(),
        ],
    )?;
    Ok(())
}

pub fn get_track_spectrogram(db: &Db, track_id: i64) -> Result<Option<TrackSpectrogram>, DbError> {
    let stored = db
        .conn()
        .query_row(
            "SELECT s.data \
             FROM track_spectrograms s \
             JOIN tracks t ON t.id = s.track_id \
             WHERE s.track_id = ?1 AND s.format_version = ?2 \
               AND s.source_mtime = t.file_mtime \
               AND s.source_size = t.file_size \
               AND s.source_device IS t.device \
               AND s.source_inode IS t.inode",
            rusqlite::params![track_id, SPECTROGRAM_FORMAT_VERSION],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?;
    stored
        .map(|cells| {
            TrackSpectrogram::from_cells(cells).map_err(|error| {
                DbError::Sqlite(rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Blob,
                    Box::new(error),
                ))
            })
        })
        .transpose()
}

/// Rendering data lifted out before a rewrite that changes a file's metadata
/// but not its audio, so it can be re-keyed to the new source identity.
///
/// The invalidation trigger keys on file metadata because that is all it can
/// see. A caller that rewrites tags knows more than the trigger does: it knows
/// no sample changed. This type is how that knowledge is carried.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CarriedRenderData {
    waveform_peaks: Option<Vec<u8>>,
    spectrogram: Option<(i64, Vec<u8>)>,
}

impl CarriedRenderData {
    fn is_empty(&self) -> bool {
        self.waveform_peaks.is_none() && self.spectrogram.is_none()
    }
}

/// Captures the rendering data that is *currently valid* for a track. A stale
/// spectrogram is deliberately left behind: it would have to be recomputed
/// either way.
pub(crate) fn snapshot_render_data(
    conn: &Connection,
    track_id: i64,
) -> Result<CarriedRenderData, rusqlite::Error> {
    conn.query_row(
        "SELECT t.waveform_peaks, s.format_version, s.data \
         FROM tracks t LEFT JOIN track_spectrograms s \
           ON s.track_id = t.id \
          AND s.source_mtime = t.file_mtime AND s.source_size = t.file_size \
          AND s.source_device IS t.device AND s.source_inode IS t.inode \
         WHERE t.id = ?1",
        [track_id],
        |row| {
            let format_version = row.get::<_, Option<i64>>(1)?;
            let data = row.get::<_, Option<Vec<u8>>>(2)?;
            Ok(CarriedRenderData {
                waveform_peaks: row.get(0)?,
                spectrogram: format_version.zip(data),
            })
        },
    )
    .optional()
    .map(Option::unwrap_or_default)
}

/// Re-keys carried rendering data onto the track's new source identity. A
/// no-op when nothing was carried or the track no longer exists.
pub(crate) fn restore_render_data(
    conn: &Connection,
    track_id: i64,
    carried: &CarriedRenderData,
) -> Result<(), rusqlite::Error> {
    if carried.is_empty() {
        return Ok(());
    }
    let Some(source) = source_fingerprint(conn, track_id)? else {
        return Ok(());
    };
    if let Some(peaks) = &carried.waveform_peaks {
        // Only fills the hole the trigger punched; never overwrites peaks that
        // something else produced in the meantime.
        conn.execute(
            "UPDATE tracks SET waveform_peaks = ?1 \
             WHERE id = ?2 AND waveform_peaks IS NULL",
            rusqlite::params![peaks, track_id],
        )?;
    }
    if let Some((format_version, data)) = &carried.spectrogram {
        conn.execute(
            "INSERT INTO track_spectrograms \
             (track_id, source_mtime, source_size, source_device, source_inode, \
              format_version, data) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
             ON CONFLICT(track_id) DO NOTHING",
            rusqlite::params![
                track_id,
                source.mtime_seconds,
                source.size_bytes,
                source.device,
                source.inode,
                format_version,
                data,
            ],
        )?;
        if *format_version == SPECTROGRAM_FORMAT_VERSION {
            if let Ok(spectrogram) = TrackSpectrogram::from_cells(data.clone()) {
                crate::db_sound_features::write_sound_features(
                    conn,
                    track_id,
                    &derive_sound_features(&spectrogram),
                )?;
            }
        }
    }
    Ok(())
}

/// The track's current source identity, as the invalidation trigger sees it.
pub fn track_source_fingerprint(
    db: &Db,
    track_id: i64,
) -> Result<Option<TrackSourceFingerprint>, DbError> {
    Ok(source_fingerprint(db.conn(), track_id)?)
}

fn source_fingerprint(
    conn: &Connection,
    track_id: i64,
) -> Result<Option<TrackSourceFingerprint>, rusqlite::Error> {
    conn.query_row(
        "SELECT file_mtime, file_size, device, inode FROM tracks WHERE id = ?1",
        [track_id],
        |row| {
            Ok(TrackSourceFingerprint {
                mtime_seconds: row.get(0)?,
                size_bytes: row.get(1)?,
                device: row.get(2)?,
                inode: row.get(3)?,
            })
        },
    )
    .optional()
}

/// Stores pre-computed waveform peaks for a track.
pub fn set_waveform_peaks(db: &Db, track_id: i64, peaks: &[u8]) -> Result<(), DbError> {
    db.conn().execute(
        "UPDATE tracks SET waveform_peaks = ?1 WHERE id = ?2",
        rusqlite::params![peaks, track_id],
    )?;
    Ok(())
}

/// Loads pre-computed waveform peaks for a track. Returns `None` if not yet analyzed.
pub fn get_waveform_peaks(db: &Db, track_id: i64) -> Result<Option<Vec<u8>>, DbError> {
    let result = db.conn().query_row(
        "SELECT waveform_peaks FROM tracks WHERE id = ?1",
        [track_id],
        |row| row.get::<_, Option<Vec<u8>>>(0),
    )?;
    Ok(result)
}

/// Returns live tracks whose rendering data is absent or stale, in stable id order.
pub fn pending_render_data_tracks(db: &Db) -> Result<Vec<PendingRenderDataTrack>, DbError> {
    let mut statement = db.conn().prepare(&format!(
        "SELECT t.id, t.path, t.file_mtime, t.file_size, t.device, t.inode, \
                CASE WHEN t.waveform_peaks IS NOT NULL THEN s.data END \
         FROM tracks t \
         LEFT JOIN track_spectrograms s ON s.track_id = t.id \
           AND s.format_version = ?1 AND s.source_mtime = t.file_mtime \
           AND s.source_size = t.file_size AND s.source_device IS t.device \
           AND s.source_inode IS t.inode \
         LEFT JOIN track_sound_features f ON f.track_id = t.id AND f.format_version = ?2 \
         WHERE {} AND (t.waveform_peaks IS NULL OR s.track_id IS NULL OR f.track_id IS NULL) \
         ORDER BY t.id",
        crate::queries::PRESENT
    ))?;
    let tracks = statement
        .query_map(
            [SPECTROGRAM_FORMAT_VERSION, SOUND_FEATURES_FORMAT_VERSION],
            |row| {
                let spectrogram = row
                    .get::<_, Option<Vec<u8>>>(6)?
                    .map(TrackSpectrogram::from_cells)
                    .transpose()
                    .map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            6,
                            rusqlite::types::Type::Blob,
                            Box::new(error),
                        )
                    })?;
                Ok(PendingRenderDataTrack {
                    track_id: row.get(0)?,
                    path: row.get(1)?,
                    source: TrackSourceFingerprint {
                        mtime_seconds: row.get(2)?,
                        size_bytes: row.get(3)?,
                        device: row.get(4)?,
                        inode: row.get(5)?,
                    },
                    spectrogram,
                })
            },
        )?
        .collect::<Result<_, _>>()?;
    Ok(tracks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> TrackSourceFingerprint {
        TrackSourceFingerprint {
            mtime_seconds: 11,
            size_bytes: 22,
            device: Some(33),
            inode: Some(44),
        }
    }

    #[test]
    fn v54_upgrade_creates_the_rendering_table_once() {
        for table_already_exists in [false, true] {
            let conn = crate::db::open(None).unwrap();
            crate::db::migrate_connection(&conn).unwrap();
            if !table_already_exists {
                conn.execute("DROP TRIGGER invalidate_track_render_data", [])
                    .unwrap();
                conn.execute("DROP TABLE track_spectrograms", []).unwrap();
            }
            conn.pragma_update(None, "user_version", 54).unwrap();

            migrate_v55(&conn).unwrap();
            migrate_v55(&conn).unwrap();

            assert_eq!(
                conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                    .unwrap(),
                55
            );
            let table_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_schema \
                     WHERE type = 'table' AND name = 'track_spectrograms'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(table_count, 1);
        }
    }

    #[test]
    fn absent_and_computed_empty_spectrograms_are_distinct_states() {
        let db = Db::open_in_memory().unwrap();
        db.conn()
            .execute(
                "INSERT INTO tracks \
                 (id, path, title, added_at, file_mtime, file_size, device, inode) \
                 VALUES (1, '/empty.flac', '', 0, 11, 22, 33, 44)",
                [],
            )
            .unwrap();

        assert!(get_track_spectrogram(&db, 1).unwrap().is_none());
        assert_eq!(
            set_track_spectrogram(&db, 1, source(), &TrackSpectrogram::empty()).unwrap(),
            SpectrogramStoreOutcome::Stored
        );
        assert_eq!(
            get_track_spectrogram(&db, 1).unwrap(),
            Some(TrackSpectrogram::empty())
        );
    }

    #[test]
    fn pending_render_data_includes_tracks_with_old_peaks_but_no_spectrogram() {
        let db = Db::open_in_memory().unwrap();
        for (id, missing_since) in [(1, None), (2, None), (3, Some(1))] {
            db.conn()
                .execute(
                    "INSERT INTO tracks \
                     (id, path, title, added_at, file_mtime, file_size, device, inode, missing_since) \
                     VALUES (?1, ?2, '', 0, 11, 22, 33, ?3, ?4)",
                    rusqlite::params![id, format!("/{id}.flac"), 40 + id, missing_since],
                )
                .unwrap();
        }
        set_waveform_peaks(&db, 1, &[9]).unwrap();

        assert_eq!(
            pending_render_data_tracks(&db).unwrap(),
            vec![
                PendingRenderDataTrack {
                    track_id: 1,
                    path: "/1.flac".into(),
                    source: TrackSourceFingerprint {
                        inode: Some(41),
                        ..source()
                    },
                    spectrogram: None,
                },
                PendingRenderDataTrack {
                    track_id: 2,
                    path: "/2.flac".into(),
                    source: TrackSourceFingerprint {
                        inode: Some(42),
                        ..source()
                    },
                    spectrogram: None,
                },
            ]
        );
    }

    #[test]
    fn source_change_physically_invalidates_both_rendering_caches() {
        let db = Db::open_in_memory().unwrap();
        db.conn()
            .execute(
                "INSERT INTO tracks \
                 (id, path, title, added_at, file_mtime, file_size, device, inode) \
                 VALUES (1, '/changed.flac', '', 0, 11, 22, 33, 44)",
                [],
            )
            .unwrap();
        let spectrogram = TrackSpectrogram::from_cells(vec![7; 48]).unwrap();
        set_waveform_peaks(&db, 1, &[8, 9]).unwrap();
        assert_eq!(
            set_track_spectrogram(&db, 1, source(), &spectrogram).unwrap(),
            SpectrogramStoreOutcome::Stored
        );

        db.conn()
            .execute("UPDATE tracks SET file_mtime = 12 WHERE id = 1", [])
            .unwrap();

        assert!(get_waveform_peaks(&db, 1).unwrap().is_none());
        assert!(get_track_spectrogram(&db, 1).unwrap().is_none());
        let rows: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM track_spectrograms", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(rows, 0, "stale blob must not remain on overflow pages");
        assert_eq!(
            set_track_spectrogram(&db, 1, source(), &spectrogram).unwrap(),
            SpectrogramStoreOutcome::SourceChanged
        );
    }

    #[test]
    fn spectrogram_is_a_separate_cascading_track_table() {
        let db = Db::open_in_memory().unwrap();
        db.conn()
            .execute(
                "INSERT INTO tracks \
                 (id, path, title, added_at, file_mtime, file_size, device, inode) \
                 VALUES (1, '/deleted.flac', '', 0, 11, 22, 33, 44)",
                [],
            )
            .unwrap();
        set_track_spectrogram(&db, 1, source(), &TrackSpectrogram::empty()).unwrap();

        let track_columns = db
            .conn()
            .prepare("PRAGMA table_info(tracks)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(!track_columns.iter().any(|name| name == "spectrogram"));

        db.conn()
            .execute("DELETE FROM tracks WHERE id = 1", [])
            .unwrap();
        let rows: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM track_spectrograms", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(rows, 0);
    }
}
