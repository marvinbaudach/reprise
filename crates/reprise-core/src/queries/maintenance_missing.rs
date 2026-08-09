//! Missing-file marking triggered by a playback fault.
//!
//! Kept apart from the broader maintenance query module so both files remain
//! below the repository's code-file limit.

use std::path::Path;

use rusqlite::OptionalExtension;

use crate::db::Db;
use crate::library::source::{
    LibraryLinkMode, LibraryPathPresence, LibrarySource, UnixLibrarySource,
};

use super::clauses::PRESENT;

/// Marks `track_id` missing only while it is still the live row at
/// `expected_path`. The expected path is the identity snapshot taken before
/// the asynchronous backend fault arrived. Both the read and write require
/// that same path plus `PRESENT`, and the file is rechecked immediately before
/// writing, so a concurrent watcher/Locate repair wins the race.
///
/// The source supplies the same reachability verdict and mount-point evidence
/// as the scanner's vanish phase. Returns whether one row changed.
pub fn mark_track_missing_if_current(
    db: &Db,
    track_id: i64,
    expected_path: &Path,
) -> Result<bool, rusqlite::Error> {
    mark_track_missing_if_current_with(&UnixLibrarySource, db, track_id, expected_path)
}

/// [`mark_track_missing_if_current`] with the library source injected.
/// Production passes [`UnixLibrarySource`]; the seam keeps opaque sources and
/// source failures testable without a real filesystem.
pub(super) fn mark_track_missing_if_current_with(
    source: &dyn LibrarySource,
    db: &Db,
    track_id: i64,
    expected_path: &Path,
) -> Result<bool, rusqlite::Error> {
    let conn = db.conn();
    let expected_path = expected_path.to_string_lossy();
    let row: Option<(String, Option<i64>)> = conn
        .query_row(
            &format!("SELECT path,device FROM tracks WHERE id=?1 AND path=?2 AND {PRESENT}"),
            rusqlite::params![track_id, expected_path.as_ref()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((path, device)) = row else {
        return Ok(false);
    };
    let source_path = Path::new(&path);
    if source.probe(source_path, LibraryLinkMode::Follow) != LibraryPathPresence::Absent {
        return Ok(false);
    }
    let reason = source.reachability(source_path, device);
    let mount_point = source
        .mount_point(source_path)
        .map(|mount| mount.to_string_lossy().into_owned());
    let changed = conn.execute(
        &format!(
            "UPDATE tracks SET missing_since=strftime('%s','now'),missing_reason=?3, \
             mount_point=?4 WHERE id=?1 AND path=?2 AND {PRESENT}"
        ),
        rusqlite::params![track_id, path, reason.as_str(), mount_point],
    )?;
    Ok(changed == 1)
}
