//! `ViewSource::Smart(id)` test coverage — split out of the former
//! single-file `queries.rs`'s inline test module (Refactoring &
//! Extensibility Task 1) purely to keep every file under the project's
//! 800-line rule; see `tests.rs`'s doc comment for the full split map. A
//! pure move, no assertion change.

use super::*;

fn seeded_conn_with_tracks(count: i64) -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    for i in 1..=count {
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, '', 0)",
            rusqlite::params![i, format!("/x/{i}.flac"), format!("Track {i}")],
        )
        .unwrap();
    }
    conn
}

fn insert_smart_playlist(
    conn: &Connection,
    rules_json: &str,
    sort_field: &str,
    sort_dir: &str,
    limit_count: Option<i64>,
) -> i64 {
    conn.execute(
        "INSERT INTO smart_playlists (name, rules_json, sort_field, sort_dir, limit_count) \
         VALUES ('S', ?1, ?2, ?3, ?4)",
        rusqlite::params![rules_json, sort_field, sort_dir, limit_count],
    )
    .unwrap();
    conn.last_insert_rowid()
}

#[test]
fn smart_window_applies_rules_and_own_sort() {
    let conn = seeded_conn_with_tracks(5);
    conn.execute("UPDATE tracks SET rating = 4 WHERE id IN (2, 4)", [])
        .unwrap();
    let smart_id = insert_smart_playlist(
        &conn,
        r#"[{"field":"rating","op":">=","value":4}]"#,
        "title",
        "asc",
        None,
    );

    let mut conn = conn;
    let rows = query_track_window(
        &mut conn,
        &ViewSource::Smart(smart_id),
        "ignored",
        "ignored",
        "",
        0,
        10,
        &[],
    )
    .unwrap();
    let ids: Vec<i64> = rows.iter().map(|t| t.id).collect();
    assert_eq!(ids, vec![2, 4]);
    assert_eq!(
        query_track_count(&conn, &ViewSource::Smart(smart_id), "", &[]).unwrap(),
        2
    );
}

#[test]
fn smart_window_keeps_membership_but_honors_the_requested_column_sort() {
    let conn = seeded_conn_with_tracks(3);
    conn.execute(
        "UPDATE tracks SET artist = CASE id \
         WHEN 1 THEN 'Gamma' WHEN 2 THEN 'Beta' ELSE 'Alpha' END",
        [],
    )
    .unwrap();
    let smart_id = insert_smart_playlist(&conn, "[]", "title", "asc", Some(2));

    let mut conn = conn;
    let rows = query_track_window(
        &mut conn,
        &ViewSource::Smart(smart_id),
        "artist",
        "asc",
        "",
        0,
        10,
        &[],
    )
    .unwrap();
    let ids: Vec<i64> = rows.iter().map(|track| track.id).collect();

    assert_eq!(
        ids,
        vec![2, 1],
        "the smart definition chooses members 1 and 2, then the clicked Artist column sorts them"
    );
    assert_eq!(
        query_track_ids(
            &conn,
            &ViewSource::Smart(smart_id),
            "artist",
            "asc",
            "",
            &[],
        )
        .unwrap(),
        vec![2, 1],
        "playback snapshots must follow the same visible smart-playlist order"
    );
}

#[test]
fn smart_window_applies_live_search_filter_too() {
    let conn = seeded_conn_with_tracks(5);
    conn.execute("UPDATE tracks SET rating = 4", []).unwrap();
    let smart_id = insert_smart_playlist(
        &conn,
        r#"[{"field":"rating","op":">=","value":4}]"#,
        "title",
        "asc",
        None,
    );

    let mut conn = conn;
    let rows = query_track_window(
        &mut conn,
        &ViewSource::Smart(smart_id),
        "ignored",
        "ignored",
        "Track 3",
        0,
        10,
        &[],
    )
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "Track 3");
}

#[test]
fn smart_window_offset_within_limit_returns_the_edge_case_slice() {
    // Regression for the exact edge case the task calls out: a smart
    // playlist limited to 50 rows, windowed at offset 40/limit 20, must
    // return exactly 10 rows (positions 40..49), never rows beyond the
    // smart playlist's own limit.
    let conn = seeded_conn_with_tracks(100);
    let smart_id = insert_smart_playlist(&conn, "[]", "title", "asc", Some(50));

    let mut conn = conn;
    let rows = query_track_window(
        &mut conn,
        &ViewSource::Smart(smart_id),
        "ignored",
        "ignored",
        "",
        40,
        20,
        &[],
    )
    .unwrap();
    assert_eq!(rows.len(), 10);

    // Minor fix (review round 1): the previous comment here claimed the
    // tail of the 50-row set was "Track 41".."Track 49" — that's the
    // *numeric* tail, not the lexicographic (`COLLATE NOCASE`) one this
    // query actually produces (e.g. "Track 5" sorts before "Track 50").
    // Rather than hand-picking (and mis-describing) the expected slice,
    // re-derive it directly from the same lexicographic string sort Rust
    // gives `Vec<String>::sort`, matching `title COLLATE NOCASE` for
    // this all-ASCII fixture.
    let mut all_titles: Vec<String> = (1..=100).map(|i| format!("Track {i}")).collect();
    all_titles.sort();
    let expected = &all_titles[40..50];
    let got: Vec<String> = rows.iter().map(|t| t.title.clone()).collect();
    assert_eq!(got, expected);
}

#[test]
fn smart_count_is_capped_by_limit_count() {
    let conn = seeded_conn_with_tracks(100);
    let smart_id = insert_smart_playlist(&conn, "[]", "title", "asc", Some(50));

    assert_eq!(
        query_track_count(&conn, &ViewSource::Smart(smart_id), "", &[]).unwrap(),
        50
    );
}

#[test]
fn smart_ids_are_capped_by_limit_count() {
    let conn = seeded_conn_with_tracks(100);
    let smart_id = insert_smart_playlist(&conn, "[]", "title", "asc", Some(50));

    let ids = query_track_ids(
        &conn,
        &ViewSource::Smart(smart_id),
        "ignored",
        "ignored",
        "",
        &[],
    )
    .unwrap();
    assert_eq!(ids.len(), 50);
}

#[test]
fn smart_window_falls_back_to_title_on_tampered_sort_field() {
    // Simulates a hand-edited (DB-tampered) smart_playlists row whose
    // sort_field isn't a whitelisted value — `order_expr_and_dir` must
    // fall back to title order rather than erroring or (worse)
    // interpolating the value into SQL.
    let conn = seeded_conn_with_tracks(3);
    let smart_id = insert_smart_playlist(&conn, "[]", "sneaky; DROP TABLE tracks--", "asc", None);

    let mut conn = conn;
    let rows = query_track_window(
        &mut conn,
        &ViewSource::Smart(smart_id),
        "ignored",
        "ignored",
        "",
        0,
        10,
        &[],
    )
    .unwrap();
    let titles: Vec<&str> = rows.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, vec!["Track 1", "Track 2", "Track 3"]);
}

#[test]
fn smart_source_not_found_degrades_to_empty() {
    let conn = seeded_conn_with_tracks(3);
    let mut conn = conn;
    assert!(
        query_track_window(&mut conn, &ViewSource::Smart(999), "x", "x", "", 0, 10, &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        query_track_count(&conn, &ViewSource::Smart(999), "", &[]).unwrap(),
        0
    );
    assert!(
        query_track_ids(&conn, &ViewSource::Smart(999), "x", "x", "", &[])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn fil_8_recently_added_includes_every_track_from_the_last_seven_days_without_a_50_cap() {
    let conn = crate::db::open_migrated(None).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    for id in 1..=60 {
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at)
             VALUES (?1, ?2, ?3, '', ?4)",
            rusqlite::params![
                id,
                format!("/recent/{id}.flac"),
                format!("Recent {id}"),
                now - id
            ],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, added_at)
         VALUES (61, '/old.flac', 'Old', '', ?1)",
        [now - 8 * 24 * 60 * 60],
    )
    .unwrap();

    assert_eq!(
        query_track_count(&conn, &ViewSource::RecentlyAdded, "", &[]).unwrap(),
        60
    );
    let ids = query_track_ids(
        &conn,
        &ViewSource::RecentlyAdded,
        "added_at",
        "desc",
        "",
        &[],
    )
    .unwrap();
    assert_eq!(ids.len(), 60);
    assert_eq!(ids[0], 1);
    assert!(!ids.contains(&61));

    let smart_id = conn
        .query_row(
            "SELECT id FROM smart_playlists WHERE role = ?1",
            [crate::library::playlists::RECENTLY_ADDED_ROLE],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    assert_eq!(
        query_track_count(&conn, &ViewSource::Smart(smart_id), "", &[]).unwrap(),
        60,
        "legacy sessions and non-GTK consumers must resolve the built-in smart id identically"
    );
}
