use rusqlite::{Connection, OptionalExtension};

use super::loudness::{album_loudness, MeasuredLoudness};
use crate::spectrogram::TrackSourceFingerprint;

pub const LOUDNESS_FORMAT_VERSION: i64 = 1;

const LOUDNESS_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS track_loudness (
  track_id        INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
  source_mtime    INTEGER NOT NULL,
  source_size     INTEGER NOT NULL,
  source_device   INTEGER,
  source_inode    INTEGER,
  format_version  INTEGER NOT NULL,
  integrated_lufs REAL,
  true_peak       REAL
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
  DELETE FROM track_loudness WHERE track_id = NEW.id;
  UPDATE tracks SET waveform_peaks = NULL WHERE id = NEW.id;
END;
"#;

pub(crate) fn migrate_v89(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let has_loudness_table: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='track_loudness')",
        [],
        |row| row.get(0),
    )?;
    if version >= 89 && has_loudness_table {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    if version < 89 {
        for (column, declaration) in [
            ("rg_track_gain", "REAL"),
            ("rg_track_peak", "REAL"),
            ("rg_album_gain", "REAL"),
            ("rg_album_peak", "REAL"),
            ("tag_scan_version", "INTEGER NOT NULL DEFAULT 0"),
        ] {
            let exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('tracks') WHERE name = ?1)",
                [column],
                |row| row.get(0),
            )?;
            if !exists {
                transaction.execute_batch(&format!(
                    "ALTER TABLE tracks ADD COLUMN {column} {declaration};"
                ))?;
            }
        }
        transaction.pragma_update(None, "user_version", 89)?;
    }
    transaction.execute_batch(LOUDNESS_SCHEMA)?;
    transaction.commit()
}

pub(crate) fn write_track_loudness(
    conn: &Connection,
    track_id: i64,
    source: TrackSourceFingerprint,
    loudness: Option<MeasuredLoudness>,
) -> Result<(), rusqlite::Error> {
    let (integrated_lufs, true_peak) = loudness.map_or((None, None), |value| {
        (Some(value.integrated_lufs), Some(value.true_peak))
    });
    conn.execute(
        "INSERT INTO track_loudness \
         (track_id, source_mtime, source_size, source_device, source_inode, \
          format_version, integrated_lufs, true_peak) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
         ON CONFLICT(track_id) DO UPDATE SET \
           source_mtime=excluded.source_mtime, source_size=excluded.source_size, \
           source_device=excluded.source_device, source_inode=excluded.source_inode, \
           format_version=excluded.format_version, integrated_lufs=excluded.integrated_lufs, \
           true_peak=excluded.true_peak",
        rusqlite::params![
            track_id,
            source.mtime_seconds,
            source.size_bytes,
            source.device,
            source.inode,
            LOUDNESS_FORMAT_VERSION,
            integrated_lufs,
            true_peak,
        ],
    )?;
    Ok(())
}

pub fn measured_loudness(
    conn: &Connection,
    track_id: i64,
) -> Result<Option<MeasuredLoudness>, rusqlite::Error> {
    Ok(stored_loudness(conn, track_id)?.flatten())
}

pub(crate) fn stored_loudness(
    conn: &Connection,
    track_id: i64,
) -> Result<Option<Option<MeasuredLoudness>>, rusqlite::Error> {
    conn.query_row(
        "SELECT l.integrated_lufs, l.true_peak \
         FROM track_loudness l JOIN tracks t ON t.id = l.track_id \
         WHERE l.track_id = ?1 AND l.format_version = ?2 \
           AND l.source_mtime = t.file_mtime AND l.source_size = t.file_size \
           AND l.source_device IS t.device AND l.source_inode IS t.inode",
        rusqlite::params![track_id, LOUDNESS_FORMAT_VERSION],
        |row| {
            Ok(row
                .get::<_, Option<f64>>(0)?
                .zip(row.get::<_, Option<f64>>(1)?)
                .map(|(integrated_lufs, true_peak)| MeasuredLoudness {
                    integrated_lufs,
                    true_peak,
                }))
        },
    )
    .optional()
}

pub fn album_measured_loudness(
    conn: &Connection,
    track_id: i64,
) -> Result<Option<(f64, f64)>, rusqlite::Error> {
    let key = conn
        .query_row(
            "SELECT LOWER(TRIM(album)), \
                    LOWER(CASE WHEN TRIM(album_artist) <> '' THEN TRIM(album_artist) \
                               ELSE TRIM(artist) END) \
             FROM tracks WHERE id = ?1 AND TRIM(album) <> ''",
            [track_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let Some((album, artist)) = key else {
        return Ok(None);
    };
    let mut statement = conn.prepare(&format!(
        "SELECT l.integrated_lufs, l.true_peak, t.duration_ms \
         FROM tracks t LEFT JOIN track_loudness l ON l.track_id = t.id \
           AND l.format_version = ?3 AND l.source_mtime = t.file_mtime \
           AND l.source_size = t.file_size AND l.source_device IS t.device \
           AND l.source_inode IS t.inode \
         WHERE {} AND LOWER(TRIM(t.album)) = ?1 \
           AND LOWER(CASE WHEN TRIM(t.album_artist) <> '' THEN TRIM(t.album_artist) \
                          ELSE TRIM(t.artist) END) = ?2",
        crate::queries::PRESENT
    ))?;
    let rows = statement
        .query_map(
            rusqlite::params![album, artist, LOUDNESS_FORMAT_VERSION],
            |row| {
                Ok((
                    row.get::<_, Option<f64>>(0)?,
                    row.get::<_, Option<f64>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    // A track without a row, or with a NULL (silent) measurement, leaves the
    // album without a complete measurement: the caller falls back to the track
    // rule instead of normalising against a partial mean.
    let measured = rows
        .iter()
        .map(|(lufs, peak, duration)| lufs.zip(*peak).map(|(lufs, peak)| (lufs, peak, *duration)))
        .collect::<Option<Vec<_>>>();
    let Some(measured) = measured.filter(|tracks| !tracks.is_empty()) else {
        return Ok(None);
    };
    let tracks = measured
        .iter()
        .map(|(lufs, _, duration)| (*lufs, *duration))
        .collect::<Vec<_>>();
    let true_peak = measured
        .iter()
        .map(|(_, peak, _)| *peak)
        .fold(0.0_f64, f64::max);
    Ok(album_loudness(&tracks).map(|lufs| (lufs, true_peak)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert_track(conn: &Connection, id: i64, album: &str, artist: &str, duration_ms: i64) {
        conn.execute(
            "INSERT INTO tracks (id, path, title, album, artist, added_at, duration_ms, \
             file_mtime, file_size, device, inode) \
             VALUES (?1, ?2, '', ?3, ?4, 0, ?5, 11, 22, 33, ?6)",
            rusqlite::params![
                id,
                format!("/{id}.flac"),
                album,
                artist,
                duration_ms,
                40 + id
            ],
        )
        .unwrap();
    }

    #[test]
    fn migration_v89_adds_replaygain_columns_once() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE tracks (id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE);\
             PRAGMA user_version = 88;",
        )
        .unwrap();

        migrate_v89(&conn).unwrap();

        for column in [
            "rg_track_gain",
            "rg_track_peak",
            "rg_album_gain",
            "rg_album_peak",
            "tag_scan_version",
        ] {
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM pragma_table_info('tracks') WHERE name = ?1)",
                    [column],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing tracks.{column}");
        }
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 89);

        migrate_v89(&conn).unwrap();
    }

    #[test]
    fn migration_v89_adds_the_loudness_table_even_when_columns_already_landed() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE tracks (id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE,
               rg_track_gain REAL, rg_track_peak REAL, rg_album_gain REAL, rg_album_peak REAL,
               tag_scan_version INTEGER NOT NULL DEFAULT 0);
             PRAGMA user_version = 89;",
        )
        .unwrap();

        migrate_v89(&conn).unwrap();
        migrate_v89(&conn).unwrap();

        let tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name='track_loudness'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tables, 1);
    }

    #[test]
    fn measured_loudness_requires_a_current_source_fingerprint() {
        let db = crate::db::Db::open_in_memory().unwrap();
        insert_track(db.conn(), 1, "Album", "Artist", 1_000);
        write_track_loudness(
            db.conn(),
            1,
            crate::spectrogram::TrackSourceFingerprint {
                mtime_seconds: 11,
                size_bytes: 22,
                device: Some(33),
                inode: Some(41),
            },
            Some(MeasuredLoudness {
                integrated_lufs: -20.0,
                true_peak: 0.8,
            }),
        )
        .unwrap();

        assert_eq!(
            measured_loudness(db.conn(), 1).unwrap(),
            Some(MeasuredLoudness {
                integrated_lufs: -20.0,
                true_peak: 0.8,
            })
        );
        db.conn()
            .execute("UPDATE tracks SET file_size = 23 WHERE id = 1", [])
            .unwrap();
        assert_eq!(measured_loudness(db.conn(), 1).unwrap(), None);
    }

    #[test]
    fn album_loudness_waits_until_every_present_track_has_a_row() {
        let db = crate::db::Db::open_in_memory().unwrap();
        insert_track(db.conn(), 1, " Album ", "Artist", 1_000);
        insert_track(db.conn(), 2, "album", "Artist", 3_000);
        let source = |inode| crate::spectrogram::TrackSourceFingerprint {
            mtime_seconds: 11,
            size_bytes: 22,
            device: Some(33),
            inode: Some(inode),
        };
        write_track_loudness(
            db.conn(),
            1,
            source(41),
            Some(MeasuredLoudness {
                integrated_lufs: -20.0,
                true_peak: 0.5,
            }),
        )
        .unwrap();
        assert_eq!(album_measured_loudness(db.conn(), 1).unwrap(), None);

        write_track_loudness(
            db.conn(),
            2,
            source(42),
            Some(MeasuredLoudness {
                integrated_lufs: -10.0,
                true_peak: 0.8,
            }),
        )
        .unwrap();
        let (lufs, peak) = album_measured_loudness(db.conn(), 1).unwrap().unwrap();
        let expected =
            10.0 * ((1_000.0 * 10_f64.powf(-2.0) + 3_000.0 * 10_f64.powf(-1.0)) / 4_000.0).log10();
        assert!((lufs - expected).abs() < 1e-6);
        assert_eq!(peak, 0.8);
    }

    #[test]
    fn album_loudness_treats_a_silent_track_as_unmeasured() {
        let db = crate::db::Db::open_in_memory().unwrap();
        insert_track(db.conn(), 1, "Album", "Artist", 1_000);
        insert_track(db.conn(), 2, "Album", "Artist", 3_000);
        let source = |inode| crate::spectrogram::TrackSourceFingerprint {
            mtime_seconds: 11,
            size_bytes: 22,
            device: Some(33),
            inode: Some(inode),
        };
        write_track_loudness(
            db.conn(),
            1,
            source(41),
            Some(MeasuredLoudness {
                integrated_lufs: -20.0,
                true_peak: 0.5,
            }),
        )
        .unwrap();
        // A NULL measurement (silence) must neither count as zero energy for
        // its duration nor be skipped: the album has no complete measurement.
        write_track_loudness(db.conn(), 2, source(42), None).unwrap();

        assert_eq!(album_measured_loudness(db.conn(), 1).unwrap(), None);
    }
}
