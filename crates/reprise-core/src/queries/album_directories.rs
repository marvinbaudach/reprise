//! Filesystem homes of the live tracks belonging to one album.

use std::path::PathBuf;

use crate::db::Db;

use super::library_views::EFFECTIVE_ALBUM_ARTIST;
use super::PRESENT;

/// Returns each distinct parent directory represented by a live track in the
/// case-insensitive album identity.
pub fn query_album_directories(
    db: &Db,
    album: &str,
    album_artist: &str,
) -> Result<Vec<PathBuf>, rusqlite::Error> {
    let sql = format!(
        "SELECT path FROM tracks \
         WHERE {PRESENT} \
           AND TRIM(album) = TRIM(?1) COLLATE NOCASE \
           AND {EFFECTIVE_ALBUM_ARTIST} = TRIM(?2) COLLATE NOCASE \
         ORDER BY path"
    );
    let conn = db.conn();
    let mut statement = conn.prepare(&sql)?;
    let paths = statement
        .query_map([album, album_artist], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut directories = paths
        .into_iter()
        .filter_map(|path| PathBuf::from(path).parent().map(PathBuf::from))
        .filter(|directory| !directory.as_os_str().is_empty())
        .collect::<Vec<_>>();
    directories.sort();
    directories.dedup();
    Ok(directories)
}

#[cfg(test)]
#[path = "album_directories_tests.rs"]
mod tests;
