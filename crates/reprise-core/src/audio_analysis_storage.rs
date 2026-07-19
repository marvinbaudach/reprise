use rusqlite::Connection;

use crate::db::DbError;
use crate::sound_profile::SourceFingerprint;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingWaveform {
    pub track_id: i64,
    pub path: String,
    pub source: SourceFingerprint,
}

pub fn pending_waveform_work(conn: &Connection) -> Result<Vec<PendingWaveform>, DbError> {
    let mut statement = conn.prepare(&format!(
        "SELECT id, path, file_mtime, file_size FROM tracks \
         WHERE waveform_peaks IS NULL AND {} ORDER BY id",
        crate::queries::PRESENT
    ))?;
    let rows = statement
        .query_map([], |row| {
            let mtime = row.get(2)?;
            let size = row.get(3)?;
            let source = SourceFingerprint::new(mtime, size).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    3,
                    rusqlite::types::Type::Integer,
                    Box::new(error),
                )
            })?;
            Ok(PendingWaveform {
                track_id: row.get(0)?,
                path: row.get(1)?,
                source,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn save_waveform_if_current(
    conn: &Connection,
    track_id: i64,
    source: SourceFingerprint,
    peaks: &[u8],
) -> Result<bool, DbError> {
    let changed = conn.execute(
        &format!(
            "UPDATE tracks SET waveform_peaks = ?1 \
             WHERE id = ?2 AND file_mtime = ?3 AND file_size = ?4 AND {}",
            crate::queries::PRESENT
        ),
        rusqlite::params![peaks, track_id, source.mtime(), source.size()],
    )?;
    Ok(changed == 1)
}

pub fn reset_failed_analyses(conn: &Connection) -> Result<u64, DbError> {
    let changed = conn.execute(
        "DELETE FROM track_audio_analysis WHERE status = 'failed'",
        [],
    )?;
    Ok(changed as u64)
}

/// Removes derived Audio Character rows so every present track becomes
/// eligible for analysis again. Track metadata and audio files are untouched.
pub fn reset_all_analyses(conn: &Connection) -> Result<u64, DbError> {
    let changed = conn.execute("DELETE FROM track_audio_analysis", [])?;
    Ok(changed as u64)
}
