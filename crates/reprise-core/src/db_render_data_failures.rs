//! The marker a failed analysis leaves (schema v91, finding C8).
//!
//! A file whose rate or channel count changes mid-stream, a file truncated
//! before a track starts, or one the decoder cannot read at all fails the same
//! way on every attempt. Without a record of that, every backfill run decodes
//! it again. The marker is written against the source fingerprint and format
//! version a stored spectrogram would carry, and only while the fingerprint is
//! still the one the failed decode began on, so it holds exactly as long as a
//! stored result would: a changed file, a re-cut track (both invalidation
//! triggers delete the row) or a new analysis format makes the track pending
//! again. A later successful store clears it.

use rusqlite::{Connection, OptionalExtension};

use super::SpectrogramStoreOutcome;
use crate::db::{Db, DbError};
use crate::render_data_segments::SegmentBounds;
use crate::spectrogram::{TrackSourceFingerprint, SPECTROGRAM_FORMAT_VERSION};

/// Joins the marker that still holds for the track `t`; a pending query keeps
/// the rows where `f.track_id IS NULL`. Binds the format version as `?1`.
pub(crate) const FAILURE_JOIN: &str = "LEFT JOIN render_data_failures f ON f.track_id = t.id \
       AND f.format_version = ?1 AND f.source_mtime = t.file_mtime \
       AND f.source_size = t.file_size AND f.source_device IS t.device \
       AND f.source_inode IS t.inode";

/// Remembers that the analysis of `track_id` failed for `reason`, against the
/// source fingerprint `source` the failed decode began on and, for a CUE
/// track, the cut `bounds` it measured. Records nothing and returns
/// [`SpectrogramStoreOutcome::SourceChanged`] when either changed while the
/// decode ran, or the track is gone, exactly as a store would: a decode that
/// failed on a file being rewritten says nothing about the file it became, and
/// marking that one would hide it until it changed again.
pub fn record_render_data_failure(
    db: &Db,
    track_id: i64,
    source: TrackSourceFingerprint,
    bounds: Option<SegmentBounds>,
    reason: &str,
) -> Result<SpectrogramStoreOutcome, DbError> {
    let transaction = db.conn().unchecked_transaction()?;
    if !super::still_current(&transaction, track_id, source, bounds)? {
        return Ok(SpectrogramStoreOutcome::SourceChanged);
    }
    transaction.execute(
        "INSERT INTO render_data_failures \
         (track_id, source_mtime, source_size, source_device, source_inode, \
          format_version, reason, failed_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, CAST(strftime('%s', 'now') AS INTEGER)) \
         ON CONFLICT(track_id) DO UPDATE SET \
           source_mtime = excluded.source_mtime, source_size = excluded.source_size, \
           source_device = excluded.source_device, source_inode = excluded.source_inode, \
           format_version = excluded.format_version, reason = excluded.reason, \
           failed_at = excluded.failed_at",
        rusqlite::params![
            track_id,
            source.mtime_seconds,
            source.size_bytes,
            source.device,
            source.inode,
            SPECTROGRAM_FORMAT_VERSION,
            reason
        ],
    )?;
    transaction.commit()?;
    Ok(SpectrogramStoreOutcome::Stored)
}

/// Whether a failure recorded for `track_id` still holds: recorded against the
/// file as it is now and the current analysis format.
pub fn render_data_failed(db: &Db, track_id: i64) -> Result<bool, DbError> {
    let found = db
        .conn()
        .query_row(
            &format!(
                "SELECT 1 FROM tracks t {FAILURE_JOIN} WHERE t.id = ?2 AND f.track_id IS NOT NULL"
            ),
            rusqlite::params![SPECTROGRAM_FORMAT_VERSION, track_id],
            |_| Ok(()),
        )
        .optional()?;
    Ok(found.is_some())
}

/// Forgets any failure recorded for `track_id`.
pub fn clear_render_data_failure(db: &Db, track_id: i64) -> Result<(), DbError> {
    Ok(clear_failure(db.conn(), track_id)?)
}

/// [`clear_render_data_failure`] inside a caller's transaction.
pub(crate) fn clear_failure(conn: &Connection, track_id: i64) -> Result<(), rusqlite::Error> {
    conn.execute(
        "DELETE FROM render_data_failures WHERE track_id = ?1",
        [track_id],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "db_render_data_failures_tests.rs"]
mod tests;
