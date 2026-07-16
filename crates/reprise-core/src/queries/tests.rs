//! Core `queries` test suite: the query-builder/whitelist/LIKE-escaping
//! unit tests plus the Library/Missing source's window/count/ids/stats
//! coverage. Split out of the former single-file `queries.rs`'s inline
//! `#[cfg(test)] mod tests` (Refactoring & Extensibility Task 1) — a pure
//! move, no behavior/assertion change. The Playlist/Smart/Queue/maintenance
//! sections of that same original test module live in the sibling
//! `tests_playlist.rs`/`tests_smart.rs`/`tests_queue.rs`/
//! `tests_maintenance.rs` files (declared from `queries/mod.rs`), split out
//! purely to keep every file under the project's 800-line rule.

use super::clauses::like_pattern;
use super::*;

fn seeded_titled_conn() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    for (t, a) in [("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")] {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, added_at) VALUES (?1, ?2, ?3, 0)",
            rusqlite::params![format!("/x/{t}.flac"), t, a],
        )
        .unwrap();
    }
    conn
}

#[test]
fn query_builder_whitelists_and_sorts() {
    let q = build_track_query("artist", "asc", false);
    assert!(q.contains(
        "ORDER BY artist COLLATE NOCASE, year, album COLLATE NOCASE, track_no ASC"
    ));
    assert!(q.contains("WHERE missing = 0"));
    assert!(!q.contains("?3")); // no filter placeholder without a filter
}

#[test]
fn query_builder_rejects_unknown_column_with_title_fallback() {
    let q = build_track_query("path; DROP TABLE tracks", "desc", true);
    assert!(q.contains("ORDER BY title COLLATE NOCASE DESC"));
    assert!(q.contains(
        "(title LIKE ?3 ESCAPE '\\' OR artist LIKE ?3 ESCAPE '\\' \
         OR album LIKE ?3 ESCAPE '\\' OR genre LIKE ?3 ESCAPE '\\')"
    ));
}

/// Pins the exact escaped pattern `like_pattern` produces (per this
/// project's SQLite skill: assert the exact escaped param, not just
/// `contains`, so a regression that escapes the wrong character or the
/// wrong order still fails this test).
#[test]
fn like_pattern_escapes_backslash_first_then_percent_and_underscore() {
    assert_eq!(like_pattern("50%_off\\sale"), "%50\\%\\_off\\\\sale%");
}

/// Regression for the LIKE-escaping finding: a literal `%` typed into
/// the search box must match only rows that actually contain a literal
/// `%`, not act as a live wildcard matching everything.
#[test]
fn search_filter_treats_a_literal_percent_as_a_literal_not_a_wildcard() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    for (t, a) in [("A%B", "X"), ("AZB", "Y"), ("Other", "Z")] {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, added_at) VALUES (?1, ?2, ?3, 0)",
            rusqlite::params![format!("/x/{t}.flac"), t, a],
        )
        .unwrap();
    }

    let mut conn = conn;
    let rows = query_track_window(
        &mut conn,
        &ViewSource::Library,
        "title",
        "asc",
        "%",
        0,
        10,
        &[],
    )
    .unwrap();
    assert_eq!(
        rows.len(),
        1,
        "a literal '%' must match only the literal-% row"
    );
    assert_eq!(rows[0].title, "A%B");

    assert_eq!(
        query_track_count(&conn, &ViewSource::Library, "%", &[]).unwrap(),
        1
    );
}

#[test]
fn window_returns_filtered_sorted_tracks() {
    let mut conn = seeded_titled_conn();
    let rows = query_track_window(
        &mut conn,
        &ViewSource::Library,
        "title",
        "asc",
        "",
        0,
        10,
        &[],
    )
    .unwrap();
    assert_eq!(rows[0].title, "Alpha");
    let rows = query_track_window(
        &mut conn,
        &ViewSource::Library,
        "title",
        "asc",
        "zu",
        0,
        10,
        &[],
    )
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "Zulu");
}

#[test]
fn count_is_zero_for_empty_db() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    assert_eq!(
        query_track_count(&conn, &ViewSource::Library, "", &[]).unwrap(),
        0
    );
}

#[test]
fn count_matches_inserted_rows() {
    let conn = seeded_titled_conn();
    assert_eq!(
        query_track_count(&conn, &ViewSource::Library, "", &[]).unwrap(),
        3
    );
}

#[test]
fn count_applies_filter() {
    let conn = seeded_titled_conn();
    assert_eq!(
        query_track_count(&conn, &ViewSource::Library, "zu", &[]).unwrap(),
        1
    );
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
    assert_eq!(
        query_track_count(&conn, &ViewSource::Library, "", &[]).unwrap(),
        0
    );
}

#[test]
fn track_ids_follow_whitelist_sort_order() {
    let conn = seeded_titled_conn();
    let ids = query_track_ids(&conn, &ViewSource::Library, "title", "asc", "", &[]).unwrap();
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
    let conn = seeded_titled_conn();
    let ids = query_track_ids(&conn, &ViewSource::Library, "title", "asc", "zu", &[]).unwrap();
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
        query_track_ids(&conn, &ViewSource::Library, "title", "asc", "", &[]).unwrap(),
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
        "INSERT INTO tracks (path, title, artist, album, year, duration_ms, added_at) \
         VALUES ('/x/a.flac', 'A Title', 'An Artist', 'An Album', 2026, 123456, 0)",
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
    assert_eq!(summary.year, Some(2026));
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

    assert_eq!(
        query_track_count(&conn, &ViewSource::Library, "", &[]).unwrap(),
        1
    );
    assert_eq!(
        query_track_ids(&conn, &ViewSource::Library, "title", "asc", "", &[]).unwrap(),
        vec![id]
    );

    mark_track_missing(&conn, id).unwrap();

    assert_eq!(
        query_track_count(&conn, &ViewSource::Library, "", &[]).unwrap(),
        0
    );
    assert_eq!(
        query_track_ids(&conn, &ViewSource::Library, "title", "asc", "", &[]).unwrap(),
        Vec::<i64>::new()
    );
}

#[test]
fn library_stats_without_filter_has_none_filtered_count_and_full_totals() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    for (t, a) in [("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")] {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, duration_ms, added_at) \
             VALUES (?1, ?2, ?3, 1000, 0)",
            rusqlite::params![format!("/x/{t}.flac"), t, a],
        )
        .unwrap();
    }

    let stats = query_library_stats(&conn, "").unwrap();
    assert_eq!(stats.track_count, 3);
    assert_eq!(stats.total_duration_ms, 3000);
    assert_eq!(stats.filtered_count, None);
}

#[test]
fn library_stats_with_filter_matches_query_track_count_and_keeps_totals_unfiltered() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    for (t, a) in [("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")] {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, duration_ms, added_at) \
             VALUES (?1, ?2, ?3, 1000, 0)",
            rusqlite::params![format!("/x/{t}.flac"), t, a],
        )
        .unwrap();
    }

    let stats = query_library_stats(&conn, "zu").unwrap();
    // Totals stay unfiltered even though a filter is active.
    assert_eq!(stats.track_count, 3);
    assert_eq!(stats.total_duration_ms, 3000);
    assert_eq!(
        stats.filtered_count,
        Some(query_track_count(&conn, &ViewSource::Library, "zu", &[]).unwrap())
    );
    assert_eq!(stats.filtered_count, Some(1));
}

#[test]
fn library_stats_missing_rows_excluded_from_both_counts() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, duration_ms, added_at, missing) \
         VALUES ('/x/a.flac', 'A', '', 1000, 0, 1)",
        [],
    )
    .unwrap();

    let unfiltered = query_library_stats(&conn, "").unwrap();
    assert_eq!(unfiltered.track_count, 0);
    assert_eq!(unfiltered.total_duration_ms, 0);
    assert_eq!(unfiltered.filtered_count, None);

    let filtered = query_library_stats(&conn, "A").unwrap();
    assert_eq!(filtered.track_count, 0);
    assert_eq!(filtered.filtered_count, Some(0));
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
    let rows = query_track_window(
        &mut conn,
        &ViewSource::Library,
        "title",
        "asc",
        "",
        0,
        -1,
        &[],
    )
    .unwrap();
    assert_eq!(rows.len(), 0);

    // A limit far above MAX_WINDOW_LIMIT is clamped down to the cap,
    // which still comfortably covers this small fixture set, so all
    // rows are returned rather than the query becoming unbounded.
    let rows = query_track_window(
        &mut conn,
        &ViewSource::Library,
        "title",
        "asc",
        "",
        0,
        10_000,
        &[],
    )
    .unwrap();
    assert_eq!(rows.len(), 3);
}

// -- Missing source -------------------------------------------------

fn seeded_conn_with_missing() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    for (t, a, missing) in [("Zulu", "AAA", 1), ("Alpha", "BBB", 0), ("Mid", "CCC", 1)] {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, added_at, missing) \
             VALUES (?1, ?2, ?3, 0, ?4)",
            rusqlite::params![format!("/x/{t}.flac"), t, a, missing],
        )
        .unwrap();
    }
    conn
}

fn seeded_browse_conn() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    for (id, title, artist, album, genre) in [
        (1, "Live One", "A", "Stage", "Rock"),
        (2, "Studio One", "A", "Room", "Rock"),
        (3, "Live Two", "B", "Stage", "Rock"),
        (4, "Live Three", "A", "Stage", "Jazz"),
    ] {
        conn.execute(
            "INSERT INTO tracks (id,path,title,artist,album,genre,added_at,duration_ms) \
             VALUES (?1,?2,?3,?4,?5,?6,0,1000)",
            rusqlite::params![id, format!("/x/{id}.flac"), title, artist, album, genre],
        )
        .unwrap();
    }
    conn
}

#[test]
fn browse_and_text_filter_match_across_window_count_ids_and_stats() {
    let mut conn = seeded_browse_conn();
    let browse = BrowseFilter {
        genre: Some("Rock".into()),
        artist: Some("A".into()),
        album: None,
    };
    let rows = query_track_window_browsed(
        &mut conn,
        &ViewSource::Library,
        "title",
        "asc",
        "live",
        &browse,
        0,
        10,
        &[],
    )
    .unwrap();
    assert_eq!(
        rows.iter().map(|track| track.id).collect::<Vec<_>>(),
        vec![1]
    );
    assert_eq!(
        query_track_count_browsed(&conn, &ViewSource::Library, "live", &browse, &[]).unwrap(),
        1
    );
    assert_eq!(
        query_track_ids_browsed(
            &conn,
            &ViewSource::Library,
            "title",
            "asc",
            "live",
            &browse,
            &[],
        )
        .unwrap(),
        vec![1]
    );
    let stats = query_library_stats_browsed(&conn, "live", &browse).unwrap();
    assert_eq!(stats.track_count, 4);
    assert_eq!(stats.filtered_count, Some(1));
}

#[test]
fn non_library_sources_ignore_browse_filter() {
    let mut conn = seeded_browse_conn();
    let playlist = crate::library::playlists::create(&conn, "All").unwrap();
    crate::library::playlists::add_tracks(&mut conn, playlist, &[1, 2, 3, 4]).unwrap();
    let browse = BrowseFilter {
        genre: Some("Does not exist".into()),
        ..BrowseFilter::default()
    };
    assert_eq!(
        query_track_count_browsed(&conn, &ViewSource::Playlist(playlist), "", &browse, &[],)
            .unwrap(),
        4
    );
}

#[test]
fn missing_window_and_count_only_include_missing_rows() {
    let mut conn = seeded_conn_with_missing();
    let rows = query_track_window(
        &mut conn,
        &ViewSource::Missing,
        "title",
        "asc",
        "",
        0,
        10,
        &[],
    )
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].title, "Mid");
    assert_eq!(rows[1].title, "Zulu");

    assert_eq!(
        query_track_count(&conn, &ViewSource::Missing, "", &[]).unwrap(),
        2
    );
}

#[test]
fn missing_ids_are_sorted_like_library() {
    let conn = seeded_conn_with_missing();
    let ids = query_track_ids(&conn, &ViewSource::Missing, "title", "asc", "", &[]).unwrap();
    let by_title: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT id FROM tracks WHERE missing = 1 ORDER BY title COLLATE NOCASE ASC")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(ids, by_title);
}
