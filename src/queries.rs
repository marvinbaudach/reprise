use crate::models::Track;
use rusqlite::Connection;

/// Global constraint: window queries never return more rows than this in one
/// page, regardless of what the caller requests. SQLite treats a negative
/// `LIMIT` as "unlimited", so this also protects against a bad UI-side page
/// size from turning into a full-table scan. Limits capped.
const MAX_WINDOW_LIMIT: i64 = 500;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStats {
    pub track_count: i64,
    pub total_duration_ms: i64,
    // Reserved for a future filtered/active-view count (e.g. status bar showing
    // "N of M tracks" while a filter is applied). Always None until a filter
    // parameter is threaded through query_library_stats.
    pub filtered_count: Option<i64>,
}

const SORT_WHITELIST: [(&str, &str); 6] = [
    ("title", "title COLLATE NOCASE"),
    (
        "artist",
        "artist COLLATE NOCASE, album COLLATE NOCASE, track_no",
    ),
    ("album", "album COLLATE NOCASE, track_no"),
    ("year", "year"),
    ("duration_ms", "duration_ms"),
    ("rating", "rating"),
];

/// Builds the parameterized SELECT for a library window. `sort_field` is only
/// ever used to look up an entry in `SORT_WHITELIST` — it is never interpolated
/// into the SQL string directly, so caller input cannot inject arbitrary SQL.
/// Unknown sort fields silently fall back to sorting by title.
pub fn build_track_query(sort_field: &str, sort_dir: &str, has_filter: bool) -> String {
    let order_expr = SORT_WHITELIST
        .iter()
        .find(|(k, _)| *k == sort_field)
        .map(|(_, v)| *v)
        .unwrap_or("title COLLATE NOCASE");
    let dir = if sort_dir.eq_ignore_ascii_case("desc") {
        "DESC"
    } else {
        "ASC"
    };
    let filter_clause = if has_filter {
        " AND (title LIKE ?3 OR artist LIKE ?3 OR album LIKE ?3 OR genre LIKE ?3)"
    } else {
        ""
    };
    format!(
        "SELECT id, path, title, artist, album, album_artist, year, track_no, genre, \
         duration_ms, bitrate_kbps, rating, play_count, last_played_at, added_at, \
         file_mtime, missing \
         FROM tracks WHERE missing = 0{filter_clause} \
         ORDER BY {order_expr} {dir} LIMIT ?1 OFFSET ?2"
    )
}

fn row_to_track(r: &rusqlite::Row) -> rusqlite::Result<Track> {
    Ok(Track {
        id: r.get(0)?,
        path: r.get(1)?,
        title: r.get(2)?,
        artist: r.get(3)?,
        album: r.get(4)?,
        album_artist: r.get(5)?,
        year: r.get(6)?,
        track_no: r.get(7)?,
        genre: r.get(8)?,
        duration_ms: r.get(9)?,
        bitrate_kbps: r.get(10)?,
        rating: r.get(11)?,
        play_count: r.get(12)?,
        last_played_at: r.get(13)?,
        added_at: r.get(14)?,
        file_mtime: r.get(15)?,
        missing: r.get::<_, i64>(16)? != 0,
    })
}

/// Runs the windowed track query. `filter` is always bound as a parameter
/// (`LIKE ?3`), never concatenated into the SQL text.
pub fn query_track_window(
    conn: &mut Connection,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<Track>, rusqlite::Error> {
    let limit = limit.clamp(0, MAX_WINDOW_LIMIT);
    let has_filter = !filter.trim().is_empty();
    let sql = build_track_query(sort_field, sort_dir, has_filter);
    let mut stmt = conn.prepare(&sql)?;
    let like = format!("%{}%", filter.trim());
    let rows = if has_filter {
        stmt.query_map(rusqlite::params![limit, offset, like], row_to_track)?
    } else {
        stmt.query_map(rusqlite::params![limit, offset], row_to_track)?
    };
    rows.collect()
}

/// Aggregates library-wide stats over all non-missing tracks. Powers the
/// status line (`ui::status_bar`).
pub fn query_library_stats(conn: &Connection) -> Result<LibraryStats, rusqlite::Error> {
    conn.query_row(
        "SELECT count(*), coalesce(sum(duration_ms),0) FROM tracks WHERE missing = 0",
        [],
        |r| {
            Ok(LibraryStats {
                track_count: r.get(0)?,
                total_duration_ms: r.get(1)?,
                filtered_count: None,
            })
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_builder_whitelists_and_sorts() {
        let q = build_track_query("artist", "asc", false);
        assert!(q.contains("ORDER BY artist COLLATE NOCASE, album COLLATE NOCASE, track_no ASC"));
        assert!(q.contains("WHERE missing = 0"));
        assert!(!q.contains("?3")); // no filter placeholder without a filter
    }

    #[test]
    fn query_builder_rejects_unknown_column_with_title_fallback() {
        let q = build_track_query("path; DROP TABLE tracks", "desc", true);
        assert!(q.contains("ORDER BY title COLLATE NOCASE DESC"));
        assert!(q.contains("(title LIKE ?3 OR artist LIKE ?3 OR album LIKE ?3 OR genre LIKE ?3)"));
    }

    #[test]
    fn window_returns_filtered_sorted_tracks() {
        let mut conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        for (t, a) in [("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")] {
            conn.execute(
                "INSERT INTO tracks (path, title, artist, added_at) VALUES (?1, ?2, ?3, 0)",
                rusqlite::params![format!("/x/{t}.flac"), t, a],
            )
            .unwrap();
        }
        let rows = query_track_window(&mut conn, "title", "asc", "", 0, 10).unwrap();
        assert_eq!(rows[0].title, "Alpha");
        let rows = query_track_window(&mut conn, "title", "asc", "zu", 0, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Zulu");
    }

    #[test]
    fn window_limit_is_clamped() {
        let mut conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        for t in ["Alpha", "Beta", "Gamma"] {
            conn.execute(
                "INSERT INTO tracks (path, title, artist, added_at) VALUES (?1, ?2, '', 0)",
                rusqlite::params![format!("/x/{t}.flac"), t],
            )
            .unwrap();
        }

        // SQLite treats a negative LIMIT as "unlimited"; clamped to 0, a
        // negative caller-supplied limit must return no rows.
        let rows = query_track_window(&mut conn, "title", "asc", "", 0, -1).unwrap();
        assert_eq!(rows.len(), 0);

        // A limit far above MAX_WINDOW_LIMIT is clamped down to the cap,
        // which still comfortably covers this small fixture set, so all
        // rows are returned rather than the query becoming unbounded.
        let rows = query_track_window(&mut conn, "title", "asc", "", 0, 10_000).unwrap();
        assert_eq!(rows.len(), 3);
    }
}
