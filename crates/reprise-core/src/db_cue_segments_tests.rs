use std::collections::BTreeSet;

use rusqlite::Connection;

/// The schema as it was at v89: baseline plus every migration before this one.
fn at_v89() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db_schema_baseline::migrate_baseline(
        &conn,
        false,
        std::path::Path::new("."),
        std::path::Path::new("."),
    )
    .unwrap();
    crate::db_migrations::run_migrations_through(&conn, 89).unwrap();
    conn
}

/// One row in every table that references `tracks`, plus the two history tables
/// that keep a track id without a foreign key.
const SEED: &str = "
INSERT INTO tracks (id, path, title, added_at, file_mtime, file_size, device, inode)
VALUES (1, '/m/a.flac', 'A', 10, 100, 1000, 7, 71),
       (2, '/m/b.flac', 'B', 10, 200, 2000, 7, 72),
       (3, '/m/c.flac', 'C', 10, 300, 3000, 7, 73);
INSERT INTO ai_jobs (kind, params_json, params_fingerprint, created_at, source_track_id, result_track_id)
VALUES ('instrumental', '{}', 'f', 1, 1, 2);
INSERT INTO track_provenance (track_id, kind, created_at, source_track_id) VALUES (2, 'stems', 1, 1);
INSERT INTO playlists (id, name, position) VALUES (1, 'P', 0);
INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (1, 1, 0), (1, 3, 1);
INSERT INTO track_analysis_sidecars (track_id, sidecar_path) VALUES (1, '/m/a.reprise-analysis');
INSERT INTO track_loudness (track_id, source_mtime, source_size, source_device, source_inode, format_version)
VALUES (1, 100, 1000, 7, 71, 1), (2, 200, 2000, 7, 72, 1);
INSERT INTO track_mobile_sync_paths (track_id, device_path) VALUES (3, 'Music/c.flac');
INSERT INTO track_spectrograms (track_id, source_mtime, source_size, source_device, source_inode,
                                format_version, data)
VALUES (1, 100, 1000, 7, 71, 1, zeroblob(24));
INSERT INTO listen_events (track_id, played_at, ms_played) VALUES (1, 5, 6);
INSERT INTO library_exclusions (path, device, inode, excluded_at) VALUES ('/m/x.flac', 9, 91, 1);
INSERT INTO library_exclusions (path, device, inode, excluded_at) VALUES ('/m/y.flac', NULL, NULL, 1);";

const REFERENCING: [(&str, &str); 9] = [
    ("ai_jobs", "result_track_id"),
    ("ai_jobs", "source_track_id"),
    ("playlist_tracks", "track_id"),
    ("track_analysis_sidecars", "track_id"),
    ("track_loudness", "track_id"),
    ("track_mobile_sync_paths", "track_id"),
    ("track_provenance", "source_track_id"),
    ("track_provenance", "track_id"),
    ("track_spectrograms", "track_id"),
];

fn strings(conn: &Connection, sql: &str) -> BTreeSet<String> {
    let mut statement = conn.prepare(sql).unwrap();
    let rows = statement.query_map([], |row| row.get(0)).unwrap();
    rows.map(Result::unwrap).collect()
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .unwrap()
}

fn column_shapes(conn: &Connection, table: &str) -> BTreeSet<String> {
    strings(
        conn,
        &format!(
            "SELECT name || '|' || type || '|' || \"notnull\" || '|' || coalesce(dflt_value, '') \
             || '|' || pk FROM pragma_table_info('{table}')"
        ),
    )
}

fn schema_names(conn: &Connection, kind: &str) -> BTreeSet<String> {
    strings(
        conn,
        &format!(
            "SELECT name FROM sqlite_schema WHERE tbl_name = 'tracks' AND type = '{kind}' \
             AND sql IS NOT NULL"
        ),
    )
}

#[test]
fn the_fixture_covers_every_table_that_references_tracks() {
    let conn = at_v89();
    let actual = strings(
        &conn,
        "SELECT m.name || '.' || f.\"from\" FROM sqlite_schema m, pragma_foreign_key_list(m.name) f \
         WHERE m.type = 'table' AND f.\"table\" = 'tracks'",
    );
    let seeded: BTreeSet<String> = REFERENCING
        .iter()
        .map(|(table, column)| format!("{table}.{column}"))
        .collect();
    assert_eq!(actual, seeded, "a new child table needs a seed row here");
}

#[test]
fn the_rebuild_keeps_every_row_reference_index_and_trigger() {
    let conn = at_v89();
    conn.execute_batch(SEED).unwrap();
    let columns_before = column_shapes(&conn, "tracks");
    let indexes_before = schema_names(&conn, "index");
    let triggers_before = schema_names(&conn, "trigger");
    let children: Vec<(&str, i64)> = REFERENCING
        .iter()
        .map(|(table, _)| (*table, count(&conn, table)))
        .chain([("listen_events", 1), ("tracks", 3)])
        .collect();

    super::migrate_v90(&conn).unwrap();

    for (table, rows) in children {
        assert_eq!(count(&conn, table), rows, "{table}");
    }
    let ids: Vec<i64> = conn
        .prepare("SELECT id FROM tracks ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(ids, [1, 2, 3]);
    let foreign_key_violations: i64 = conn
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(foreign_key_violations, 0);
    assert_eq!(
        conn.query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1,
        "enforcement is switched back on"
    );
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        90
    );

    let added: BTreeSet<String> = [
        "segment_index|INTEGER|1|0|0",
        "segment_start_ms|INTEGER|0||0",
        "segment_end_ms|INTEGER|0||0",
        "cue_path|TEXT|0||0",
        "cue_mtime|INTEGER|0||0",
    ]
    .map(String::from)
    .into();
    let columns_after = column_shapes(&conn, "tracks");
    assert_eq!(
        columns_after
            .difference(&columns_before)
            .cloned()
            .collect::<BTreeSet<_>>(),
        added
    );
    assert!(
        columns_before.is_subset(&columns_after),
        "no column was lost"
    );
    assert_eq!(schema_names(&conn, "index"), indexes_before);
    assert_eq!(schema_names(&conn, "trigger"), triggers_before);
}

#[test]
fn the_rebuilt_table_still_cascades_and_still_invalidates_analysis() {
    let conn = at_v89();
    conn.execute_batch(SEED).unwrap();
    super::migrate_v90(&conn).unwrap();

    conn.execute("UPDATE tracks SET file_mtime = 101 WHERE id = 1", [])
        .unwrap();
    assert_eq!(
        count(&conn, "track_loudness"),
        1,
        "the loudness of track 1 went with its changed file"
    );
    assert_eq!(count(&conn, "track_spectrograms"), 0);

    conn.execute(
        "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES (2, 9, 9)",
        [],
    )
    .unwrap();
    let snapshot: String = conn
        .query_row(
            "SELECT title FROM listen_events WHERE track_id = 2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        snapshot, "B",
        "the trigger on listen_events still reads tracks"
    );

    conn.execute("DELETE FROM tracks WHERE id = 3", []).unwrap();
    assert_eq!(count(&conn, "playlist_tracks"), 1);
    assert_eq!(count(&conn, "track_mobile_sync_paths"), 0);
    conn.execute("DELETE FROM tracks WHERE id = 1", []).unwrap();
    let kept_job: (Option<i64>, Option<i64>) = conn
        .query_row(
            "SELECT source_track_id, result_track_id FROM ai_jobs",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(kept_job, (None, Some(2)), "SET NULL still applies");
}

#[test]
fn a_path_may_repeat_only_with_another_segment_index() {
    let conn = at_v89();
    super::migrate_v90(&conn).unwrap();
    let insert = |index: i64| {
        conn.execute(
            "INSERT INTO tracks (path, added_at, segment_index) VALUES ('/m/album.flac', 1, ?1)",
            [index],
        )
    };
    insert(0).unwrap();
    insert(1).unwrap();
    insert(2).unwrap();
    assert!(insert(1).is_err(), "the same segment twice");
    assert!(insert(0).is_err(), "two whole-file rows for one path");
}

#[test]
fn exclusions_are_per_segment_and_survive_the_rebuild() {
    let conn = at_v89();
    conn.execute_batch(SEED).unwrap();
    super::migrate_v90(&conn).unwrap();
    assert_eq!(count(&conn, "library_exclusions"), 2);
    let whole_file_segments: i64 = conn
        .query_row(
            "SELECT count(*) FROM library_exclusions WHERE segment_index = 0",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(whole_file_segments, 2);
    for segment_index in [1, 2] {
        conn.execute(
            "INSERT INTO library_exclusions (path, device, inode, excluded_at, segment_index)
             VALUES ('/m/x.flac', 9, 91, 1, ?1)",
            [segment_index],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO library_exclusions (path, device, inode, excluded_at, segment_index)
             VALUES ('/m/y.flac', NULL, NULL, 1, ?1)",
            [segment_index],
        )
        .unwrap();
    }
    assert!(conn
        .execute(
            "INSERT INTO library_exclusions (path, device, inode, excluded_at, segment_index)
             VALUES ('/m/x.flac', 9, 91, 1, 1)",
            [],
        )
        .is_err());
}

#[test]
fn running_the_migration_again_changes_nothing() {
    let conn = at_v89();
    conn.execute_batch(SEED).unwrap();
    super::migrate_v90(&conn).unwrap();
    let shape = column_shapes(&conn, "tracks");
    super::migrate_v90(&conn).unwrap();
    assert_eq!(column_shapes(&conn, "tracks"), shape);
    assert_eq!(count(&conn, "tracks"), 3);
}

#[test]
fn a_connection_inside_a_transaction_is_refused_and_nothing_is_dropped() {
    let conn = at_v89();
    conn.execute_batch(SEED).unwrap();
    conn.execute_batch("BEGIN").unwrap();
    assert!(super::migrate_v90(&conn).is_err());
    conn.execute_batch("ROLLBACK").unwrap();
    assert_eq!(count(&conn, "tracks"), 3);
    assert_eq!(count(&conn, "playlist_tracks"), 2);
}

#[test]
fn a_fresh_database_reaches_the_new_schema() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate_connection(&conn).unwrap();
    let columns = strings(&conn, "SELECT name FROM pragma_table_info('tracks')");
    for column in [
        "segment_index",
        "segment_start_ms",
        "segment_end_ms",
        "cue_path",
        "cue_mtime",
    ] {
        assert!(columns.contains(column), "{column}");
    }
}

#[test]
fn a_rewound_version_does_not_rebuild_the_table_again() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate_connection(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks (path, added_at, segment_index, segment_start_ms, segment_end_ms)
         VALUES ('/m/live.flac', 1, 3, 100, 200)",
        [],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 89).unwrap();

    super::migrate_v90(&conn).unwrap();

    let kept: (i64, i64) = conn
        .query_row(
            "SELECT segment_index, segment_end_ms FROM tracks",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(kept, (3, 200));
}
