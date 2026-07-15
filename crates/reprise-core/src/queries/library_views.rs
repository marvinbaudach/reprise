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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistSummary {
    pub artist: String,
    pub track_count: i64,
    pub album_count: i64,
    pub total_plays: i64,
    pub last_played_at: i64,
    pub representative_path: String,
}

/// Returns one row per case-insensitive `(album, effective album artist)`
/// pair. Blank albums and missing tracks are excluded; the lowest track id
/// supplies stable display spelling and the representative cover path.
pub fn query_albums(conn: &Connection) -> Result<Vec<AlbumSummary>, rusqlite::Error> {
    let sql = format!(
        "WITH grouped AS ( \
           SELECT LOWER(TRIM(album)) AS album_key, \
                  LOWER({EFFECTIVE_ALBUM_ARTIST}) AS artist_key, \
                  MIN(id) AS representative_id, COUNT(*) AS track_count \
           FROM tracks \
           WHERE missing = 0 AND TRIM(album) <> '' \
           GROUP BY album_key, artist_key \
         ) \
         SELECT TRIM(tracks.album), {EFFECTIVE_ALBUM_ARTIST}, tracks.path, grouped.track_count \
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
        })
    })?;
    rows.collect()
}

/// One row per case-insensitive effective album artist. Compilation and
/// featured tracks collapse under their album artist rather than exploding
/// the list. Blank artists and missing tracks are excluded.
pub fn query_artists(conn: &Connection) -> Result<Vec<ArtistSummary>, rusqlite::Error> {
    let sql = format!(
        "WITH grouped AS ( \
           SELECT LOWER({EFFECTIVE_ALBUM_ARTIST}) AS artist_key, \
                  MIN(id) AS representative_id, \
                  COUNT(*) AS track_count, \
                  COUNT(DISTINCT CASE WHEN TRIM(album) <> '' \
                        THEN LOWER(TRIM(album)) END) AS album_count, \
                  COALESCE(SUM(play_count), 0) AS total_plays, \
                  COALESCE(MAX(last_played_at), 0) AS last_played_at \
           FROM tracks \
           WHERE missing = 0 AND TRIM({EFFECTIVE_ALBUM_ARTIST}) <> '' \
           GROUP BY artist_key \
         ) \
         SELECT {EFFECTIVE_ALBUM_ARTIST}, grouped.track_count, grouped.album_count, \
                grouped.total_plays, grouped.last_played_at, tracks.path \
         FROM grouped JOIN tracks ON tracks.id = grouped.representative_id \
         ORDER BY {EFFECTIVE_ALBUM_ARTIST} COLLATE NOCASE ASC"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map([], |row| {
        Ok(ArtistSummary {
            artist: row.get(0)?,
            track_count: row.get(1)?,
            album_count: row.get(2)?,
            total_plays: row.get(3)?,
            last_played_at: row.get(4)?,
            representative_path: row.get(5)?,
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
                },
                AlbumSummary {
                    album: "First".into(),
                    album_artist: "Other Artist".into(),
                    representative_path: "/music/other.flac".into(),
                    track_count: 1,
                },
                AlbumSummary {
                    album: "First".into(),
                    album_artist: "Solo".into(),
                    representative_path: "/music/first-a.flac".into(),
                    track_count: 2,
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
    fn artists_group_by_effective_album_artist_with_aggregates() {
        let conn = seeded_library();
        let artists = query_artists(&conn).unwrap();
        let names: Vec<&str> = artists.iter().map(|a| a.artist.as_str()).collect();
        assert_eq!(
            names,
            vec!["Nobody", "Other Artist", "Solo", "Various Artists"]
        );
        let solo = artists.iter().find(|a| a.artist == "Solo").unwrap();
        assert_eq!(solo.track_count, 2);
        assert_eq!(solo.album_count, 1);
        assert_eq!(solo.total_plays, 0);
        let va = artists
            .iter()
            .find(|a| a.artist == "Various Artists")
            .unwrap();
        assert_eq!(va.track_count, 2);
        assert_eq!(va.album_count, 1);
        assert_eq!(va.representative_path, "/music/mix-a.flac");
    }

    #[test]
    fn artists_sum_play_count_and_max_last_played_at_across_group_rows() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks
               (id,path,title,artist,album,album_artist,added_at,missing,play_count,last_played_at) VALUES
             (1,'/music/a.flac','Track A','Solo','Album','',0,0,3,100),
             (2,'/music/b.flac','Track B','Solo','Album','',0,0,5,200);",
        )
        .unwrap();

        let artists = query_artists(&conn).unwrap();
        let solo = artists.iter().find(|a| a.artist == "Solo").unwrap();

        // A per-row read (e.g. only the representative row's play_count) or a
        // MIN/first-value bug would yield 3 or 5, not the summed/maxed value.
        assert_eq!(solo.total_plays, 8);
        assert_eq!(solo.last_played_at, 200);
        assert_eq!(solo.representative_path, "/music/a.flac");
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
}
