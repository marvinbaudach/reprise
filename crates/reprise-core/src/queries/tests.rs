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
use std::path::Path;

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
    assert!(q.contains("ORDER BY artist COLLATE NOCASE, year, album COLLATE NOCASE, track_no ASC"));
    assert!(q.contains(&format!("WHERE {}", super::clauses::PRESENT)));
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
        "INSERT INTO tracks (path, title, artist, added_at, missing_since) \
         VALUES ('/x/a.flac', 'A', '', 0, 1)",
        [],
    )
    .unwrap();
    assert_eq!(
        query_track_count(&conn, &ViewSource::Library, "", &[]).unwrap(),
        0
    );
}

/// The Task 1.2 predicate flip, pinned directly: a row with `missing_since`
/// set must disappear from the library window/count even though the legacy
/// `missing` column (still populated by pre-this-task writers, per schema
/// v10's doc comment) says `0` — proving the window/count queries read
/// `missing_since`, not `missing`, for presence.
#[test]
fn missing_since_excludes_a_row_even_when_the_legacy_missing_column_says_present() {
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, added_at, missing_since) \
         VALUES ('/x/a.flac', 'A', '', 0, 1)",
        [],
    )
    .unwrap();
    assert_eq!(
        query_track_count(&conn, &ViewSource::Library, "", &[]).unwrap(),
        0,
        "a set missing_since must exclude the row from the library"
    );
    assert!(query_track_window(
        &mut conn,
        &ViewSource::Library,
        "title",
        "asc",
        "",
        0,
        10,
        &[],
    )
    .unwrap()
    .is_empty());
}

/// `removed_at` (the tombstone column a later task starts writing for the
/// 10-second undo window) must exclude a row from the *present* predicate
/// even while `missing_since` is `NULL` — see `clauses::PRESENT`'s doc
/// comment for why a removed-but-not-yet-purged row must never resurface in
/// the library window/count.
#[test]
fn removed_at_excludes_a_row_from_the_library_even_while_present() {
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, added_at, removed_at) \
         VALUES ('/x/a.flac', 'A', '', 0, 1)",
        [],
    )
    .unwrap();
    assert_eq!(
        query_track_count(&conn, &ViewSource::Library, "", &[]).unwrap(),
        0,
        "a tombstoned row must not count as present"
    );
    assert!(query_track_window(
        &mut conn,
        &ViewSource::Library,
        "title",
        "asc",
        "",
        0,
        10,
        &[],
    )
    .unwrap()
    .is_empty());
}

/// `MissingReason::parse` must never fail to load a row: an unrecognized
/// `missing_reason` string (a value this app version never wrote — a future
/// schema, or a hand-edited row) falls back to `Unknown` rather than
/// panicking or erroring, matching the enum's own doc comment.
#[test]
fn missing_reason_parse_falls_back_to_unknown_for_an_unrecognized_value() {
    assert_eq!(
        crate::models::MissingReason::parse("garbage"),
        crate::models::MissingReason::Unknown
    );
    assert_eq!(
        crate::models::MissingReason::parse("unmounted"),
        crate::models::MissingReason::Unmounted
    );
    assert_eq!(
        crate::models::MissingReason::parse("deleted"),
        crate::models::MissingReason::Deleted
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
        "INSERT INTO tracks (path, title, artist, added_at, missing_since) \
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

    assert!(mark_track_missing_if_current(&conn, id, Path::new("/x/a.flac")).unwrap());

    let (missing_since, missing_reason): (Option<i64>, Option<String>) = conn
        .query_row(
            "SELECT missing_since, missing_reason FROM tracks WHERE id = ?1",
            rusqlite::params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(missing_since.is_some());
    // The row above never went through the scanner, so it has no recorded
    // `device` (NULL) — `classify_missing(None, _)` has nothing to compare
    // against and honestly reports `Unknown` rather than guessing (see
    // `MissingReason`'s own doc comment). A device-bearing row's real
    // Deleted/Unmounted verdict is `mounts::classify_missing`'s own test
    // suite's job, not this one's.
    assert_eq!(missing_reason.as_deref(), Some("unknown"));
}

/// The classifier's `Deleted` branch, driven end-to-end through
/// `mark_track_missing_if_current` itself rather than `mounts::classify_missing`
/// directly: a row whose recorded `device` matches its directory's real,
/// current `st_dev` (i.e. the mount is genuinely still there) and whose file
/// has actually been deleted must land on `missing_reason == "deleted"`, not
/// the `unknown` the sibling test above pins for a `NULL`-device row. The
/// device is read from the real temp directory via `lstat`, matching
/// `mounts.rs`'s own test convention — no root or real mounting needed.
#[test]
fn mark_track_missing_classifies_deleted_when_device_matches() {
    use std::os::unix::fs::MetadataExt;

    let dir = tempfile::tempdir().unwrap();
    let real_dev = std::fs::symlink_metadata(dir.path()).unwrap().dev() as i64;
    let path = dir.path().join("gone.flac");

    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, added_at, device) VALUES (?1, 'A', '', 0, ?2)",
        rusqlite::params![path.to_string_lossy(), real_dev],
    )
    .unwrap();
    let id: i64 = conn
        .query_row("SELECT id FROM tracks", [], |r| r.get(0))
        .unwrap();
    // The row is deliberately never written to disk, so `classify_missing`
    // sees a matching device and an absent file — the honest "deleted, not
    // unmounted" verdict.

    assert!(mark_track_missing_if_current(&conn, id, &path).unwrap());

    let (missing_since, missing_reason): (Option<i64>, Option<String>) = conn
        .query_row(
            "SELECT missing_since, missing_reason FROM tracks WHERE id = ?1",
            rusqlite::params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(missing_since.is_some());
    assert_eq!(missing_reason.as_deref(), Some("deleted"));
}

/// The classifier's `Unmounted` branch, same shape as the `Deleted` test
/// above but with a fabricated non-matching device (`real_dev + 99_999`,
/// mirroring `mounts.rs`'s own test convention for a guaranteed
/// non-collision) — pure arithmetic, no loopback device or real unmount
/// needed to prove the branch.
#[test]
fn mark_track_missing_classifies_unmounted_when_device_differs() {
    use std::os::unix::fs::MetadataExt;

    let dir = tempfile::tempdir().unwrap();
    let real_dev = std::fs::symlink_metadata(dir.path()).unwrap().dev() as i64;
    let path = dir.path().join("gone.flac");

    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, added_at, device) VALUES (?1, 'A', '', 0, ?2)",
        rusqlite::params![path.to_string_lossy(), real_dev + 99_999],
    )
    .unwrap();
    let id: i64 = conn
        .query_row("SELECT id FROM tracks", [], |r| r.get(0))
        .unwrap();

    assert!(mark_track_missing_if_current(&conn, id, &path).unwrap());

    let missing_reason: Option<String> = conn
        .query_row(
            "SELECT missing_reason FROM tracks WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(missing_reason.as_deref(), Some("unmounted"));
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

    assert!(mark_track_missing_if_current(&conn, id, Path::new("/x/a.flac")).unwrap());

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
        "INSERT INTO tracks (path, title, artist, duration_ms, added_at, missing_since) \
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
    for (t, a, missing_since) in [
        ("Zulu", "AAA", Some(1)),
        ("Alpha", "BBB", None),
        ("Mid", "CCC", Some(1)),
    ] {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, added_at, missing_since) \
             VALUES (?1, ?2, ?3, 0, ?4)",
            rusqlite::params![format!("/x/{t}.flac"), t, a, missing_since],
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
        ..BrowseFilter::default()
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
fn play_1_scoped_playback_ids_are_the_exact_visible_refined_order() {
    let mut conn = seeded_browse_conn();
    let source = ViewSource::Artist("A".into());
    let browse = BrowseFilter {
        genre: Some("Rock".into()),
        ..BrowseFilter::default()
    };

    let rows =
        query_track_window_browsed(&mut conn, &source, "title", "desc", "", &browse, 0, 10, &[])
            .unwrap();
    let visible = rows.iter().map(|track| track.id).collect::<Vec<_>>();
    let playback =
        query_track_ids_browsed(&conn, &source, "title", "desc", "", &browse, &[]).unwrap();

    assert_eq!(visible, vec![2, 1]);
    assert_eq!(playback, visible);
    assert_eq!(
        query_track_count_browsed(&conn, &source, "", &browse, &[]).unwrap(),
        2
    );
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
            .prepare(&format!(
                "SELECT id FROM tracks WHERE {} ORDER BY title COLLATE NOCASE ASC",
                super::clauses::MISSING
            ))
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(ids, by_title);
}

#[test]
fn fil_7_count_browsed_ai_excludes_ai_tracks_via_count_star() {
    // The cheap COUNT(*) variant that replaces the QUEUE_LIMIT-capped
    // ids.len() fallback: with exclude_ai it counts only non-AI Library
    // tracks; without it, every present track.
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute_batch(
        "INSERT INTO tracks (id, path, title, artist, added_at, file_mtime, file_size) \
           VALUES (1, '/a.flac', 'Original', 'A', 1, 1, 1);
         INSERT INTO tracks (id, path, title, artist, added_at, file_mtime, file_size) \
           VALUES (2, '/b.flac', 'Instrumental', 'A', 1, 1, 1);
         INSERT INTO track_provenance (track_id, kind, ai, created_at) \
           VALUES (2, 'vocals-removed', 1, 0);",
    )
    .unwrap();
    let browse = BrowseFilter::default();

    let all =
        query_track_count_browsed_ai(&conn, &ViewSource::Library, "", &browse, &[], false).unwrap();
    assert_eq!(all, 2, "without the filter both present tracks count");
    let non_ai =
        query_track_count_browsed_ai(&conn, &ViewSource::Library, "", &browse, &[], true).unwrap();
    assert_eq!(non_ai, 1, "the AI instrumental is excluded from the count");

    // The COUNT(*) agrees with the AI-filtered id list it replaces.
    let ids = query_track_ids_browsed_ai(
        &conn,
        &ViewSource::Library,
        "title",
        "asc",
        "",
        &browse,
        &[],
        true,
    )
    .unwrap();
    assert_eq!(
        non_ai as usize,
        ids.len(),
        "count matches the AI-filtered id list length"
    );
}
