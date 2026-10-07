//! The marker a failed analysis leaves (schema v91, finding C8).
//!
//! A file whose rate or channel count changes mid-stream, a file truncated
//! before a track starts, or one the decoder cannot read at all fails the same
//! way on every attempt. Without a record of that, every backfill run decodes
//! it again. The marker is written against the source fingerprint and format
//! version a stored spectrogram would carry, so it holds exactly as long as a
//! stored result would: a changed file, a re-cut track (both invalidation
//! triggers delete the row) or a new analysis format makes the track pending
//! again. A later successful store clears it.

use rusqlite::{Connection, OptionalExtension};

use crate::db::{Db, DbError};
use crate::spectrogram::SPECTROGRAM_FORMAT_VERSION;

/// Joins the marker that still holds for the track `t`; a pending query keeps
/// the rows where `f.track_id IS NULL`. Binds the format version as `?1`.
pub(crate) const FAILURE_JOIN: &str = "LEFT JOIN render_data_failures f ON f.track_id = t.id \
       AND f.format_version = ?1 AND f.source_mtime = t.file_mtime \
       AND f.source_size = t.file_size AND f.source_device IS t.device \
       AND f.source_inode IS t.inode";

/// Remembers that the analysis of `track_id` failed for `reason`, against the
/// track's current source fingerprint. A track without a stat identity is
/// recorded with none; a track that no longer exists records nothing.
pub fn record_render_data_failure(db: &Db, track_id: i64, reason: &str) -> Result<(), DbError> {
    db.conn().execute(
        "INSERT INTO render_data_failures \
         (track_id, source_mtime, source_size, source_device, source_inode, \
          format_version, reason, failed_at) \
         SELECT id, file_mtime, file_size, device, inode, ?2, ?3, \
                CAST(strftime('%s', 'now') AS INTEGER) \
         FROM tracks WHERE id = ?1 \
         ON CONFLICT(track_id) DO UPDATE SET \
           source_mtime = excluded.source_mtime, source_size = excluded.source_size, \
           source_device = excluded.source_device, source_inode = excluded.source_inode, \
           format_version = excluded.format_version, reason = excluded.reason, \
           failed_at = excluded.failed_at",
        rusqlite::params![track_id, SPECTROGRAM_FORMAT_VERSION, reason],
    )?;
    Ok(())
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
