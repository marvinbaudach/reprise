//! `playlists.rs`'s test suite, split into its own file purely to keep
//! `playlists.rs` under the project's 800-line rule — `playlists.rs`
//! declares this via `#[cfg(test)] #[path = "playlists_tests.rs"]
//! mod tests;`, so this file's contents are still the crate-private
//! `crate::library::playlists::tests` module, with the exact same tests,
//! unchanged, that used to live inline (a pure move, not a rewrite).

use super::*;

const GENRE_RULES: &str = r#"[{"field":"genre","op":"=","value":"Ambient"}]"#;

fn seeded_conn() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    // Insert test tracks.
    for id in 1..=5 {
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at, rating, play_count, last_played_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                format!("/x/track{}.flac", id),
                format!("Track {}", id),
                format!("Artist {}", id),
                1000 + id,
                (id % 5) as i64,           // rating 0-4
                id * 10,                    // play_count
                if id > 2 { Some(2000 + id) } else { None }, // last_played_at
            ],
        )
        .unwrap();
    }
    conn
}

#[test]
fn create_playlist_returns_new_id() {
    let conn = seeded_conn();
    let id = create(&conn, "My Playlist").unwrap();
    assert!(id > 0);

    let name: String = conn
        .query_row(
            "SELECT name FROM playlists WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(name, "My Playlist");
}

#[test]
fn create_smart_inserts_a_playlist_that_list_smart_returns() {
    let conn = seeded_conn();

    let id = create_smart(
        &conn,
        "Night Mix",
        GENRE_RULES,
        "play_count",
        "desc",
        Some(25),
    )
    .unwrap();

    let created = list_smart(&conn)
        .unwrap()
        .into_iter()
        .find(|playlist| playlist.id == id)
        .expect("created smart playlist is listed");
    assert_eq!(created.name, "Night Mix");
    assert_eq!(created.rules_json, GENRE_RULES);
    assert_eq!(created.sort_field, "play_count");
    assert_eq!(created.sort_dir, "desc");
    assert_eq!(created.limit_count, Some(25));
}

/// Pressing "Create Smart Mix" twice must not leave two identical playlists
/// behind for the user to clean up; a differing rule set is still a new one.
#[test]
fn create_smart_is_idempotent_for_an_identical_playlist() {
    let conn = seeded_conn();
    let before = list_smart(&conn).unwrap().len();

    let first = create_smart(&conn, "Genre Mix", GENRE_RULES, "play_count", "desc", None).unwrap();
    let again = create_smart(&conn, "Genre Mix", GENRE_RULES, "play_count", "desc", None).unwrap();
    assert_eq!(first, again);
    assert_eq!(list_smart(&conn).unwrap().len(), before + 1);

    let other = create_smart(
        &conn,
        "Genre Mix",
        r#"[{"field":"genre","op":"=","value":"Jazz"}]"#,
        "play_count",
        "desc",
        None,
    )
    .unwrap();
    assert_ne!(first, other);
    assert_eq!(list_smart(&conn).unwrap().len(), before + 2);
}

#[test]
fn create_smart_rejects_invalid_rules_json() {
    let conn = seeded_conn();
    let before = list_smart(&conn).unwrap().len();

    let result = create_smart(
        &conn,
        "Broken Mix",
        r#"[{"field":"not-a-field","op":"="}]"#,
        "title",
        "asc",
        None,
    );

    assert!(result.is_err());
    assert_eq!(list_smart(&conn).unwrap().len(), before);
}

#[test]
fn create_playlist_assigns_sequential_positions() {
    let conn = seeded_conn();
    let id1 = create(&conn, "Playlist 1").unwrap();
    let id2 = create(&conn, "Playlist 2").unwrap();
    let id3 = create(&conn, "Playlist 3").unwrap();

    let (pos1, pos2, pos3): (i64, i64, i64) = conn
        .query_row(
            "SELECT position FROM playlists WHERE id = ?1",
            params![id1],
            |r| r.get(0),
        )
        .and_then(|p1| {
            let p2: i64 = conn.query_row(
                "SELECT position FROM playlists WHERE id = ?1",
                params![id2],
                |r| r.get(0),
            )?;
            let p3: i64 = conn.query_row(
                "SELECT position FROM playlists WHERE id = ?1",
                params![id3],
                |r| r.get(0),
            )?;
            Ok((p1, p2, p3))
        })
        .unwrap();

    assert_eq!(pos1, 0);
    assert_eq!(pos2, 1);
    assert_eq!(pos3, 2);
}

#[test]
fn create_playlist_accepts_empty_name() {
    let conn = seeded_conn();
    let id = create(&conn, "").unwrap();
    let name: String = conn
        .query_row(
            "SELECT name FROM playlists WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(name, "");
}

#[test]
fn create_playlist_trims_whitespace_name() {
    let conn = seeded_conn();
    let id = create(&conn, "  My Playlist  ").unwrap();
    let name: String = conn
        .query_row(
            "SELECT name FROM playlists WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(name, "My Playlist");
}

#[test]
fn rename_playlist() {
    let conn = seeded_conn();
    let id = create(&conn, "Old Name").unwrap();
    // A real rename reports one row changed.
    assert_eq!(rename(&conn, id, "New Name").unwrap(), 1);

    let name: String = conn
        .query_row(
            "SELECT name FROM playlists WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(name, "New Name");

    // A non-existent id matches nothing and changes zero rows.
    assert_eq!(rename(&conn, 9_999, "Nope").unwrap(), 0);
}

#[test]
fn get_returns_the_summary_or_none() {
    let mut conn = seeded_conn();
    let id = create(&conn, "Mix").unwrap();
    add_tracks(&mut conn, id, &[1, 2]).unwrap();

    let summary = get(&conn, id).unwrap().expect("playlist exists");
    assert_eq!(summary.id, id);
    assert_eq!(summary.name, "Mix");
    assert_eq!(summary.track_count, 2);

    // A missing id is Ok(None), not an error.
    assert!(get(&conn, 9_999).unwrap().is_none());
}

#[test]
fn list_playlists_ordered_by_position() {
    let conn = seeded_conn();
    let _ = create(&conn, "P1").unwrap();
    let _ = create(&conn, "P2").unwrap();
    let _ = create(&conn, "P3").unwrap();

    let playlists = list(&conn).unwrap();
    assert_eq!(playlists.len(), 3);
    assert_eq!(playlists[0].name, "P1");
    assert_eq!(playlists[1].name, "P2");
    assert_eq!(playlists[2].name, "P3");
}

#[test]
fn list_playlists_includes_track_count() {
    let mut m_conn = crate::db::open(None).unwrap();
    crate::db::migrate(&m_conn).unwrap();
    // Set up same test data
    for id in 1..=5 {
        m_conn
            .execute(
                "INSERT INTO tracks (id, path, title, artist, added_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    id,
                    format!("/x/track{}.flac", id),
                    format!("Track {}", id),
                    format!("Artist {}", id),
                    1000 + id,
                ],
            )
            .unwrap();
    }
    let id = create(&m_conn, "P1").unwrap();
    add_tracks(&mut m_conn, id, &[1, 2, 3]).unwrap();

    let playlists = list(&m_conn).unwrap();
    assert_eq!(playlists.len(), 1);
    assert_eq!(playlists[0].track_count, 3);
}

#[test]
fn add_tracks_appends_to_end() {
    let mut conn = seeded_conn();
    let id = create(&conn, "P1").unwrap();
    let inserted = add_tracks(&mut conn, id, &[1, 2, 3]).unwrap();
    assert_eq!(inserted, 3);

    let positions: Vec<i64> = conn
        .prepare("SELECT position FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map(params![id], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(positions, vec![0, 1, 2]);
}

#[test]
fn add_tracks_empty_slice_returns_zero() {
    let mut conn = seeded_conn();
    let id = create(&conn, "P1").unwrap();
    let inserted = add_tracks(&mut conn, id, &[]).unwrap();
    assert_eq!(inserted, 0);
}

#[test]
fn add_tracks_allows_duplicates() {
    let mut conn = seeded_conn();
    let id = create(&conn, "P1").unwrap();
    add_tracks(&mut conn, id, &[1, 2, 1, 3, 1]).unwrap();

    let track_ids: Vec<i64> = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map(params![id], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(track_ids, vec![1, 2, 1, 3, 1]);
}

#[test]
fn add_tracks_multiple_calls_append() {
    let mut conn = seeded_conn();
    let id = create(&conn, "P1").unwrap();
    add_tracks(&mut conn, id, &[1, 2]).unwrap();
    add_tracks(&mut conn, id, &[3, 4]).unwrap();

    let track_ids: Vec<i64> = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map(params![id], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(track_ids, vec![1, 2, 3, 4]);
}

#[test]
fn create_with_tracks_creates_playlist_and_appends_in_one_call() {
    let mut conn = seeded_conn();
    let id = create_with_tracks(&mut conn, "Mix", &[1, 2, 3]).unwrap();

    let name: String = conn
        .query_row(
            "SELECT name FROM playlists WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(name, "Mix");

    let track_ids: Vec<i64> = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map(params![id], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(track_ids, vec![1, 2, 3]);
}

#[test]
fn create_with_tracks_empty_slice_creates_empty_playlist() {
    let mut conn = seeded_conn();
    let id = create_with_tracks(&mut conn, "Empty", &[]).unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id = ?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

/// TDD regression for the import non-atomicity finding: if the append
/// step fails partway (here, a `track_id` that doesn't exist, tripping
/// the `playlist_tracks.track_id` foreign key), the whole transaction
/// rolls back — no orphaned empty playlist row is left in `playlists`.
#[test]
fn create_with_tracks_rolls_back_playlist_row_on_fk_violation() {
    let mut conn = seeded_conn();
    let before: i64 = conn
        .query_row("SELECT COUNT(*) FROM playlists", [], |r| r.get(0))
        .unwrap();

    // Track id 9999 doesn't exist in the seeded data (only 1..=5) — the
    // second insert should trip the foreign key and roll back the first.
    let result = create_with_tracks(&mut conn, "Bad Playlist", &[1, 9999]);
    assert!(result.is_err());

    let after: i64 = conn
        .query_row("SELECT COUNT(*) FROM playlists", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, after, "no playlist row should survive the rollback");

    let name_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM playlists WHERE name = 'Bad Playlist'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(name_exists, 0);
}

#[test]
fn remove_positions_single() {
    let mut conn = seeded_conn();
    let id = create(&conn, "P1").unwrap();
    add_tracks(&mut conn, id, &[1, 2, 3, 4]).unwrap();
    let removed = remove_positions(&mut conn, id, &[1]).unwrap();
    assert_eq!(removed, 1);

    let track_ids: Vec<i64> = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map(params![id], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(track_ids, vec![1, 3, 4]);

    let positions: Vec<i64> = conn
        .prepare("SELECT position FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map(params![id], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(positions, vec![0, 1, 2]); // Renumbered to be contiguous
}

#[test]
fn remove_positions_multiple() {
    let mut conn = seeded_conn();
    let id = create(&conn, "P1").unwrap();
    add_tracks(&mut conn, id, &[1, 2, 3, 4, 5]).unwrap();
    let removed = remove_positions(&mut conn, id, &[1, 3]).unwrap();
    assert_eq!(removed, 2);

    let track_ids: Vec<i64> = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map(params![id], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(track_ids, vec![1, 3, 5]);

    let positions: Vec<i64> = conn
        .prepare("SELECT position FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map(params![id], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(positions, vec![0, 1, 2]);
}

#[test]
fn remove_positions_empty_slice() {
    let mut conn = seeded_conn();
    let id = create(&conn, "P1").unwrap();
    add_tracks(&mut conn, id, &[1, 2, 3]).unwrap();
    let removed = remove_positions(&mut conn, id, &[]).unwrap();
    assert_eq!(removed, 0);

    let track_ids: Vec<i64> = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map(params![id], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(track_ids, vec![1, 2, 3]);
}

#[test]
fn move_position_down() {
    let mut conn = seeded_conn();
    let id = create(&conn, "P1").unwrap();
    add_tracks(&mut conn, id, &[1, 2, 3, 4]).unwrap();
    move_position(&mut conn, id, 0, 2).unwrap();

    let track_ids: Vec<i64> = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map(params![id], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(track_ids, vec![2, 3, 1, 4]);
}

#[test]
fn move_position_up() {
    let mut conn = seeded_conn();
    let id = create(&conn, "P1").unwrap();
    add_tracks(&mut conn, id, &[1, 2, 3, 4]).unwrap();
    move_position(&mut conn, id, 3, 1).unwrap();

    let track_ids: Vec<i64> = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map(params![id], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(track_ids, vec![1, 4, 2, 3]);
}

#[test]
fn move_position_same_is_noop() {
    let mut conn = seeded_conn();
    let id = create(&conn, "P1").unwrap();
    add_tracks(&mut conn, id, &[1, 2, 3]).unwrap();
    move_position(&mut conn, id, 1, 1).unwrap();

    let track_ids: Vec<i64> = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map(params![id], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(track_ids, vec![1, 2, 3]);
}

#[test]
fn smart_rules_to_sql_empty_rules() {
    let (where_clause, params) = smart_rules_to_sql("[]").unwrap();
    assert_eq!(where_clause, "1=1");
    assert!(params.is_empty());
}

#[test]
fn smart_rules_to_sql_equals() {
    let json = r#"[{"field":"artist","op":"=","value":"Alice"}]"#;
    let (where_clause, params) = smart_rules_to_sql(json).unwrap();
    assert!(where_clause.contains("artist = ?"));
    assert_eq!(params.len(), 1);
}

#[test]
fn smart_rules_to_sql_not_null() {
    let json = r#"[{"field":"last_played_at","op":"not-null"}]"#;
    let (where_clause, params) = smart_rules_to_sql(json).unwrap();
    assert!(where_clause.contains("last_played_at IS NOT NULL"));
    assert!(params.is_empty());
}

#[test]
fn smart_rules_to_sql_is_null() {
    let json = r#"[{"field":"rating","op":"is-null"}]"#;
    let (where_clause, params) = smart_rules_to_sql(json).unwrap();
    assert!(where_clause.contains("rating IS NULL"));
    assert!(params.is_empty());
}

#[test]
fn smart_rules_to_sql_contains_with_escaping() {
    let json = r#"[{"field":"title","op":"contains","value":"50%"}]"#;
    let (_where_clause, params) = smart_rules_to_sql(json).unwrap();
    assert_eq!(params.len(), 1);
    // Verify % is escaped
    if let rusqlite::types::Value::Text(s) = &params[0] {
        assert!(s.contains("\\%"));
    }
}

#[test]
fn smart_rules_to_sql_contains_underscore_escaping() {
    let json = r#"[{"field":"artist","op":"contains","value":"X_Y"}]"#;
    let (_where_clause, params) = smart_rules_to_sql(json).unwrap();
    if let rusqlite::types::Value::Text(s) = &params[0] {
        assert!(s.contains("\\_"));
    }
}

#[test]
fn smart_rules_to_sql_contains_backslash_escaping() {
    // Test that a user value with backslash is fully escaped (no live wildcards).
    // Input: "a\\%" in JSON → represents string value a\% (one backslash, one percent)
    // After escaping: backslash → \\, percent → \%, result: a\\\%
    // After wrapping wildcards: %a\\\%%
    // In Rust source string literal: "%a\\\\\\%%"
    let json = r#"[{"field":"title","op":"contains","value":"a\\%"}]"#;
    let (_where_clause, params) = smart_rules_to_sql(json).unwrap();
    let expected = "%a\\\\\\%%";
    let rusqlite::types::Value::Text(s) = &params[0] else {
        panic!("expected text param")
    };
    assert_eq!(s, expected);
}

#[test]
fn smart_rules_to_sql_contains_non_string_value_error() {
    // Contains operator with numeric value should error, not degrade to %%
    let json = r#"[{"field":"title","op":"contains","value":42}]"#;
    let result = smart_rules_to_sql(json);
    assert!(matches!(result, Err(SmartRulesError::InvalidValue(_))));
}

#[test]
fn smart_rules_to_sql_missing_value_on_equals() {
    let json = r#"[{"field":"title","op":"="}]"#;
    let result = smart_rules_to_sql(json);
    assert!(matches!(result, Err(SmartRulesError::MissingValue(_))));
}

#[test]
fn smart_rules_to_sql_missing_value_on_not_equals() {
    let json = r#"[{"field":"artist","op":"!="}]"#;
    let result = smart_rules_to_sql(json);
    assert!(matches!(result, Err(SmartRulesError::MissingValue(_))));
}

#[test]
fn smart_rules_to_sql_missing_value_on_gte() {
    let json = r#"[{"field":"rating","op":">="}]"#;
    let result = smart_rules_to_sql(json);
    assert!(matches!(result, Err(SmartRulesError::MissingValue(_))));
}

#[test]
fn smart_rules_to_sql_missing_value_on_lte() {
    let json = r#"[{"field":"rating","op":"<="}]"#;
    let result = smart_rules_to_sql(json);
    assert!(matches!(result, Err(SmartRulesError::MissingValue(_))));
}

#[test]
fn smart_rules_to_sql_missing_value_on_gt() {
    let json = r#"[{"field":"duration_ms","op":">"}]"#;
    let result = smart_rules_to_sql(json);
    assert!(matches!(result, Err(SmartRulesError::MissingValue(_))));
}

#[test]
fn smart_rules_to_sql_missing_value_on_lt() {
    let json = r#"[{"field":"duration_ms","op":"<"}]"#;
    let result = smart_rules_to_sql(json);
    assert!(matches!(result, Err(SmartRulesError::MissingValue(_))));
}

#[test]
fn smart_rules_to_sql_missing_value_on_contains() {
    let json = r#"[{"field":"title","op":"contains"}]"#;
    let result = smart_rules_to_sql(json);
    assert!(matches!(result, Err(SmartRulesError::MissingValue(_))));
}

#[test]
fn move_position_out_of_range_unchanged() {
    let mut conn = seeded_conn();
    let id = create(&conn, "P1").unwrap();
    add_tracks(&mut conn, id, &[1, 2, 3]).unwrap();

    // Get initial track order
    let initial_tracks: Vec<i64> = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map(params![id], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    // Move out of range
    let result = move_position(&mut conn, id, 10, 1);
    assert!(result.is_ok());

    // Verify tracks are unchanged
    let final_tracks: Vec<i64> = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map(params![id], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(initial_tracks, final_tracks);
}

#[test]
fn smart_rules_to_sql_unknown_field_error() {
    let json = r#"[{"field":"title; DROP TABLE tracks--","op":"=","value":"x"}]"#;
    let result = smart_rules_to_sql(json);
    assert!(matches!(result, Err(SmartRulesError::UnknownField(_))));
}

#[test]
fn smart_rules_to_sql_unknown_op_error() {
    let json = r#"[{"field":"artist","op":"unknown_op","value":"x"}]"#;
    let result = smart_rules_to_sql(json);
    assert!(matches!(result, Err(SmartRulesError::UnknownOperator(_))));
}

#[test]
fn smart_rules_to_sql_and_joined() {
    let json =
        r#"[{"field":"rating","op":">=","value":4},{"field":"artist","op":"=","value":"Bob"}]"#;
    let (where_clause, params) = smart_rules_to_sql(json).unwrap();
    assert!(where_clause.contains("AND"));
    assert_eq!(params.len(), 2);
}

#[test]
fn list_smart_returns_seeded_playlists() {
    let conn = seeded_conn();
    let playlists = list_smart(&conn).unwrap();
    assert_eq!(playlists.len(), 3);

    // Check by name
    let names: Vec<&str> = playlists.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"Recently played"));
    assert!(names.contains(&"Top rated"));
    assert!(names.contains(&"Recently added"));
}

#[test]
fn list_smart_recently_played_seed() {
    let conn = seeded_conn();
    let playlists = list_smart(&conn).unwrap();
    let recently_played = playlists
        .iter()
        .find(|p| p.name == "Recently played")
        .unwrap();
    assert_eq!(
        recently_played.rules_json,
        r#"[{"field":"last_played_at","op":"not-null"}]"#
    );
    assert_eq!(recently_played.sort_field, "last_played_at");
    assert_eq!(recently_played.sort_dir, "desc");
    assert_eq!(recently_played.limit_count, Some(50));
}

#[test]
fn list_smart_top_rated_seed() {
    let conn = seeded_conn();
    let playlists = list_smart(&conn).unwrap();
    let top_rated = playlists.iter().find(|p| p.name == "Top rated").unwrap();
    assert_eq!(
        top_rated.rules_json,
        r#"[{"field":"rating","op":">=","value":4}]"#
    );
    assert_eq!(top_rated.sort_field, "rating");
    assert_eq!(top_rated.sort_dir, "desc");
    assert_eq!(top_rated.limit_count, None);
}

#[test]
fn list_smart_recently_added_seed() {
    let conn = seeded_conn();
    let playlists = list_smart(&conn).unwrap();
    let recently_added = playlists
        .iter()
        .find(|p| p.name == "Recently added")
        .unwrap();
    assert_eq!(recently_added.rules_json, "[]");
    assert_eq!(recently_added.sort_field, "added_at");
    assert_eq!(recently_added.sort_dir, "desc");
    assert_eq!(recently_added.limit_count, Some(50));
}
