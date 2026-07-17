//! Live per-artist detail aggregates behind the Artists detail pane. All reads;
//! the time window takes its reference "now" as a parameter (UTC unix seconds)
//! so tests stay timezone- and clock-free, matching `library::stats_screen`.

use rusqlite::{params, Connection};

use crate::queries::library_views::EFFECTIVE_ALBUM_ARTIST;
use crate::queries::PRESENT;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistHeader {
    pub album_count: i64,
    pub track_count: i64,
    /// Σ duration_ms — total length of the artist's catalog in milliseconds.
    pub catalog_ms: i64,
    pub plays_this_year: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistTopTrack {
    pub track_id: i64,
    pub title: String,
    pub album: String,
    pub track_path: String,
    pub play_count: i64,
    pub duration_ms: i64,
}

pub fn artist_header(
    conn: &Connection,
    artist: &str,
    now_unix: i64,
) -> Result<ArtistHeader, rusqlite::Error> {
    // Start of the calendar year containing now_unix, UTC, as unix seconds.
    let year_start: i64 = conn.query_row(
        "SELECT CAST(strftime('%s', strftime('%Y', ?1, 'unixepoch') || '-01-01T00:00:00Z') AS INTEGER)",
        params![now_unix],
        |row| row.get(0),
    )?;
    let effective_album_artist_t2 =
        "CASE WHEN TRIM(t2.album_artist) <> '' THEN TRIM(t2.album_artist) ELSE TRIM(t2.artist) END";
    let sql = format!(
        "SELECT \
           COUNT(DISTINCT CASE WHEN TRIM(album) <> '' THEN LOWER(TRIM(album)) END), \
           COUNT(*), \
           COALESCE(SUM(duration_ms), 0), \
           ( SELECT COUNT(*) FROM listen_events le JOIN tracks t2 ON t2.id = le.track_id \
             WHERE {PRESENT} AND {effective_album_artist_t2} = ?1 COLLATE NOCASE \
             AND le.played_at >= ?2 AND le.played_at <= ?3 ) \
         FROM tracks \
         WHERE {PRESENT} AND {EFFECTIVE_ALBUM_ARTIST} = ?1 COLLATE NOCASE"
    );
    conn.query_row(&sql, params![artist.trim(), year_start, now_unix], |row| {
        Ok(ArtistHeader {
            album_count: row.get(0)?,
            track_count: row.get(1)?,
            catalog_ms: row.get(2)?,
            plays_this_year: row.get(3)?,
        })
    })
}

pub fn artist_top_tracks(
    conn: &Connection,
    artist: &str,
    limit: i64,
) -> Result<Vec<ArtistTopTrack>, rusqlite::Error> {
    let sql = format!(
        "SELECT id, title, album, path, play_count, duration_ms FROM tracks \
         WHERE {PRESENT} AND {EFFECTIVE_ALBUM_ARTIST} = ?1 COLLATE NOCASE \
         ORDER BY play_count DESC, last_played_at DESC, id ASC LIMIT ?2"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params![artist.trim(), limit], |row| {
        Ok(ArtistTopTrack {
            track_id: row.get(0)?,
            title: row.get(1)?,
            album: row.get(2)?,
            track_path: row.get(3)?,
            play_count: row.get(4)?,
            duration_ms: row.get(5)?,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded() -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (id,path,title,artist,album,album_artist,year,\
               duration_ms,play_count,last_played_at,added_at) VALUES
             (1,'/a.flac','A','Solo','One','Solo',2020,180000,5,100,0),
             (2,'/b.flac','B','Solo','One','Solo',2020,120000,2,50,0),
             (3,'/c.flac','C','Solo','Two','Solo',2022,200000,9,200,0);",
        )
        .unwrap();
        // one in-year event (2026-03), one prior-year (2025-03)
        conn.execute_batch(
            "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES
             (1, 1772582400, 180000),
             (3, 1741046400, 200000);",
        )
        .unwrap();
        conn
    }

    // 2026-07-15T00:00:00Z
    const NOW: i64 = 1_784_073_600;

    #[test]
    fn header_aggregates_counts_hours_and_year_plays() {
        let conn = seeded();
        let h = artist_header(&conn, "solo", NOW).unwrap();
        assert_eq!(h.track_count, 3);
        assert_eq!(h.album_count, 2);
        // Σ duration_ms = 180000 + 120000 + 200000 (catalog length, not weighted by play_count)
        assert_eq!(h.catalog_ms, 180_000 + 120_000 + 200_000);
        // only the 2026-03 event counts
        assert_eq!(h.plays_this_year, 1);
    }

    #[test]
    fn top_tracks_order_by_play_count_desc() {
        let conn = seeded();
        let top = artist_top_tracks(&conn, "solo", 5).unwrap();
        assert_eq!(
            top.iter().map(|t| t.track_id).collect::<Vec<_>>(),
            vec![3, 1, 2]
        );
    }

    #[test]
    fn detail_queries_are_read_only() {
        let conn = seeded();
        let before = conn.total_changes();
        artist_header(&conn, "solo", NOW).unwrap();
        artist_top_tracks(&conn, "solo", 5).unwrap();
        assert_eq!(conn.total_changes(), before);
    }

    // 2026-01-01T00:00:00Z — start of the calendar year containing NOW.
    const YEAR_START: i64 = 1_767_225_600;

    #[test]
    fn plays_this_year_includes_year_start_and_excludes_after_now() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (id,path,title,artist,album,album_artist,year,\
               duration_ms,play_count,last_played_at,added_at) VALUES
             (1,'/a.flac','A','Solo','One','Solo',2026,180000,1,0,0);",
        )
        .unwrap();
        // One event exactly at the year-start boundary (inclusive, `>=`):
        // must count. One event after NOW, later in the same year (the
        // upper bound is `<= now_unix`): must be excluded.
        conn.execute_batch(&format!(
            "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES
             (1, {YEAR_START}, 180000),
             (1, {}, 180000);",
            NOW + 10 * 24 * 60 * 60
        ))
        .unwrap();
        let h = artist_header(&conn, "solo", NOW).unwrap();
        assert_eq!(h.plays_this_year, 1);
    }

    #[test]
    fn top_tracks_tiebreak_by_last_played_then_id() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (id,path,title,artist,album,album_artist,year,\
               duration_ms,play_count,last_played_at,added_at) VALUES
             (1,'/a.flac','A','Solo','One','Solo',2020,180000,5,100,0),
             (2,'/b.flac','B','Solo','One','Solo',2020,120000,5,200,0),
             (3,'/c.flac','C','Solo','Two','Solo',2022,150000,3,300,0),
             (4,'/d.flac','D','Solo','Two','Solo',2022,150000,3,300,0);",
        )
        .unwrap();
        let top = artist_top_tracks(&conn, "solo", 10).unwrap();
        // Track 1 and 2 share play_count=5; track 2 has the later
        // last_played_at, so it must sort first (tiebreak `last_played_at
        // DESC`). Track 3 and 4 share both play_count=3 and last_played_at
        // 300, so the final tiebreak `id ASC` orders 3 before 4.
        assert_eq!(
            top.iter().map(|t| t.track_id).collect::<Vec<_>>(),
            vec![2, 1, 3, 4]
        );
    }
}
