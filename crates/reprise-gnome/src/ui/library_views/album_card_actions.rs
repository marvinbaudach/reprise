//! Album-level playback actions shared by the card play button and the
//! context menu. Each function resolves the album's track IDs via a DB
//! query, then delegates to the injected playback callbacks — never
//! touching `PlayerController` directly (same closure-injection seam the
//! track list uses).

use reprise_core::queries::{self, AlbumSummary};
use rusqlite::Connection;

/// Fetches track IDs for an album in canonical disc and track order.
pub(in crate::ui) fn album_track_ids(conn: &Connection, album: &AlbumSummary) -> Vec<i64> {
    queries::query_album_canonical_track_ids(conn, &album.album, &album.album_artist)
        .unwrap_or_default()
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
}
