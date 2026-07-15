//! Read-only projections and detail queries for the visual library views.

use crate::models::Track;
use rusqlite::types::Value;
use rusqlite::Connection;

use super::clauses::{filter_clause, like_pattern, order_expr_and_dir, row_to_id, row_to_track};
use super::queue::QUEUE_LIMIT;
use super::MAX_WINDOW_LIMIT;

const EFFECTIVE_ALBUM_ARTIST: &str =
    "CASE WHEN TRIM(album_artist) <> '' THEN TRIM(album_artist) ELSE TRIM(artist) END";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumSummary {
    pub album: String,
    pub album_artist: String,
    pub representative_path: String,
    pub track_count: i64,
    pub year: Option<i32>,
    pub total_duration_ms: i64,
    pub max_added_at: i64,
    pub total_play_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistSummary {
    pub artist: String,
    pub track_count: i64,
    pub album_count: i64,
}

/// Returns one row per case-insensitive `(album, effective album artist)`
/// pair. Blank albums and missing tracks are excluded; the lowest track id
/// supplies stable display spelling and the representative cover path.
pub fn query_albums(conn: &Connection) -> Result<Vec<AlbumSummary>, rusqlite::Error> {
    let sql = format!(
        "WITH grouped AS ( \
           SELECT LOWER(TRIM(album)) AS album_key, \
                  LOWER({EFFECTIVE_ALBUM_ARTIST}) AS artist_key, \
                  MIN(id) AS representative_id, \
                  COUNT(*) AS track_count, \
                  MIN(CASE WHEN year > 0 THEN year END) AS year, \
                  SUM(duration_ms) AS total_duration_ms, \
                  MAX(added_at) AS max_added_at, \
                  SUM(play_count) AS total_play_count \
           FROM tracks \
           WHERE missing = 0 AND TRIM(album) <> '' \
           GROUP BY album_key, artist_key \
         ) \
         SELECT TRIM(tracks.album), {EFFECTIVE_ALBUM_ARTIST}, tracks.path, \
                grouped.track_count, grouped.year, \
                COALESCE(grouped.total_duration_ms, 0), \
                COALESCE(grouped.max_added_at, 0), \
                COALESCE(grouped.total_play_count, 0) \
         FROM grouped JOIN tracks ON tracks.id = grouped.representative_id \
         ORDER BY TRIM(tracks.album) COLLATE NOCASE ASC, \
                  {EFFECTIVE_ALBUM_ARTIST} COLLATE NOCASE ASC"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map([], |row| {
        Ok(AlbumSummary {
            album: row.get(0)?,
            album_artist: row.get(1)?,
            representative_path: row.get(2)?,
            track_count: row.get(3)?,
            year: row.get(4)?,
            total_duration_ms: row.get(5)?,
            max_added_at: row.get(6)?,
            total_play_count: row.get(7)?,
        })
    })?;
    rows.collect()
}

/// Returns one row per case-insensitive track artist. Blank artists and
/// missing tracks are excluded; album counts ignore blank album values.
pub fn query_artists(conn: &Connection) -> Result<Vec<ArtistSummary>, rusqlite::Error> {
    let sql = "WITH grouped AS ( \
                 SELECT LOWER(TRIM(artist)) AS artist_key, MIN(id) AS representative_id, \
                        COUNT(*) AS track_count, \
                        COUNT(DISTINCT CASE WHEN TRIM(album) <> '' \
                                      THEN LOWER(TRIM(album)) END) AS album_count \
                 FROM tracks \
                 WHERE missing = 0 AND TRIM(artist) <> '' \
                 GROUP BY artist_key \
               ) \
               SELECT TRIM(tracks.artist), grouped.track_count, grouped.album_count \
               FROM grouped JOIN tracks ON tracks.id = grouped.representative_id \
               ORDER BY TRIM(tracks.artist) COLLATE NOCASE ASC";
    let mut statement = conn.prepare(sql)?;
    let rows = statement.query_map([], |row| {
        Ok(ArtistSummary {
            artist: row.get(0)?,
            track_count: row.get(1)?,
            album_count: row.get(2)?,
        })
    })?;
    rows.collect()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn query_album_track_window(
    conn: &mut Connection,
    album: &str,
    album_artist: &str,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<Track>, rusqlite::Error> {
    let limit = limit.clamp(0, MAX_WINDOW_LIMIT);
    let has_filter = !filter.trim().is_empty();
    let (order, direction) = order_expr_and_dir(sort_field, sort_dir);
    let filter_sql = filter_clause(has_filter, 5);
    let sql = format!(
        "SELECT id, path, title, artist, album, album_artist, year, track_no, genre, \
         duration_ms, bitrate_kbps, rating, play_count, last_played_at, added_at, \
         file_mtime, missing, file_size, device, inode \
         FROM tracks WHERE missing = 0 \
         AND TRIM(album) = ?3 COLLATE NOCASE \
         AND {EFFECTIVE_ALBUM_ARTIST} = ?4 COLLATE NOCASE{filter_sql} \
         ORDER BY {order} {direction} LIMIT ?1 OFFSET ?2"
    );
    let mut params = vec![
        Value::Integer(limit),
        Value::Integer(offset),
        Value::Text(album.trim().to_string()),
        Value::Text(album_artist.trim().to_string()),
    ];
    if has_filter {
        params.push(Value::Text(like_pattern(filter.trim())));
    }
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(rusqlite::params_from_iter(params), row_to_track)?;
    rows.collect()
}

pub(super) fn query_album_track_count(
    conn: &Connection,
    album: &str,
    album_artist: &str,
    filter: &str,
) -> Result<i64, rusqlite::Error> {
    let has_filter = !filter.trim().is_empty();
    let filter_sql = filter_clause(has_filter, 3);
    let sql = format!(
        "SELECT count(*) FROM tracks WHERE missing = 0 \
         AND TRIM(album) = ?1 COLLATE NOCASE \
         AND {EFFECTIVE_ALBUM_ARTIST} = ?2 COLLATE NOCASE{filter_sql}"
    );
    let mut params = vec![
        Value::Text(album.trim().to_string()),
        Value::Text(album_artist.trim().to_string()),
    ];
    if has_filter {
        params.push(Value::Text(like_pattern(filter.trim())));
    }
    conn.query_row(&sql, rusqlite::params_from_iter(params), |row| row.get(0))
}

pub(super) fn query_album_track_ids(
    conn: &Connection,
    album: &str,
    album_artist: &str,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
) -> Result<Vec<i64>, rusqlite::Error> {
    let has_filter = !filter.trim().is_empty();
    let (order, direction) = order_expr_and_dir(sort_field, sort_dir);
    let filter_sql = filter_clause(has_filter, 3);
    let sql = format!(
        "SELECT id FROM tracks WHERE missing = 0 \
         AND TRIM(album) = ?1 COLLATE NOCASE \
         AND {EFFECTIVE_ALBUM_ARTIST} = ?2 COLLATE NOCASE{filter_sql} \
         ORDER BY {order} {direction} LIMIT {QUEUE_LIMIT}"
    );
    let mut params = vec![
        Value::Text(album.trim().to_string()),
        Value::Text(album_artist.trim().to_string()),
    ];
    if has_filter {
        params.push(Value::Text(like_pattern(filter.trim())));
    }
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(rusqlite::params_from_iter(params), row_to_id)?;
    rows.collect()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn query_artist_track_window(
    conn: &mut Connection,
    artist: &str,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<Track>, rusqlite::Error> {
    let limit = limit.clamp(0, MAX_WINDOW_LIMIT);
    let has_filter = !filter.trim().is_empty();
    let (order, direction) = order_expr_and_dir(sort_field, sort_dir);
    let filter_sql = filter_clause(has_filter, 4);
    let sql = format!(
        "SELECT id, path, title, artist, album, album_artist, year, track_no, genre, \
         duration_ms, bitrate_kbps, rating, play_count, last_played_at, added_at, \
         file_mtime, missing, file_size, device, inode \
         FROM tracks WHERE missing = 0 \
         AND TRIM(artist) = ?3 COLLATE NOCASE{filter_sql} \
         ORDER BY {order} {direction} LIMIT ?1 OFFSET ?2"
    );
    let mut params = vec![
        Value::Integer(limit),
        Value::Integer(offset),
        Value::Text(artist.trim().to_string()),
    ];
    if has_filter {
        params.push(Value::Text(like_pattern(filter.trim())));
    }
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(rusqlite::params_from_iter(params), row_to_track)?;
    rows.collect()
}

pub(super) fn query_artist_track_count(
    conn: &Connection,
    artist: &str,
    filter: &str,
) -> Result<i64, rusqlite::Error> {
    let has_filter = !filter.trim().is_empty();
    let filter_sql = filter_clause(has_filter, 2);
    let sql = format!(
        "SELECT count(*) FROM tracks WHERE missing = 0 \
         AND TRIM(artist) = ?1 COLLATE NOCASE{filter_sql}"
    );
    let mut params = vec![Value::Text(artist.trim().to_string())];
    if has_filter {
        params.push(Value::Text(like_pattern(filter.trim())));
    }
    conn.query_row(&sql, rusqlite::params_from_iter(params), |row| row.get(0))
}

pub(super) fn query_artist_track_ids(
    conn: &Connection,
    artist: &str,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
) -> Result<Vec<i64>, rusqlite::Error> {
    let has_filter = !filter.trim().is_empty();
    let (order, direction) = order_expr_and_dir(sort_field, sort_dir);
    let filter_sql = filter_clause(has_filter, 2);
    let sql = format!(
        "SELECT id FROM tracks WHERE missing = 0 \
         AND TRIM(artist) = ?1 COLLATE NOCASE{filter_sql} \
         ORDER BY {order} {direction} LIMIT {QUEUE_LIMIT}"
    );
    let mut params = vec![Value::Text(artist.trim().to_string())];
    if has_filter {
        params.push(Value::Text(like_pattern(filter.trim())));
    }
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(rusqlite::params_from_iter(params), row_to_id)?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::{query_track_count, query_track_ids, query_track_window};
    use crate::view_source::ViewSource;

    fn seeded_library() -> rusqlite::Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks
               (id,path,title,artist,album,album_artist,added_at,missing) VALUES
             (1,'/music/first-a.flac','A','Solo',' First ','',0,0),
             (2,'/music/first-b.flac','B','Solo','first','',0,0),
             (3,'/music/other.flac','Other','Other Artist','First','',0,0),
             (4,'/music/mix-a.flac','Mix A','Guest A','Compilation','Various Artists',0,0),
             (5,'/music/mix-b.flac','Mix B','Guest B','Compilation','Various Artists',0,0),
             (6,'/music/blank.flac','Blank','Nobody','','',0,0),
             (7,'/music/missing.flac','Missing','Solo','Lost','',0,1);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn albums_group_by_trimmed_case_insensitive_title_and_effective_artist() {
        let conn = seeded_library();

        assert_eq!(
            query_albums(&conn).unwrap(),
            vec![
                AlbumSummary {
                    album: "Compilation".into(),
                    album_artist: "Various Artists".into(),
                    representative_path: "/music/mix-a.flac".into(),
                    track_count: 2,
                    year: None,
                    total_duration_ms: 0,
                    max_added_at: 0,
                    total_play_count: 0,
                },
                AlbumSummary {
                    album: "First".into(),
                    album_artist: "Other Artist".into(),
                    representative_path: "/music/other.flac".into(),
                    track_count: 1,
                    year: None,
                    total_duration_ms: 0,
                    max_added_at: 0,
                    total_play_count: 0,
                },
                AlbumSummary {
                    album: "First".into(),
                    album_artist: "Solo".into(),
                    representative_path: "/music/first-a.flac".into(),
                    track_count: 2,
                    year: None,
                    total_duration_ms: 0,
                    max_added_at: 0,
                    total_play_count: 0,
                },
            ]
        );
    }

    #[test]
    fn albums_query_is_read_only_and_excludes_blank_or_missing_rows() {
        let conn = seeded_library();
        let changes_before = conn.total_changes();

        let albums = query_albums(&conn).unwrap();

        assert_eq!(conn.total_changes(), changes_before);
        assert!(albums.iter().all(|album| !album.album.is_empty()));
        assert!(albums.iter().all(|album| album.album != "Lost"));
    }

    #[test]
    fn album_source_count_window_and_ids_select_the_exact_album_artist_group() {
        let mut conn = seeded_library();
        let source = ViewSource::Album {
            album: "FIRST".into(),
            album_artist: "solo".into(),
        };

        assert_eq!(query_track_count(&conn, &source, "", &[]).unwrap(), 2);
        assert_eq!(
            query_track_window(&mut conn, &source, "title", "desc", "", 0, 20, &[])
                .unwrap()
                .into_iter()
                .map(|track| track.title)
                .collect::<Vec<_>>(),
            ["B", "A"]
        );
        assert_eq!(
            query_track_ids(&conn, &source, "title", "asc", "A", &[]).unwrap(),
            [1]
        );
    }

    #[test]
    fn artists_group_case_insensitively_and_report_track_and_album_counts() {
        let conn = seeded_library();

        assert_eq!(
            query_artists(&conn).unwrap(),
            vec![
                ArtistSummary {
                    artist: "Guest A".into(),
                    track_count: 1,
                    album_count: 1,
                },
                ArtistSummary {
                    artist: "Guest B".into(),
                    track_count: 1,
                    album_count: 1,
                },
                ArtistSummary {
                    artist: "Nobody".into(),
                    track_count: 1,
                    album_count: 0,
                },
                ArtistSummary {
                    artist: "Other Artist".into(),
                    track_count: 1,
                    album_count: 1,
                },
                ArtistSummary {
                    artist: "Solo".into(),
                    track_count: 2,
                    album_count: 1,
                },
            ]
        );
    }

    #[test]
    fn artists_query_is_read_only_and_excludes_blank_or_missing_rows() {
        let conn = seeded_library();
        conn.execute(
            "INSERT INTO tracks (path,title,artist,album,added_at,missing) \
             VALUES ('/music/no-artist.flac','No Artist',' ','First',0,0)",
            [],
        )
        .unwrap();
        let changes_before = conn.total_changes();

        let artists = query_artists(&conn).unwrap();

        assert_eq!(conn.total_changes(), changes_before);
        assert!(artists.iter().all(|artist| !artist.artist.is_empty()));
        assert_eq!(
            artists
                .iter()
                .find(|artist| artist.artist == "Solo")
                .unwrap()
                .track_count,
            2
        );
    }

    #[test]
    fn artist_source_count_window_and_ids_select_the_exact_artist_group() {
        let mut conn = seeded_library();
        let source = ViewSource::Artist(" SOLO ".into());

        assert_eq!(query_track_count(&conn, &source, "", &[]).unwrap(), 2);
        assert_eq!(
            query_track_window(&mut conn, &source, "title", "desc", "", 0, 20, &[])
                .unwrap()
                .into_iter()
                .map(|track| track.title)
                .collect::<Vec<_>>(),
            ["B", "A"]
        );
        assert_eq!(
            query_track_ids(&conn, &source, "title", "asc", "A", &[]).unwrap(),
            [1]
        );
    }

    #[test]
    fn albums_include_year_duration_added_and_play_count_aggregates() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks
               (id,path,title,artist,album,album_artist,year,duration_ms,added_at,play_count,missing) VALUES
             (1,'/a.flac','A','Solo','Album','',2020,180000,1000,5,0),
             (2,'/b.flac','B','Solo','Album','',2020,240000,2000,3,0),
             (3,'/c.flac','C','Solo','Album','',0,120000,500,0,0);",
        )
        .unwrap();

        let albums = super::query_albums(&conn).unwrap();
        assert_eq!(albums.len(), 1);
        let album = &albums[0];
        assert_eq!(album.year, Some(2020));
        assert_eq!(album.total_duration_ms, 540000);
        assert_eq!(album.max_added_at, 2000);
        assert_eq!(album.total_play_count, 8);
    }
}
