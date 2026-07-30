//! Persistent scan exclusions created by explicit Remove-from-Library.

use std::path::Path;

use rusqlite::Connection;

use crate::db::Db;

/// Records the current track identity only when both id and path still match
/// the caller's selection snapshot. `INSERT OR REPLACE` retires an older
/// record for the same stable identity, or for the same fallback path when
/// no identity was available.
pub(crate) fn record_track(
    conn: &Connection,
    track_id: i64,
    expected_path: &Path,
    excluded_at: i64,
) -> Result<bool, rusqlite::Error> {
    let changed = conn.execute(
        "INSERT OR REPLACE INTO library_exclusions
         (path,device,inode,file_size,file_mtime,excluded_at)
         SELECT path,device,inode,file_size,file_mtime,?3 FROM tracks
         WHERE id=?1 AND path=?2",
        rusqlite::params![track_id, expected_path.to_string_lossy(), excluded_at],
    )?;
    Ok(changed == 1)
}

/// An identity-bearing exclusion follows the same file across renames.
/// Legacy/unknown identities conservatively fall back to their exact path.
pub(crate) fn matches_file(
    conn: &Connection,
    path: &Path,
    device: Option<i64>,
    inode: Option<i64>,
) -> Result<bool, rusqlite::Error> {
    conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM library_exclusions
           WHERE (device IS NOT NULL AND inode IS NOT NULL
                  AND device=?2 AND inode=?3)
              OR ((device IS NULL OR inode IS NULL) AND path=?1)
         )",
        rusqlite::params![path.to_string_lossy(), device, inode],
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

pub fn clear_paths(db: &Db, paths: &[&Path]) -> Result<usize, rusqlite::Error> {
    let conn = db.conn();
    let mut cleared = 0;
    for path in paths {
        cleared += conn.execute(
            "DELETE FROM library_exclusions WHERE path=?1",
            [path.to_string_lossy()],
        )?;
    }
    Ok(cleared)
}
