//! Persistent scan exclusions created by explicit Remove-from-Library.

use std::path::Path;

use rusqlite::Connection;

use crate::db::Db;

pub(crate) use crate::db_library_exclusions::{
    park_segment_exclusions, place_segment_exclusion, SegmentPlacement,
};

/// The excluded tracks of a file, every one but the whole-file exclusion.
pub(crate) fn segment_exclusions(
    conn: &Connection,
    path: &str,
    device: Option<i64>,
    inode: Option<i64>,
) -> Result<Vec<crate::db_library_exclusions::SegmentExclusion>, rusqlite::Error> {
    crate::db_library_exclusions::segment_exclusions(conn, path, device, inode)
}

/// Records the current track identity only when both id and path still match
/// the caller's selection snapshot. A record for the same stable identity, or
/// for the same fallback path when no identity was available, takes the new
/// values in place.
///
/// The record carries the track's `segment_index`, so removing one track of a
/// CUE file hides that track and leaves its siblings; a whole-file track records
/// index `0`, which hides the file. A CUE track also records its start, its
/// title and the version of the sheet that cut it, so it stays hidden as the
/// same song when an edit of the sheet moves it to another position.
pub(crate) fn record_track(
    conn: &Connection,
    track_id: i64,
    expected_path: &Path,
    excluded_at: i64,
) -> Result<bool, rusqlite::Error> {
    crate::db_library_exclusions::record_track(
        conn,
        track_id,
        &expected_path.to_string_lossy(),
        excluded_at,
    )
}

/// Whether the whole file is excluded. An identity-bearing exclusion follows the
/// same file across renames. Legacy/unknown identities conservatively fall back
/// to their exact path. An exclusion of a single CUE track does not count here.
pub(crate) fn matches_file(
    conn: &Connection,
    path: &Path,
    device: Option<i64>,
    inode: Option<i64>,
) -> Result<bool, rusqlite::Error> {
    crate::db_library_exclusions::exists(conn, &path.to_string_lossy(), device, inode, 0)
}

/// Whether a file the catalog holds no track of is a CUE file whose every
/// track is hidden, unchanged since: the file still has `mtime`, and the sheet
/// that placed its hidden tracks is the one that governs it now, given as
/// `(path, mtime, size)`, or none for a sheet embedded in the file.
pub(crate) fn hidden_file_unchanged(
    conn: &Connection,
    path: &Path,
    device: Option<i64>,
    inode: Option<i64>,
    mtime: i64,
    governing: Option<(&str, i64, i64)>,
) -> Result<bool, rusqlite::Error> {
    let version = crate::db_library_exclusions::hidden_file_version(
        conn,
        &path.to_string_lossy(),
        device,
        inode,
    )?;
    Ok(version.is_some_and(|(hidden_mtime, sheet)| {
        hidden_mtime == mtime
            && sheet
                .as_ref()
                .map(|(path, mtime, size)| (path.as_str(), *mtime, *size))
                == governing
    }))
}

pub fn count(db: &Db) -> Result<u32, rusqlite::Error> {
    let conn = db.conn();
    conn.query_row("SELECT count(*) FROM library_exclusions", [], |row| {
        row.get(0)
    })
}

pub fn clear(db: &Db) -> Result<usize, rusqlite::Error> {
    let conn = db.conn();
    conn.execute("DELETE FROM library_exclusions", [])
}
