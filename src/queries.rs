use crate::models::Track;
use rusqlite::{Connection, OptionalExtension};

/// Global constraint: window queries never return more rows than this in one
/// page, regardless of what the caller requests. SQLite treats a negative
/// `LIMIT` as "unlimited", so this also protects against a bad UI-side page
/// size from turning into a full-table scan. Limits capped.
const MAX_WINDOW_LIMIT: i64 = 500;

/// Hard cap on how many track ids `query_track_ids` will ever return in one
/// call. This is a *separate* constant from `MAX_WINDOW_LIMIT` on purpose:
/// `query_track_ids` powers the queue (Stage 2 Task 4 — "play this whole
/// view"), which legitimately wants every matching id, not one `ColumnView`
/// page. `MAX_WINDOW_LIMIT` (500) is sized for a UI page; a queue is
/// reasonably built from a much larger library, but still must not turn a
/// huge/unfiltered library into an unbounded query. 10,000 tracks is a very
/// large personal library and a small `Vec<i64>` (~80 KB) even at the cap.
/// Callers should compare the returned `Vec`'s length against this constant
/// via `is_queue_capped` and log a warning when it's capped, since the `Vec`
/// alone can't distinguish "capped" from "library has exactly this many
/// tracks".
pub const QUEUE_LIMIT: i64 = 10_000;

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

/// Shared LIKE-filter clause on `(title, artist, album, genre)`, parameterized
/// by the positional index of the bound `?N` placeholder: `build_track_query`
/// binds the filter as the third parameter (after `LIMIT`/`OFFSET`), while
/// `query_track_count` has no limit/offset and binds it as the first. Both
/// build their WHERE clause through this one function so the filtered
/// columns and LIKE semantics can never drift apart between the count and
/// the rows it describes (DRY).
fn filter_clause(has_filter: bool, param_index: u8) -> String {
    if has_filter {
        format!(
            " AND (title LIKE ?{i} OR artist LIKE ?{i} OR album LIKE ?{i} OR genre LIKE ?{i})",
            i = param_index
        )
    } else {
        String::new()
    }
}

/// Resolves `sort_field`/`sort_dir` to a whitelisted `ORDER BY` expression
/// and direction keyword. Shared by `build_track_query` and
/// `build_track_ids_query` so the two queries can never disagree about what
/// a given sort field/direction means. `sort_field` is only ever used as a
/// lookup key into `SORT_WHITELIST` — never interpolated into SQL directly —
/// so caller input cannot inject arbitrary SQL. Unknown sort fields silently
/// fall back to sorting by title.
fn order_expr_and_dir(sort_field: &str, sort_dir: &str) -> (&'static str, &'static str) {
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
    (order_expr, dir)
}

/// Builds the parameterized SELECT for a library window. `sort_field` is only
/// ever used to look up an entry in `SORT_WHITELIST` — it is never interpolated
/// into the SQL string directly, so caller input cannot inject arbitrary SQL.
/// Unknown sort fields silently fall back to sorting by title.
pub fn build_track_query(sort_field: &str, sort_dir: &str, has_filter: bool) -> String {
    let (order_expr, dir) = order_expr_and_dir(sort_field, sort_dir);
    let filter_clause = filter_clause(has_filter, 3);
    format!(
        "SELECT id, path, title, artist, album, album_artist, year, track_no, genre, \
         duration_ms, bitrate_kbps, rating, play_count, last_played_at, added_at, \
         file_mtime, missing \
         FROM tracks WHERE missing = 0{filter_clause} \
         ORDER BY {order_expr} {dir} LIMIT ?1 OFFSET ?2"
    )
}

/// Builds the parameterized `SELECT id` for the queue seam
/// (`query_track_ids`): every id matching `(sort_field, sort_dir, filter)`,
/// capped at `QUEUE_LIMIT` — a literal, not a bound parameter, since it's a
/// fixed Rust-side constant rather than caller input (nothing to inject).
/// Shares `order_expr_and_dir`/`filter_clause` with `build_track_query` so
/// the queue's ordering can never drift from the track list's.
pub fn build_track_ids_query(sort_field: &str, sort_dir: &str, has_filter: bool) -> String {
    let (order_expr, dir) = order_expr_and_dir(sort_field, sort_dir);
    let filter_clause = filter_clause(has_filter, 1);
    format!(
        "SELECT id FROM tracks WHERE missing = 0{filter_clause} \
         ORDER BY {order_expr} {dir} LIMIT {QUEUE_LIMIT}"
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

/// Counts non-missing tracks matching `filter`, using the identical LIKE
/// clause `build_track_query` applies (via `filter_clause`) so the count and
/// the windowed rows it describes can never disagree about which rows match.
pub fn query_track_count(conn: &Connection, filter: &str) -> Result<i64, rusqlite::Error> {
    let has_filter = !filter.trim().is_empty();
    let sql = format!(
        "SELECT count(*) FROM tracks WHERE missing = 0{}",
        filter_clause(has_filter, 1)
    );
    if has_filter {
        let like = format!("%{}%", filter.trim());
        conn.query_row(&sql, rusqlite::params![like], |r| r.get(0))
    } else {
        conn.query_row(&sql, [], |r| r.get(0))
    }
}

/// Returns every non-missing track id matching `(sort_field, sort_dir,
/// filter)`, in that sort order, capped at `QUEUE_LIMIT`. This is the queue
/// seam (Stage 2 Task 4): activating a row queues "the whole current view"
/// by resolving it to this id list rather than the `MAX_WINDOW_LIMIT`-capped
/// `query_track_window` (which is sized for one `ColumnView` page, not a
/// playback queue). The `Vec` alone can't tell the caller whether it was
/// truncated by the cap — compare its length with `is_queue_capped` and log
/// a warning if so.
pub fn query_track_ids(
    conn: &Connection,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
) -> Result<Vec<i64>, rusqlite::Error> {
    let has_filter = !filter.trim().is_empty();
    let sql = build_track_ids_query(sort_field, sort_dir, has_filter);
    let mut stmt = conn.prepare(&sql)?;
    let like = format!("%{}%", filter.trim());
    let rows = if has_filter {
        stmt.query_map(rusqlite::params![like], row_to_id)?
    } else {
        stmt.query_map([], row_to_id)?
    };
    rows.collect()
}

fn row_to_id(r: &rusqlite::Row) -> rusqlite::Result<i64> {
    r.get(0)
}

/// Whether a `query_track_ids` result of this length was (probably) capped
/// by `QUEUE_LIMIT`. Treats the exact-boundary case (`len == QUEUE_LIMIT`)
/// as capped: the alternative — a library with *exactly* `QUEUE_LIMIT`
/// matching tracks — is indistinguishable from a truncated one without a
/// second `COUNT(*)` query, and logging one harmless extra warning on that
/// rare exact-fit case is a better tradeoff than silently missing a real
/// truncation.
pub fn is_queue_capped(len: usize) -> bool {
    len as i64 >= QUEUE_LIMIT
}

/// The subset of a track's columns the player bar and queue playback path
/// need: the file to hand `Player::play`, the title/artist to show, and the
/// duration play-tracking's 50%-listened check requires
/// (`library::stats::should_count_play`). Deliberately narrower than the
/// full `Track` (no rating/play_count/etc. — the bar doesn't display those),
/// avoiding the cost of loading and holding the columns nothing here reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackSummary {
    pub path: String,
    pub title: String,
    pub artist: String,
    /// Stage 2 Task 6 (MPRIS): feeds `Metadata`'s `xesam:album`. Not used by
    /// the player bar (which only shows title/artist), so it went unused
    /// here until MPRIS needed it.
    pub album: String,
    pub duration_ms: i64,
}

/// Resolves one track id to its `TrackSummary` — the queue's per-track
/// playback step (`play_track_id` in `ui::player_controller`) calls this for
/// every auto-advance/next/previous, and Stage 2 Task 5's skip-on-missing-
/// file logic is documented to reuse it too. `Ok(None)` for an id with no
/// matching row (e.g. deleted between queueing and playback) — never an
/// error; the caller decides how to degrade (skip/stop), matching every
/// other fallible path in this module.
pub fn query_track_summary(
    conn: &Connection,
    id: i64,
) -> Result<Option<TrackSummary>, rusqlite::Error> {
    conn.query_row(
        "SELECT path, title, artist, album, duration_ms FROM tracks WHERE id = ?1",
        rusqlite::params![id],
        |r| {
            Ok(TrackSummary {
                path: r.get(0)?,
                title: r.get(1)?,
                artist: r.get(2)?,
                album: r.get(3)?,
                duration_ms: r.get(4)?,
            })
        },
    )
    .optional()
}

/// Marks track `track_id` as missing (Stage 2 Task 5: a physically deleted
/// file must never crash or dead-end the app — this is the DB-side half of
/// that guarantee). Every windowed/count/id query in this module already
/// filters `WHERE missing = 0` (see `build_track_query`/`build_track_ids_
/// query`/`query_track_count`), so the row disappears from the library view
/// and from a freshly-seeded queue on the very next reload, without deleting
/// the row itself — ratings/play history/etc. are preserved for when a
/// future "missing files" sidebar source (Stage 3) lets it resurface.
pub fn mark_track_missing(conn: &Connection, track_id: i64) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE tracks SET missing = 1 WHERE id = ?1",
        rusqlite::params![track_id],
    )?;
    Ok(())
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
    fn count_is_zero_for_empty_db() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        assert_eq!(query_track_count(&conn, "").unwrap(), 0);
    }

    #[test]
    fn count_matches_inserted_rows() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        for (t, a) in [("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")] {
            conn.execute(
                "INSERT INTO tracks (path, title, artist, added_at) VALUES (?1, ?2, ?3, 0)",
                rusqlite::params![format!("/x/{t}.flac"), t, a],
            )
            .unwrap();
        }
        assert_eq!(query_track_count(&conn, "").unwrap(), 3);
    }

    #[test]
    fn count_applies_filter() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        for (t, a) in [("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")] {
            conn.execute(
                "INSERT INTO tracks (path, title, artist, added_at) VALUES (?1, ?2, ?3, 0)",
                rusqlite::params![format!("/x/{t}.flac"), t, a],
            )
            .unwrap();
        }
        assert_eq!(query_track_count(&conn, "zu").unwrap(), 1);
    }

    #[test]
    fn count_excludes_missing_rows() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, artist, added_at, missing) \
             VALUES ('/x/a.flac', 'A', '', 0, 1)",
            [],
        )
        .unwrap();
        assert_eq!(query_track_count(&conn, "").unwrap(), 0);
    }

    #[test]
    fn track_ids_follow_whitelist_sort_order() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        for (t, a) in [("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")] {
            conn.execute(
                "INSERT INTO tracks (path, title, artist, added_at) VALUES (?1, ?2, ?3, 0)",
                rusqlite::params![format!("/x/{t}.flac"), t, a],
            )
            .unwrap();
        }
        let ids = query_track_ids(&conn, "title", "asc", "").unwrap();
        assert_eq!(ids.len(), 3);

        // "Alpha" < "Mid" < "Zulu" by title (COLLATE NOCASE) — assert the
        // exact id order directly against the same ORDER BY expression
        // `SORT_WHITELIST` uses for "title".
        let by_title: Vec<i64> = {
            let mut stmt = conn
                .prepare("SELECT id FROM tracks ORDER BY title COLLATE NOCASE ASC")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(ids, by_title);
    }

    #[test]
    fn track_ids_apply_filter() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        for (t, a) in [("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")] {
            conn.execute(
                "INSERT INTO tracks (path, title, artist, added_at) VALUES (?1, ?2, ?3, 0)",
                rusqlite::params![format!("/x/{t}.flac"), t, a],
            )
            .unwrap();
        }
        let ids = query_track_ids(&conn, "title", "asc", "zu").unwrap();
        assert_eq!(ids.len(), 1);

        let expected_id: i64 = conn
            .query_row("SELECT id FROM tracks WHERE title = 'Zulu'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(ids[0], expected_id);
    }

    #[test]
    fn track_ids_excludes_missing_rows() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, artist, added_at, missing) \
             VALUES ('/x/a.flac', 'A', '', 0, 1)",
            [],
        )
        .unwrap();
        assert_eq!(
            query_track_ids(&conn, "title", "asc", "").unwrap(),
            Vec::<i64>::new()
        );
    }

    #[test]
    fn track_ids_query_is_capped_at_queue_limit() {
        // Inserting QUEUE_LIMIT+1 rows just to prove the cap would make this
        // test slow and heavy for no extra confidence — the cap is a single
        // hardcoded `LIMIT` in the generated SQL, so asserting it's present
        // with the right value in `build_track_ids_query`'s output is the
        // pragmatic, fast way to pin the behavior. The boundary logic for
        // *detecting* a truncated result (`is_queue_capped`) is exercised
        // directly below instead of via a 10,001-row fixture.
        let sql = build_track_ids_query("title", "asc", false);
        assert!(sql.contains(&format!("LIMIT {QUEUE_LIMIT}")));
    }

    #[test]
    fn is_queue_capped_detects_the_boundary() {
        assert!(!is_queue_capped((QUEUE_LIMIT - 1) as usize));
        assert!(is_queue_capped(QUEUE_LIMIT as usize));
    }

    #[test]
    fn track_summary_found_returns_expected_fields() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, artist, album, duration_ms, added_at) \
             VALUES ('/x/a.flac', 'A Title', 'An Artist', 'An Album', 123456, 0)",
            [],
        )
        .unwrap();
        let id: i64 = conn
            .query_row("SELECT id FROM tracks", [], |r| r.get(0))
            .unwrap();

        let summary = query_track_summary(&conn, id).unwrap().unwrap();
        assert_eq!(summary.path, "/x/a.flac");
        assert_eq!(summary.title, "A Title");
        assert_eq!(summary.artist, "An Artist");
        assert_eq!(summary.album, "An Album");
        assert_eq!(summary.duration_ms, 123456);
    }

    #[test]
    fn track_summary_not_found_returns_none() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        assert!(query_track_summary(&conn, 999).unwrap().is_none());
    }

    #[test]
    fn mark_track_missing_sets_the_flag() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, artist, added_at) VALUES ('/x/a.flac', 'A', '', 0)",
            [],
        )
        .unwrap();
        let id: i64 = conn
            .query_row("SELECT id FROM tracks", [], |r| r.get(0))
            .unwrap();

        mark_track_missing(&conn, id).unwrap();

        let missing: i64 = conn
            .query_row(
                "SELECT missing FROM tracks WHERE id = ?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(missing, 1);
    }

    #[test]
    fn mark_track_missing_excludes_from_count_and_ids() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, artist, added_at) VALUES ('/x/a.flac', 'A', '', 0)",
            [],
        )
        .unwrap();
        let id: i64 = conn
            .query_row("SELECT id FROM tracks", [], |r| r.get(0))
            .unwrap();

        assert_eq!(query_track_count(&conn, "").unwrap(), 1);
        assert_eq!(
            query_track_ids(&conn, "title", "asc", "").unwrap(),
            vec![id]
        );

        mark_track_missing(&conn, id).unwrap();

        assert_eq!(query_track_count(&conn, "").unwrap(), 0);
        assert_eq!(
            query_track_ids(&conn, "title", "asc", "").unwrap(),
            Vec::<i64>::new()
        );
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
