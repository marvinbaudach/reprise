use crate::models::Track;
use rusqlite::Connection;
use std::sync::Mutex;
use tauri::State;

pub struct AppState {
    pub db: Mutex<Connection>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStats {
    pub track_count: i64,
    pub total_duration_ms: i64,
    // Reserved for a future filtered/active-view count (e.g. status bar showing
    // "N of M tracks" while a filter is applied). Always None until a filter
    // parameter is threaded through get_library_stats.
    pub filtered_count: Option<i64>,
}

const SORT_WHITELIST: [(&str, &str); 6] = [
    ("title", "title COLLATE NOCASE"),
    ("artist", "artist COLLATE NOCASE, album COLLATE NOCASE, track_no"),
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

#[tauri::command]
pub fn get_track_window(
    state: State<AppState>,
    sort_field: String,
    sort_dir: String,
    filter: String,
    offset: i64,
    limit: i64,
) -> Result<Vec<Track>, String> {
    let mut conn = state.db.lock().map_err(|e| {
        tracing::error!("failed to lock database connection: {e}");
        e.to_string()
    })?;
    query_track_window(
        &mut conn,
        &sort_field,
        &sort_dir,
        &filter,
        offset,
        limit.min(500),
    )
    .map_err(|e| {
        tracing::error!(sort_field = %sort_field, sort_dir = %sort_dir, "get_track_window failed: {e}");
        e.to_string()
    })
}

#[tauri::command]
pub fn scan_music_folder(
    state: State<AppState>,
    root: String,
) -> Result<crate::library::scanner::ScanReport, String> {
    tracing::info!(folder = %root, "starting library scan");
    let mut conn = state.db.lock().map_err(|e| {
        tracing::error!("failed to lock database connection: {e}");
        e.to_string()
    })?;
    let report = crate::library::scanner::scan_folder(&mut conn, std::path::Path::new(&root))
        .map_err(|e| {
            tracing::error!(folder = %root, "scan_music_folder failed: {e}");
            e.to_string()
        })?;
    tracing::info!(?report, "library scan finished");
    Ok(report)
}

#[tauri::command]
pub fn get_library_stats(state: State<AppState>) -> Result<LibraryStats, String> {
    let conn = state.db.lock().map_err(|e| {
        tracing::error!("failed to lock database connection: {e}");
        e.to_string()
    })?;
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
    .map_err(|e| {
        tracing::error!("get_library_stats failed: {e}");
        e.to_string()
    })
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
}
