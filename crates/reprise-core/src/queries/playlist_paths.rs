//! What the path lines of a playlist file stand for in the library.
//!
//! A line names a file. For a file a CUE sheet cuts into tracks, that is every
//! one of its tracks still in the library, in the order they play, just as
//! opening the file queues them. A playlist exported from Reprise writes the
//! file's path once for each of its tracks, so a run of lines naming the same
//! cut file stands for it once: an exported album comes back as the album, not
//! as the album once per track. A single track of such a file comes back as the
//! whole file, since an M3U line cannot name a stretch of one.

use crate::db::Db;

/// The tracks one path line of a playlist file stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathTracks {
    /// The path as the library stores it.
    pub path: String,
    /// In play order.
    pub ids: Vec<i64>,
    /// Whether a CUE sheet cuts the file into tracks.
    pub cut: bool,
}

/// The tracks of the file at exactly `path` that are not removed from the
/// library, in play order; `None` when there are none.
pub fn playlist_tracks_for_path(
    db: &Db,
    path: &str,
) -> Result<Option<PathTracks>, rusqlite::Error> {
    let conn = db.conn();
    let mut statement = conn.prepare(
        "SELECT id, segment_index FROM tracks \
         WHERE path = ?1 AND removed_at IS NULL ORDER BY segment_index, id",
    )?;
    let rows: Vec<(i64, i64)> = statement
        .query_map([path], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    if rows.is_empty() {
        return Ok(None);
    }
    Ok(Some(PathTracks {
        path: path.to_owned(),
        cut: rows.iter().any(|(_, index)| *index > 0),
        ids: rows.into_iter().map(|(id, _)| id).collect(),
    }))
}

/// The track ids a playlist holds whose lines resolved to `lines`, in order.
pub fn playlist_ids(lines: &[PathTracks]) -> Vec<i64> {
    let mut ids = Vec::new();
    let mut previous: Option<&PathTracks> = None;
    for line in lines {
        let repeats_a_cut_file =
            line.cut && previous.is_some_and(|previous| previous.path == line.path);
        if !repeats_a_cut_file {
            ids.extend_from_slice(&line.ids);
        }
        previous = Some(line);
    }
    ids
}
