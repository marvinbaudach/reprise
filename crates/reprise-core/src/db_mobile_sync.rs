//! Persistence for lazily imported desktop analysis sidecars.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension};

use crate::db::{Db, DbError};
use crate::spectrogram::TrackSourceFingerprint;

const SCHEMA_V61: &str = r#"
CREATE TABLE IF NOT EXISTS track_analysis_sidecars (
  track_id                 INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
  sidecar_path             TEXT NOT NULL,
  imported_source_mtime    INTEGER,
  imported_source_size     INTEGER,
  imported_source_device   INTEGER,
  imported_source_inode    INTEGER,
  CHECK (
    (imported_source_mtime IS NULL AND imported_source_size IS NULL
      AND imported_source_device IS NULL AND imported_source_inode IS NULL)
    OR (imported_source_mtime IS NOT NULL AND imported_source_size IS NOT NULL)
  )
);
"#;

pub(crate) fn migrate_v61(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 61 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V61)?;
    transaction.pragma_update(None, "user_version", 61)?;
    transaction.commit()
}

pub(crate) fn register_sidecar(
    conn: &Connection,
    track_path: &str,
    sidecar_path: &Path,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO track_analysis_sidecars (track_id, sidecar_path) \
         SELECT id, ?2 FROM tracks WHERE path = ?1 \
         ON CONFLICT(track_id) DO UPDATE SET sidecar_path = excluded.sidecar_path",
        rusqlite::params![track_path, sidecar_path.to_string_lossy()],
    )?;
    Ok(())
}

pub(crate) fn unregister_sidecar(
    conn: &Connection,
    track_path: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "DELETE FROM track_analysis_sidecars WHERE track_id = \
         (SELECT id FROM tracks WHERE path = ?1)",
        [track_path],
    )?;
    Ok(())
}

pub(crate) struct AnalysisSidecarState {
    pub path: String,
    pub imported_source: Option<TrackSourceFingerprint>,
}

pub(crate) fn analysis_sidecar_state(
    db: &Db,
    track_id: i64,
) -> Result<Option<AnalysisSidecarState>, DbError> {
    Ok(db
        .conn()
        .query_row(
            "SELECT sidecar_path, imported_source_mtime, imported_source_size, \
                    imported_source_device, imported_source_inode \
             FROM track_analysis_sidecars WHERE track_id = ?1",
            [track_id],
            |row| {
                let mtime = row.get::<_, Option<i64>>(1)?;
                let size = row.get::<_, Option<i64>>(2)?;
                let device = row.get(3)?;
                let inode = row.get(4)?;
                Ok(AnalysisSidecarState {
                    path: row.get(0)?,
                    imported_source: mtime.zip(size).map(|(mtime_seconds, size_bytes)| {
                        TrackSourceFingerprint {
                            mtime_seconds,
                            size_bytes,
                            device,
                            inode,
                        }
                    }),
                })
            },
        )
        .optional()?)
}

pub(crate) fn record_imported_source(
    db: &Db,
    track_id: i64,
    sidecar_path: &str,
    source: TrackSourceFingerprint,
) -> Result<(), DbError> {
    db.conn().execute(
        "UPDATE track_analysis_sidecars SET imported_source_mtime = ?1, \
         imported_source_size = ?2, imported_source_device = ?3, imported_source_inode = ?4 \
         WHERE track_id = ?5 AND sidecar_path = ?6",
        rusqlite::params![
            source.mtime_seconds,
            source.size_bytes,
            source.device,
            source.inode,
            track_id,
            sidecar_path,
        ],
    )?;
    Ok(())
}
