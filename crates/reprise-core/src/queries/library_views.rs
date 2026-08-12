//! Read-only projections and detail queries for the visual library views.

use crate::db::Db;
use crate::models::Track;
use rusqlite::types::Value;
use rusqlite::Connection;

use super::clauses::{
    filter_clause, like_pattern, order_clause, row_to_id, row_to_track, track_projection, PRESENT,
};
use super::queue::QUEUE_LIMIT;
use super::{browse::browse_clause, BrowseFilter, TrackWindow, WindowRange, MAX_WINDOW_LIMIT};

pub(crate) const EFFECTIVE_ALBUM_ARTIST: &str =
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
    pub total_plays: i64,
    pub last_played_at: i64,
    pub representative_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumWindow {
    pub total: i64,
    pub rows: Vec<AlbumSummary>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistWindow {
    pub total: i64,
    pub rows: Vec<ArtistSummary>,
    pub has_more: bool,
}

fn album_summary_filter_clause(has_filter: bool, param_index: u8) -> String {
    if has_filter {
        format!(
            " AND (TRIM(album) LIKE ?{param_index} ESCAPE '\\' \
             OR {EFFECTIVE_ALBUM_ARTIST} LIKE ?{param_index} ESCAPE '\\')"
        )
    } else {
        String::new()
    }
}

fn artist_summary_filter_clause(has_filter: bool, param_index: u8) -> String {
    if has_filter {
        format!(" AND {EFFECTIVE_ALBUM_ARTIST} LIKE ?{param_index} ESCAPE '\\'")
    } else {
        String::new()
    }
}

/// What counts as an album by one artist: a present track, a non-blank album
/// title, and an exact case-insensitive match on the effective album artist.
/// Shared so desktop and Android cannot drift apart on the definition.
fn artist_albums_selection(param_index: u8) -> String {
    format!(
        "{PRESENT} AND TRIM(album) <> '' \
         AND {EFFECTIVE_ALBUM_ARTIST} = ?{param_index} COLLATE NOCASE"
    )
}

/// Returns one row per case-insensitive `(album, effective album artist)`
/// pair. Blank albums and missing tracks are excluded; the lowest track id
/// supplies stable display spelling and the representative cover path. A
/// non-blank filter matches the album title or effective album artist.
pub fn query_albums(
    db: &Db,
    filter: &str,
    window: WindowRange,
) -> Result<AlbumWindow, rusqlite::Error> {
    let total = query_album_count(db, filter)?;
    let conn = db.conn();
    let limit = window.limit.clamp(0, MAX_WINDOW_LIMIT);
    let has_filter = !filter.trim().is_empty();
    let filter_sql = album_summary_filter_clause(has_filter, 3);
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
           WHERE {PRESENT} AND TRIM(album) <> ''{filter_sql} \
           GROUP BY album_key, artist_key \
         ) \
         SELECT TRIM(tracks.album), {EFFECTIVE_ALBUM_ARTIST}, tracks.path, \
                grouped.track_count, grouped.year, \
                COALESCE(grouped.total_duration_ms, 0), \
                COALESCE(grouped.max_added_at, 0), \
                COALESCE(grouped.total_play_count, 0) \
         FROM grouped JOIN tracks ON tracks.id = grouped.representative_id \
         ORDER BY TRIM(tracks.album) COLLATE NOCASE ASC, \
                  {EFFECTIVE_ALBUM_ARTIST} COLLATE NOCASE ASC \
         LIMIT ?1 OFFSET ?2"
    );
    let mut statement = conn.prepare(&sql)?;
    let mut params = vec![Value::Integer(limit), Value::Integer(window.offset)];
    if has_filter {
        params.push(Value::Text(like_pattern(filter.trim())));
    }
    let rows = statement.query_map(rusqlite::params_from_iter(params), |row| {
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
    let rows = rows.collect::<Result<Vec<_>, _>>()?;
    Ok(AlbumWindow {
        total,
        has_more: super::surface_browse::has_more(total, window, rows.len()),
        rows,
    })
}

/// Returns a bounded album summary window for one exact effective album
/// artist, newest release year first and alphabetical within a year.
pub fn query_artist_albums(
    db: &Db,
    artist: &str,
    window: WindowRange,
) -> Result<AlbumWindow, rusqlite::Error> {
    let total = query_artist_album_count(db, artist)?;
    let conn = db.conn();
    let limit = window.limit.clamp(0, MAX_WINDOW_LIMIT);
    let selection = artist_albums_selection(3);
    let sql = format!(
        "WITH grouped AS ( \
           SELECT LOWER(TRIM(album)) AS album_key, \
                  MIN(id) AS representative_id, \
                  COUNT(*) AS track_count, \
                  MIN(CASE WHEN year > 0 THEN year END) AS year, \
                  SUM(duration_ms) AS total_duration_ms, \
                  MAX(added_at) AS max_added_at, \
                  SUM(play_count) AS total_play_count \
           FROM tracks \
           WHERE {selection} \
           GROUP BY album_key \
         ) \
         SELECT TRIM(tracks.album), {EFFECTIVE_ALBUM_ARTIST}, tracks.path, \
                grouped.track_count, grouped.year, \
                COALESCE(grouped.total_duration_ms, 0), \
                COALESCE(grouped.max_added_at, 0), \
                COALESCE(grouped.total_play_count, 0) \
         FROM grouped JOIN tracks ON tracks.id = grouped.representative_id \
         ORDER BY grouped.year DESC, TRIM(tracks.album) COLLATE NOCASE ASC \
         LIMIT ?1 OFFSET ?2"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(
        rusqlite::params![limit, window.offset, artist.trim()],
        |row| {
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
        },
    )?;
    let rows = rows.collect::<Result<Vec<_>, _>>()?;
    Ok(AlbumWindow {
        total,
        has_more: super::surface_browse::has_more(total, window, rows.len()),
        rows,
    })
}

/// Counts the exact album groups returned by [`query_artist_albums`] without
/// materializing their summaries.
pub fn query_artist_album_count(db: &Db, artist: &str) -> Result<i64, rusqlite::Error> {
    let selection = artist_albums_selection(1);
    db.conn().query_row(
        &format!(
            "SELECT COUNT(*) FROM ( \
               SELECT 1 FROM tracks WHERE {selection} \
               GROUP BY LOWER(TRIM(album)) \
             )"
        ),
        rusqlite::params![artist.trim()],
        |row| row.get(0),
    )
}

/// Returns one counted, bounded window of an artist's present tracks whose
/// album tag is blank after trimming.
pub fn query_artist_untagged_tracks(
    db: &Db,
    artist: &str,
    window: WindowRange,
) -> Result<TrackWindow, rusqlite::Error> {
    let total = query_artist_untagged_track_count(db, artist)?;
    let limit = window.limit.clamp(0, MAX_WINDOW_LIMIT);
    let projection = track_projection("", true);
    let sql = format!(
        "SELECT {projection} FROM tracks WHERE {PRESENT} \
         AND {EFFECTIVE_ALBUM_ARTIST} = ?3 COLLATE NOCASE \
         AND TRIM(album) = '' \
         ORDER BY title COLLATE NOCASE ASC, path COLLATE NOCASE ASC, id ASC \
         LIMIT ?1 OFFSET ?2"
    );
    let mut statement = db.conn().prepare(&sql)?;
    let rows = statement
        .query_map(
            rusqlite::params![limit, window.offset, artist.trim()],
            row_to_track,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(TrackWindow {
        total,
        has_more: super::surface_browse::has_more(total, window, rows.len()),
        rows,
    })
}

/// Counts the exact rows returned by [`query_artist_untagged_tracks`] without
/// materializing their track projections.
pub fn query_artist_untagged_track_count(db: &Db, artist: &str) -> Result<i64, rusqlite::Error> {
    db.conn().query_row(
        &format!(
            "SELECT count(*) FROM tracks WHERE {PRESENT} \
             AND {EFFECTIVE_ALBUM_ARTIST} = ?1 COLLATE NOCASE \
             AND TRIM(album) = ''"
        ),
        rusqlite::params![artist.trim()],
        |row| row.get(0),
    )
}

/// One row per case-insensitive effective album artist. Compilation and
/// featured tracks collapse under their album artist rather than exploding
/// the list. Blank artists and missing tracks are excluded; a non-blank filter
/// matches the effective album artist.
pub fn query_artists(
    db: &Db,
    filter: &str,
    window: WindowRange,
) -> Result<ArtistWindow, rusqlite::Error> {
    let total = query_artist_count(db, filter)?;
    let conn = db.conn();
    let limit = window.limit.clamp(0, MAX_WINDOW_LIMIT);
    let has_filter = !filter.trim().is_empty();
    let filter_sql = artist_summary_filter_clause(has_filter, 3);
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
           WHERE {PRESENT} AND TRIM({EFFECTIVE_ALBUM_ARTIST}) <> ''{filter_sql} \
           GROUP BY artist_key \
         ) \
         SELECT {EFFECTIVE_ALBUM_ARTIST}, grouped.track_count, grouped.album_count, \
                grouped.total_plays, grouped.last_played_at, tracks.path \
         FROM grouped JOIN tracks ON tracks.id = grouped.representative_id \
         ORDER BY {EFFECTIVE_ALBUM_ARTIST} COLLATE NOCASE ASC \
         LIMIT ?1 OFFSET ?2"
    );
    let mut statement = conn.prepare(&sql)?;
    let mut params = vec![Value::Integer(limit), Value::Integer(window.offset)];
    if has_filter {
        params.push(Value::Text(like_pattern(filter.trim())));
    }
    let rows = statement.query_map(rusqlite::params_from_iter(params), |row| {
        Ok(ArtistSummary {
            artist: row.get(0)?,
            track_count: row.get(1)?,
            album_count: row.get(2)?,
            total_plays: row.get(3)?,
            last_played_at: row.get(4)?,
            representative_path: row.get(5)?,
        })
    })?;
    let rows = rows.collect::<Result<Vec<_>, _>>()?;
    Ok(ArtistWindow {
        total,
        has_more: super::surface_browse::has_more(total, window, rows.len()),
        rows,
    })
}

/// Counts the distinct `(album, effective album artist)` groups — the exact
/// total reported by [`query_albums`] without materializing every
/// [`AlbumSummary`]. Same presence, blank-album and text filters and same
/// case-insensitive grouping keys, so the two always agree.
pub fn query_album_count(db: &Db, filter: &str) -> Result<i64, rusqlite::Error> {
    let conn = db.conn();
    let has_filter = !filter.trim().is_empty();
    let filter_sql = album_summary_filter_clause(has_filter, 1);
    let params = has_filter
        .then(|| Value::Text(like_pattern(filter.trim())))
        .into_iter();
    conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM ( \
               SELECT 1 FROM tracks \
               WHERE {PRESENT} AND TRIM(album) <> ''{filter_sql} \
               GROUP BY LOWER(TRIM(album)), LOWER({EFFECTIVE_ALBUM_ARTIST}) \
             )"
        ),
        rusqlite::params_from_iter(params),
        |row| row.get(0),
    )
}

/// Counts the distinct effective album artists — the exact total reported by
/// [`query_artists`] without materializing every [`ArtistSummary`]. Same
/// presence, blank-artist and text filters and case-insensitive key as
/// `query_artists`, so the two always agree.
pub fn query_artist_count(db: &Db, filter: &str) -> Result<i64, rusqlite::Error> {
    let conn = db.conn();
    let has_filter = !filter.trim().is_empty();
    let filter_sql = artist_summary_filter_clause(has_filter, 1);
    let params = has_filter
        .then(|| Value::Text(like_pattern(filter.trim())))
        .into_iter();
    conn.query_row(
        &format!(
            "SELECT COUNT(DISTINCT LOWER({EFFECTIVE_ALBUM_ARTIST})) FROM tracks \
             WHERE {PRESENT} AND TRIM({EFFECTIVE_ALBUM_ARTIST}) <> ''{filter_sql}"
        ),
        rusqlite::params_from_iter(params),
        |row| row.get(0),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn query_album_track_window(
    conn: &Connection,
    album: &str,
    album_artist: &str,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    browse: &BrowseFilter,
    offset: i64,
    limit: i64,
    project_ai: bool,
) -> Result<Vec<Track>, rusqlite::Error> {
    let limit = limit.clamp(0, MAX_WINDOW_LIMIT);
    let has_filter = !filter.trim().is_empty();
    let order = order_clause(sort_field, sort_dir);
    let filter_sql = filter_clause(has_filter, 5);
    let browse_first_param = if has_filter { 6 } else { 5 };
    let (browse_sql, browse_values) = browse_clause(browse, browse_first_param);
    let projection = track_projection("", project_ai);
    let sql = format!(
        "SELECT {projection} \
         FROM tracks WHERE {PRESENT} \
         AND TRIM(album) = ?3 COLLATE NOCASE \
         AND {EFFECTIVE_ALBUM_ARTIST} = ?4 COLLATE NOCASE{filter_sql}{browse_sql} \
         ORDER BY {order} LIMIT ?1 OFFSET ?2"
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
    params.extend(browse_values.into_iter().map(Value::Text));
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(rusqlite::params_from_iter(params), row_to_track)?;
    rows.collect()
}

pub(super) fn query_album_track_count(
    conn: &Connection,
    album: &str,
    album_artist: &str,
    filter: &str,
    browse: &BrowseFilter,
) -> Result<i64, rusqlite::Error> {
    let has_filter = !filter.trim().is_empty();
    let filter_sql = filter_clause(has_filter, 3);
    let browse_first_param = if has_filter { 4 } else { 3 };
    let (browse_sql, browse_values) = browse_clause(browse, browse_first_param);
    let sql = format!(
        "SELECT count(*) FROM tracks WHERE {PRESENT} \
         AND TRIM(album) = ?1 COLLATE NOCASE \
         AND {EFFECTIVE_ALBUM_ARTIST} = ?2 COLLATE NOCASE{filter_sql}{browse_sql}"
    );
    let mut params = vec![
        Value::Text(album.trim().to_string()),
        Value::Text(album_artist.trim().to_string()),
    ];
    if has_filter {
        params.push(Value::Text(like_pattern(filter.trim())));
    }
    params.extend(browse_values.into_iter().map(Value::Text));
    conn.query_row(&sql, rusqlite::params_from_iter(params), |row| row.get(0))
}

pub fn query_album_track_ids(
    db: &Db,
    album: &str,
    album_artist: &str,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
) -> Result<Vec<i64>, rusqlite::Error> {
    let conn = db.conn();
    query_album_track_ids_browsed(
        conn,
        album,
        album_artist,
        sort_field,
        sort_dir,
        filter,
        &BrowseFilter::default(),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn query_album_track_ids_browsed(
    conn: &Connection,
    album: &str,
    album_artist: &str,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    browse: &BrowseFilter,
) -> Result<Vec<i64>, rusqlite::Error> {
    let has_filter = !filter.trim().is_empty();
    let order = order_clause(sort_field, sort_dir);
    let filter_sql = filter_clause(has_filter, 3);
    let browse_first_param = if has_filter { 4 } else { 3 };
    let (browse_sql, browse_values) = browse_clause(browse, browse_first_param);
    let sql = format!(
        "SELECT id FROM tracks WHERE {PRESENT} \
         AND TRIM(album) = ?1 COLLATE NOCASE \
         AND {EFFECTIVE_ALBUM_ARTIST} = ?2 COLLATE NOCASE{filter_sql}{browse_sql} \
         ORDER BY {order} LIMIT {QUEUE_LIMIT}"
    );
    let mut params = vec![
        Value::Text(album.trim().to_string()),
        Value::Text(album_artist.trim().to_string()),
    ];
    if has_filter {
        params.push(Value::Text(like_pattern(filter.trim())));
    }
    params.extend(browse_values.into_iter().map(Value::Text));
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(rusqlite::params_from_iter(params), row_to_id)?;
    rows.collect()
}

/// Returns every present track in one album's canonical container-play
/// order. This is intentionally independent of the Album detail view's
/// current sort and filter: those control presentation, not PLAY-1a's queue
/// snapshot. Legacy rows without a disc number behave as disc 1; unknown
/// track numbers sort after numbered tracks, then path/id make ties stable.
///
/// Rows with a blank album are not an album, exactly as in [`query_albums`] and
/// [`query_artist_detail_albums`]: an empty argument would otherwise collect
/// every untagged track in the library under one name. That matters here beyond
/// presentation, because this list is what the Android context menu offers to
/// delete from the device.
pub fn query_album_canonical_track_ids(
    db: &Db,
    album: &str,
    album_artist: &str,
) -> Result<Vec<i64>, rusqlite::Error> {
    let conn = db.conn();
    let sql = format!(
        "SELECT id FROM tracks WHERE {PRESENT} AND TRIM(album) <> '' \
         AND TRIM(album) = ?1 COLLATE NOCASE \
         AND {EFFECTIVE_ALBUM_ARTIST} = ?2 COLLATE NOCASE \
         ORDER BY COALESCE(disc_no, 1) ASC, \
                  CASE WHEN track_no IS NULL THEN 1 ELSE 0 END ASC, \
                  track_no ASC, path COLLATE NOCASE ASC, id ASC \
         LIMIT {QUEUE_LIMIT}"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(
        rusqlite::params![album.trim(), album_artist.trim()],
        row_to_id,
    )?;
    rows.collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistAlbum {
    pub album: String,
    pub year: i64,
    pub track_count: i64,
    pub representative_path: String,
}

/// Albums by one effective album artist, newest release year first.
pub fn query_artist_detail_albums(
    db: &Db,
    artist: &str,
) -> Result<Vec<ArtistAlbum>, rusqlite::Error> {
    let conn = db.conn();
    let selection = artist_albums_selection(1);
    let sql = format!(
        "WITH grouped AS ( \
           SELECT LOWER(TRIM(album)) AS album_key, MIN(id) AS representative_id, \
                  COUNT(*) AS track_count, COALESCE(MAX(year), 0) AS year \
           FROM tracks \
           WHERE {selection} \
           GROUP BY album_key \
         ) \
         SELECT TRIM(tracks.album), grouped.year, grouped.track_count, tracks.path \
         FROM grouped JOIN tracks ON tracks.id = grouped.representative_id \
         ORDER BY grouped.year DESC, TRIM(tracks.album) COLLATE NOCASE ASC"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(rusqlite::params![artist.trim()], |row| {
        Ok(ArtistAlbum {
            album: row.get(0)?,
            year: row.get(1)?,
            track_count: row.get(2)?,
            representative_path: row.get(3)?,
        })
    })?;
    rows.collect()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn query_artist_track_window(
    conn: &Connection,
    artist: &str,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    browse: &BrowseFilter,
    offset: i64,
    limit: i64,
    project_ai: bool,
) -> Result<Vec<Track>, rusqlite::Error> {
    let limit = limit.clamp(0, MAX_WINDOW_LIMIT);
    let has_filter = !filter.trim().is_empty();
    let order = order_clause(sort_field, sort_dir);
    let filter_sql = filter_clause(has_filter, 4);
    let browse_first_param = if has_filter { 5 } else { 4 };
    let (browse_sql, browse_values) = browse_clause(browse, browse_first_param);
    let projection = track_projection("", project_ai);
    let sql = format!(
        "SELECT {projection} \
         FROM tracks WHERE {PRESENT} \
         AND {EFFECTIVE_ALBUM_ARTIST} = ?3 COLLATE NOCASE{filter_sql}{browse_sql} \
         ORDER BY {order} LIMIT ?1 OFFSET ?2"
    );
    let mut params = vec![
        Value::Integer(limit),
        Value::Integer(offset),
        Value::Text(artist.trim().to_string()),
    ];
    if has_filter {
        params.push(Value::Text(like_pattern(filter.trim())));
    }
    params.extend(browse_values.into_iter().map(Value::Text));
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(rusqlite::params_from_iter(params), row_to_track)?;
    rows.collect()
}

pub(super) fn query_artist_track_count(
    conn: &Connection,
    artist: &str,
    filter: &str,
    browse: &BrowseFilter,
) -> Result<i64, rusqlite::Error> {
    let has_filter = !filter.trim().is_empty();
    let filter_sql = filter_clause(has_filter, 2);
    let browse_first_param = if has_filter { 3 } else { 2 };
    let (browse_sql, browse_values) = browse_clause(browse, browse_first_param);
    let sql = format!(
        "SELECT count(*) FROM tracks WHERE {PRESENT} \
         AND {EFFECTIVE_ALBUM_ARTIST} = ?1 COLLATE NOCASE{filter_sql}{browse_sql}"
    );
    let mut params = vec![Value::Text(artist.trim().to_string())];
    if has_filter {
        params.push(Value::Text(like_pattern(filter.trim())));
    }
    params.extend(browse_values.into_iter().map(Value::Text));
    conn.query_row(&sql, rusqlite::params_from_iter(params), |row| row.get(0))
}

pub(super) fn query_artist_track_ids(
    conn: &Connection,
    artist: &str,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    browse: &BrowseFilter,
) -> Result<Vec<i64>, rusqlite::Error> {
    let has_filter = !filter.trim().is_empty();
    let order = order_clause(sort_field, sort_dir);
    let filter_sql = filter_clause(has_filter, 2);
    let browse_first_param = if has_filter { 3 } else { 2 };
    let (browse_sql, browse_values) = browse_clause(browse, browse_first_param);
    let sql = format!(
        "SELECT id FROM tracks WHERE {PRESENT} \
         AND {EFFECTIVE_ALBUM_ARTIST} = ?1 COLLATE NOCASE{filter_sql}{browse_sql} \
         ORDER BY {order} LIMIT {QUEUE_LIMIT}"
    );
    let mut params = vec![Value::Text(artist.trim().to_string())];
    if has_filter {
        params.push(Value::Text(like_pattern(filter.trim())));
    }
    params.extend(browse_values.into_iter().map(Value::Text));
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(rusqlite::params_from_iter(params), row_to_id)?;
    rows.collect()
}

#[cfg(test)]
#[path = "library_views_tests.rs"]
mod tests;
