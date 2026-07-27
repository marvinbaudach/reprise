//! Database adapter for point-in-time manual and smart playlist snapshots.

use std::path::PathBuf;

use rusqlite::Connection;

use super::{MirrorPlaylistSnapshot, MirrorTrack, SelectionSource, SyncTrack, UnavailableTrack};

pub fn load_mirror_playlist_snapshots(
    conn: &Connection,
) -> Result<Vec<MirrorPlaylistSnapshot>, rusqlite::Error> {
    let mut snapshots = Vec::new();
    for playlist in crate::library::playlists::list(conn)? {
        snapshots.push(MirrorPlaylistSnapshot {
            source: SelectionSource::Playlist(playlist.id),
            name: playlist.name,
            entries: load_manual_entries(conn, playlist.id)?,
        });
    }
    for playlist in crate::library::playlists::list_smart(conn)? {
        let source = crate::view_source::ViewSource::Smart(playlist.id);
        let ids = crate::queries::query_track_ids(conn, &source, "title", "asc", "", &[])?;
        let tracks = crate::queries::query_sync_tracks(conn, &ids)?;
        snapshots.push(MirrorPlaylistSnapshot {
            source: SelectionSource::Smart(playlist.id),
            name: playlist.name,
            entries: tracks.into_iter().map(MirrorTrack::Available).collect(),
        });
    }
    Ok(snapshots)
}

fn load_manual_entries(
    conn: &Connection,
    playlist_id: i64,
) -> Result<Vec<MirrorTrack>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT tracks.id, tracks.path, tracks.title, tracks.artist, tracks.album, \
                tracks.album_artist, tracks.track_no, tracks.duration_ms, \
                tracks.bitrate_kbps, tracks.missing_since \
         FROM playlist_tracks \
         JOIN tracks ON tracks.id = playlist_tracks.track_id \
         WHERE playlist_tracks.playlist_id = ?1 AND tracks.removed_at IS NULL \
         ORDER BY playlist_tracks.position",
    )?;
    let rows = statement.query_map([playlist_id], |row| {
        Ok(SnapshotTrack {
            id: row.get(0)?,
            path: row.get(1)?,
            title: row.get(2)?,
            artist: row.get(3)?,
            album: row.get(4)?,
            album_artist: row.get(5)?,
            track_number: row.get(6)?,
            duration_ms: row.get(7)?,
            bitrate_kbps: row.get(8)?,
            missing_since: row.get(9)?,
        })
    })?;
    rows.map(|row| row.map(SnapshotTrack::into_mirror_track))
        .collect()
}

struct SnapshotTrack {
    id: i64,
    path: String,
    title: String,
    artist: String,
    album: String,
    album_artist: String,
    track_number: Option<u32>,
    duration_ms: i64,
    bitrate_kbps: Option<i64>,
    missing_since: Option<i64>,
}

impl SnapshotTrack {
    fn into_mirror_track(self) -> MirrorTrack {
        if self.missing_since.is_some() {
            return self.unavailable();
        }
        let source_path = PathBuf::from(&self.path);
        let Ok(metadata) = source_path.metadata() else {
            return self.unavailable();
        };
        if !metadata.is_file() {
            return self.unavailable();
        }
        let Some(original_name) = source_path.file_name() else {
            return self.unavailable();
        };
        MirrorTrack::Available(SyncTrack {
            id: self.id,
            source_path: source_path.clone(),
            original_name: original_name.to_string_lossy().into_owned(),
            title: self.title,
            artist: self.artist,
            album: self.album,
            album_artist: self.album_artist,
            track_number: self.track_number,
            duration_ms: self.duration_ms,
            bitrate_kbps: self
                .bitrate_kbps
                .and_then(|value| u32::try_from(value).ok())
                .filter(|value| *value > 0),
            size_bytes: metadata.len(),
            source_mtime: metadata
                .modified()
                .ok()
                .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                .and_then(|duration| i64::try_from(duration.as_secs()).ok())
                .unwrap_or(0),
        })
    }

    fn unavailable(self) -> MirrorTrack {
        MirrorTrack::Unavailable(UnavailableTrack {
            track_id: self.id,
            title: self.title,
            artist: self.artist,
            duration_ms: self.duration_ms,
        })
    }
}
