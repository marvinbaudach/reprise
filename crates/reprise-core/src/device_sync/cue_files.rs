//! The CUE files a sync selection reaches, as the mirror plan needs them.
//!
//! A file a CUE sheet cut into tracks reaches the device once, whatever number
//! of its tracks are selected, together with a sheet derived for the device
//! (CUE-15). The plan therefore needs, per such file, every track of it still
//! in the library: the selected ones to share the file's device path, and all
//! of them for the derived sheet, since the phone lists every track of a
//! synced file.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use rusqlite::Connection;

/// One track of a CUE file, in the file's play order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CueSyncTrack {
    pub track_id: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub title: String,
    pub performer: String,
    pub track_no: Option<u32>,
}

/// A source file a CUE sheet cut into tracks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CueSyncFile {
    pub source_path: PathBuf,
    /// The album the tracks carry, which a derived sheet names as its title.
    pub album: String,
    /// The album artist, which a derived sheet names as its performer.
    pub album_artist: String,
    pub year: Option<i32>,
    pub genre: String,
    /// Where the last track ends: the length a transcode of the file has.
    pub duration_ms: i64,
    /// Every track of the file still in the library, in play order.
    pub tracks: Vec<CueSyncTrack>,
}

impl CueSyncFile {
    pub fn track_ids(&self) -> impl Iterator<Item = i64> + '_ {
        self.tracks.iter().map(|track| track.track_id)
    }
}

/// `snapshots`, each with the CUE files its entries' tracks were cut from.
pub(super) fn with_cue_files(
    conn: &Connection,
    snapshots: Vec<super::MirrorPlaylistSnapshot>,
) -> Result<Vec<super::MirrorPlaylistSnapshot>, rusqlite::Error> {
    let ids_of = |snapshot: &super::MirrorPlaylistSnapshot| -> HashSet<i64> {
        snapshot
            .entries
            .iter()
            .map(|entry| match entry {
                super::MirrorTrack::Available(track) => track.id,
                super::MirrorTrack::Unavailable(track) => track.track_id,
            })
            .collect()
    };
    let every_id: HashSet<i64> = snapshots.iter().flat_map(ids_of).collect();
    let files = load_cue_files(conn, &every_id)?;
    Ok(snapshots
        .into_iter()
        .map(|snapshot| {
            let ids = ids_of(&snapshot);
            let cue_files = files
                .iter()
                .filter(|file| file.track_ids().any(|id| ids.contains(&id)))
                .cloned()
                .collect();
            super::MirrorPlaylistSnapshot {
                cue_files,
                ..snapshot
            }
        })
        .collect())
}

/// The CUE files that hold at least one of `track_ids`.
fn load_cue_files(
    conn: &Connection,
    track_ids: &HashSet<i64>,
) -> Result<Vec<CueSyncFile>, rusqlite::Error> {
    if track_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut statement = conn.prepare(
        "SELECT id, path, segment_start_ms, segment_end_ms, title, artist, track_no,
                album, album_artist, year, genre
         FROM tracks
         WHERE segment_index > 0 AND removed_at IS NULL
           AND segment_start_ms IS NOT NULL AND segment_end_ms IS NOT NULL
         ORDER BY path, segment_index",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(1)?,
            CueSyncTrack {
                track_id: row.get(0)?,
                start_ms: row.get(2)?,
                end_ms: row.get(3)?,
                title: row.get(4)?,
                performer: row.get(5)?,
                track_no: row.get(6)?,
            },
            (
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, Option<i32>>(9)?,
                row.get::<_, String>(10)?,
            ),
        ))
    })?;
    let mut files: Vec<CueSyncFile> = Vec::new();
    let mut positions: HashMap<String, usize> = HashMap::new();
    for row in rows {
        let (path, track, (album, album_artist, year, genre)) = row?;
        let position = *positions.entry(path.clone()).or_insert_with(|| {
            files.push(CueSyncFile {
                source_path: PathBuf::from(&path),
                album,
                album_artist,
                year,
                genre,
                duration_ms: 0,
                tracks: Vec::new(),
            });
            files.len() - 1
        });
        let file = &mut files[position];
        file.duration_ms = file.duration_ms.max(track.end_ms);
        file.tracks.push(track);
    }
    Ok(files
        .into_iter()
        .filter(|file| file.track_ids().any(|id| track_ids.contains(&id)))
        .collect())
}
