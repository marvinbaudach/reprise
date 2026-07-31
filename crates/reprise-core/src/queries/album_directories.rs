//! Filesystem homes of the live tracks belonging to one album.

use std::path::{Path, PathBuf};

use crate::db::Db;

use super::library_views::EFFECTIVE_ALBUM_ARTIST;
use super::PRESENT;

/// Returns each parent directory that a live track of the case-insensitive
/// album identity lives in **and that holds no other album**.
///
/// The exclusivity condition is not tidiness, it is what keeps `COVER-1` from
/// writing a wrong file into a right-looking place. `(album, album_artist)` is
/// a fuzzy identity: a flat library resolves *every* album to the one folder
/// all its files sit in, and "Greatest Hits" by "Various Artists" or a
/// self-titled reissue collapses distinct releases into one row set. Writing
/// `cover.jpg` into such a directory does not fill a gap — `cover::resolve_source`
/// then serves that image as the folder cover for everything else in there,
/// in Reprise and in every other player and phone that reads the same
/// convention. A directory shared by several albums therefore has no album
/// cover to write, and this query simply does not return it.
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

    let mut exclusive = Vec::with_capacity(directories.len());
    for directory in directories {
        if holds_only(db, &directory, album, album_artist)? {
            exclusive.push(directory);
        }
    }
    Ok(exclusive)
}

/// Whether every live track sitting *directly* in `directory` belongs to the
/// given album identity.
///
/// The `LIKE` prefix narrows the scan to one subtree; the parent comparison
/// afterwards is what makes the answer exact, because `LIKE` matches nested
/// directories too and ASCII-folds case, and a bonus-disc folder underneath
/// an album folder must not disqualify the album folder itself.
fn holds_only(
    db: &Db,
    directory: &Path,
    album: &str,
    album_artist: &str,
) -> Result<bool, rusqlite::Error> {
    let sql = format!(
        "SELECT path FROM tracks \
         WHERE {PRESENT} \
           AND path LIKE ?1 ESCAPE '\\' \
           AND NOT (TRIM(album) = TRIM(?2) COLLATE NOCASE \
                    AND {EFFECTIVE_ALBUM_ARTIST} = TRIM(?3) COLLATE NOCASE)"
    );
    let conn = db.conn();
    let mut statement = conn.prepare(&sql)?;
    let mut foreign = statement.query_map(
        rusqlite::params![subtree_pattern(directory), album, album_artist],
        |row| row.get::<_, String>(0),
    )?;
    Ok(!foreign.any(|path| path.is_ok_and(|path| Path::new(&path).parent() == Some(directory))))
}

/// A `LIKE ... ESCAPE '\'` pattern matching every path inside `directory`.
fn subtree_pattern(directory: &Path) -> String {
    let mut pattern = directory
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    pattern.push(std::path::MAIN_SEPARATOR);
    pattern.push('%');
    pattern
}

#[cfg(test)]
#[path = "album_directories_tests.rs"]
mod tests;
