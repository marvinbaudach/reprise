//! Album-level playback actions shared by the card play button and the
//! context menu. Each function resolves the album's track IDs via a DB
//! query, then delegates to the injected playback callbacks — never
//! touching `PlayerController` directly (same closure-injection seam the
//! track list uses).

use gtk4::gio;
use gtk4::glib;
use reprise_core::queries::{self, AlbumSummary};
use rusqlite::Connection;

/// Fetches track IDs for an album in canonical disc and track order.
pub(in crate::ui) fn album_track_ids(conn: &Connection, album: &AlbumSummary) -> Vec<i64> {
    queries::query_album_canonical_track_ids(conn, &album.album, &album.album_artist)
        .unwrap_or_default()
}

/// Opens the parent folder of a track's path in the default file manager.
pub(in crate::ui) fn open_folder(representative_path: &str) {
    let path = std::path::Path::new(representative_path);
    let folder = path.parent().unwrap_or(path);
    let uri = match glib::filename_to_uri(folder, None) {
        Ok(uri) => uri,
        Err(error) => {
            tracing::warn!(%error, path = %folder.display(), "could not build folder URI");
            return;
        }
    };
    if let Err(error) = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>) {
        tracing::warn!(%error, %uri, "could not open folder");
    }
}

/// Shuffles a slice in-place using Fisher-Yates (the standard library's
/// `SliceRandom::shuffle` requires the `rand` crate; this is self-contained).
pub(in crate::ui) fn shuffle_ids(ids: &mut [i64]) {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let state = RandomState::new();
    for i in (1..ids.len()).rev() {
        let mut hasher = state.build_hasher();
        hasher.write_usize(i);
        let j = (hasher.finish() as usize) % (i + 1);
        ids.swap(i, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn album_track_ids_uses_canonical_disc_and_track_order() {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks
               (id,path,title,artist,album,album_artist,disc_no,track_no,added_at) VALUES
             (1,'/a.flac','Disc two','Art','Alb','',2,1,0),
             (2,'/b.flac','Legacy disc','Art','Alb','',NULL,2,0),
             (3,'/c.flac','Disc one','Art','Alb','',1,1,0);",
        )
        .unwrap();
        let album = AlbumSummary {
            album: "Alb".into(),
            album_artist: "Art".into(),
            representative_path: "/a.flac".into(),
            track_count: 3,
            year: None,
            total_duration_ms: 0,
            max_added_at: 0,
            total_play_count: 0,
        };

        let ids = album_track_ids(&conn, &album);
        assert_eq!(ids, vec![3, 2, 1]);
    }

    #[test]
    fn shuffle_ids_preserves_elements() {
        let mut ids = vec![1, 2, 3, 4, 5];
        shuffle_ids(&mut ids);
        ids.sort();
        assert_eq!(ids, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn shuffle_ids_handles_empty_and_single() {
        let mut empty: Vec<i64> = vec![];
        shuffle_ids(&mut empty);
        assert!(empty.is_empty());

        let mut single = vec![42];
        shuffle_ids(&mut single);
        assert_eq!(single, vec![42]);
    }
}
