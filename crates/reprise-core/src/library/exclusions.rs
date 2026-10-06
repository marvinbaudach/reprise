//! Persistent scan exclusions created by explicit Remove-from-Library.

use std::path::Path;

use rusqlite::Connection;

use crate::db::Db;

/// Records the current track identity only when both id and path still match
/// the caller's selection snapshot. `INSERT OR REPLACE` retires an older
/// record for the same stable identity, or for the same fallback path when
/// no identity was available.
///
/// The record carries the track's `segment_index`, so removing one track of a
/// CUE file hides that track and leaves its siblings; a whole-file track records
/// index `0`, which hides the file.
pub(crate) fn record_track(
    conn: &Connection,
    track_id: i64,
    expected_path: &Path,
    excluded_at: i64,
) -> Result<bool, rusqlite::Error> {
    let changed = conn.execute(
        "INSERT OR REPLACE INTO library_exclusions
         (path,device,inode,file_size,file_mtime,excluded_at,segment_index)
         SELECT path,device,inode,file_size,file_mtime,?3,segment_index FROM tracks
         WHERE id=?1 AND path=?2",
        rusqlite::params![track_id, expected_path.to_string_lossy(), excluded_at],
    )?;
    Ok(changed == 1)
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
    matches_segment(conn, path, device, inode, 0)
}

/// Whether track `segment_index` of the file, `0` meaning the file as a whole,
/// is excluded.
pub(crate) fn matches_segment(
    conn: &Connection,
    path: &Path,
    device: Option<i64>,
    inode: Option<i64>,
    segment_index: i64,
) -> Result<bool, rusqlite::Error> {
    conn.prepare_cached(
        "SELECT EXISTS(
           SELECT 1 FROM library_exclusions
           WHERE segment_index=?4
             AND ((device IS NOT NULL AND inode IS NOT NULL
                   AND device=?2 AND inode=?3)
               OR ((device IS NULL OR inode IS NULL) AND path=?1))
         )",
    )?
    .query_row(
        rusqlite::params![path.to_string_lossy(), device, inode, segment_index],
        |row| row.get(0),
    )
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
