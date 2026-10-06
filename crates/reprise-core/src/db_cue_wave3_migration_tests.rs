//! Schema v91: exclusion identity for CUE segments and persisted render-data
//! failures.

use std::collections::BTreeSet;

use rusqlite::{params, Connection};

const NEW_EXCLUSION_COLUMNS: [&str; 5] = [
    "segment_start_ms|INTEGER|0||0",
    "segment_title|TEXT|0||0",
    "cue_path|TEXT|0||0",
    "cue_mtime|INTEGER|0||0",
    "cue_size|INTEGER|0||0",
];

const FAILURE_COLUMNS: [&str; 8] = [
    "track_id|INTEGER|0||1",
    "source_mtime|INTEGER|1||0",
    "source_size|INTEGER|1||0",
    "source_device|INTEGER|0||0",
    "source_inode|INTEGER|0||0",
    "format_version|INTEGER|1||0",
    "reason|TEXT|1||0",
    "failed_at|INTEGER|1||0",
];

/// The fingerprint a failure marker shares with the stored analysis it stands
/// in for.
const FINGERPRINT_COLUMNS: [&str; 5] = [
    "source_mtime",
    "source_size",
    "source_device",
    "source_inode",
    "format_version",
];

/// The two triggers that drop a track's render data when its audio changes:
/// the file itself, or the stretch of it a CUE segment covers.
const INVALIDATION_TRIGGERS: [&str; 2] = [
    "invalidate_track_render_data",
    "invalidate_segment_render_data",
];

/// What each of those triggers deletes, the failure marker included.
const INVALIDATED_ROWS: [&str; 4] = [
    "DELETE FROM track_spectrograms WHERE track_id = NEW.id;",
    "DELETE FROM track_loudness WHERE track_id = NEW.id;",
    "DELETE FROM render_data_failures WHERE track_id = NEW.id;",
    "UPDATE tracks SET waveform_peaks = NULL WHERE id = NEW.id;",
];

fn fresh() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate_connection(&conn).unwrap();
    conn
}

/// The schema as it was at v90: baseline plus every migration before this one.
fn at_v90() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db_schema_baseline::migrate_baseline(
        &conn,
        false,
        std::path::Path::new("."),
        std::path::Path::new("."),
    )
    .unwrap();
    crate::db_migrations::run_migrations_through(&conn, 90).unwrap();
    conn
}

fn user_version(conn: &Connection) -> i64 {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

fn column_shapes(conn: &Connection, table: &str) -> BTreeSet<String> {
    let mut statement = conn
        .prepare(&format!(
            "SELECT name || '|' || type || '|' || \"notnull\" || '|' || coalesce(dflt_value, '') \
             || '|' || pk FROM pragma_table_info('{table}')"
        ))
        .unwrap();
    let rows = statement.query_map([], |row| row.get(0)).unwrap();
    rows.map(Result::unwrap).collect()
}

fn shape_of(conn: &Connection, table: &str, column: &str) -> String {
    column_shapes(conn, table)
        .into_iter()
        .find(|shape| shape.starts_with(&format!("{column}|")))
        .unwrap_or_else(|| panic!("{table}.{column} is missing"))
}

fn insert_track(conn: &Connection, id: i64) {
    conn.execute(
        "INSERT INTO tracks (id, path, added_at) VALUES (?1, ?2, 1)",
        params![id, format!("/m/{id}.flac")],
    )
    .unwrap();
}

fn insert_failure(
    conn: &Connection,
    track_id: i64,
    source_mtime: i64,
    source_size: i64,
    format_version: i64,
    reason: Option<&str>,
) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO render_data_failures
           (track_id, source_mtime, source_size, source_device, source_inode,
            format_version, reason, failed_at)
         VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?5, 5)",
        params![track_id, source_mtime, source_size, format_version, reason],
    )
}

fn assert_failures_follow_their_track(conn: &Connection) {
    let foreign_key: (String, String, String) = conn
        .query_row(
            "SELECT \"table\", \"to\", on_delete
             FROM pragma_foreign_key_list('render_data_failures')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        foreign_key,
        ("tracks".into(), "id".into(), "CASCADE".into())
    );
}

fn trigger_sql(conn: &Connection, name: &str) -> String {
    conn.query_row(
        "SELECT sql FROM sqlite_schema WHERE type = 'trigger' AND name = ?1",
        [name],
        |row| row.get(0),
    )
    .unwrap_or_else(|error| panic!("trigger {name}: {error}"))
}

fn assert_both_triggers_clear_failures(conn: &Connection) {
    for trigger in INVALIDATION_TRIGGERS {
        let sql = trigger_sql(conn, trigger);
        for statement in INVALIDATED_ROWS {
            assert!(
                sql.contains(statement),
                "{trigger} lacks {statement}: {sql}"
            );
        }
    }
}

#[test]
fn a_fresh_database_has_the_exclusion_identity_columns() {
    let conn = fresh();
    let shapes = column_shapes(&conn, "library_exclusions");
    for column in NEW_EXCLUSION_COLUMNS {
        assert!(shapes.contains(column), "{column} in {shapes:?}");
    }
}

#[test]
fn a_fresh_database_has_the_render_data_failure_table() {
    let conn = fresh();
    let expected: BTreeSet<String> = FAILURE_COLUMNS.iter().map(ToString::to_string).collect();
    assert_eq!(column_shapes(&conn, "render_data_failures"), expected);
}

#[test]
fn the_failure_fingerprint_matches_the_stored_spectrogram_fingerprint() {
    let conn = fresh();
    for column in FINGERPRINT_COLUMNS {
        assert_eq!(
            shape_of(&conn, "render_data_failures", column),
            shape_of(&conn, "track_spectrograms", column),
        );
    }
}

#[test]
fn a_failure_row_follows_its_track_out_of_the_database() {
    let conn = fresh();
    assert_failures_follow_their_track(&conn);

    insert_track(&conn, 1);
    insert_failure(&conn, 1, 10, 20, 1, Some("decode")).unwrap();
    conn.execute("DELETE FROM tracks WHERE id = 1", []).unwrap();
    let left: i64 = conn
        .query_row("SELECT count(*) FROM render_data_failures", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(left, 0);
}

#[test]
fn a_failure_row_needs_a_track() {
    let conn = fresh();
    assert!(insert_failure(&conn, 1, 10, 20, 1, Some("decode")).is_err());
}

#[test]
fn a_failure_row_keeps_the_spectrogram_constraints() {
    let conn = fresh();
    insert_track(&conn, 1);
    for (case, mtime, size, format_version, reason) in [
        ("negative mtime", -1, 20, 1, Some("decode")),
        ("negative size", 10, -1, 1, Some("decode")),
        ("zero format version", 10, 20, 0, Some("decode")),
        ("missing reason", 10, 20, 1, None),
    ] {
        assert!(
            insert_failure(&conn, 1, mtime, size, format_version, reason).is_err(),
            "{case} must be refused",
        );
    }
    insert_failure(&conn, 1, 10, 20, 1, Some("decode")).unwrap();
}

#[test]
fn a_v90_database_keeps_its_exclusions_with_an_empty_identity() {
    let conn = at_v90();
    conn.execute_batch(
        "INSERT INTO library_exclusions (path, device, inode, excluded_at)
         VALUES ('/m/x.flac', 9, 91, 1);
         INSERT INTO library_exclusions (path, device, inode, excluded_at)
         VALUES ('/m/y.flac', NULL, NULL, 2);
         INSERT INTO library_exclusions (path, device, inode, excluded_at, segment_index)
         VALUES ('/m/live.flac', 9, 92, 3, 2);",
    )
    .unwrap();

    super::migrate_v91(&conn).unwrap();

    assert_eq!(user_version(&conn), 91);
    let mut statement = conn
        .prepare(
            "SELECT path, segment_index,
                    segment_start_ms IS NULL AND segment_title IS NULL AND cue_path IS NULL
                    AND cue_mtime IS NULL AND cue_size IS NULL
             FROM library_exclusions ORDER BY excluded_at",
        )
        .unwrap();
    let rows: Vec<(String, i64, bool)> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        rows,
        [
            ("/m/x.flac".to_string(), 0, true),
            ("/m/y.flac".to_string(), 0, true),
            ("/m/live.flac".to_string(), 2, true),
        ]
    );
    let expected: BTreeSet<String> = FAILURE_COLUMNS.iter().map(ToString::to_string).collect();
    assert_eq!(column_shapes(&conn, "render_data_failures"), expected);
    assert_failures_follow_their_track(&conn);
    assert_both_triggers_clear_failures(&conn);
}

#[test]
fn a_rewound_version_runs_the_migration_again_without_changing_anything() {
    let conn = fresh();
    conn.execute(
        "INSERT INTO library_exclusions
           (path, excluded_at, segment_index, segment_start_ms, segment_title,
            cue_path, cue_mtime, cue_size)
         VALUES ('/m/live.flac', 1, 3, 1000, 'Encore', '/m/live.cue', 7, 8)",
        [],
    )
    .unwrap();
    let exclusions = column_shapes(&conn, "library_exclusions");
    let failures = column_shapes(&conn, "render_data_failures");
    conn.pragma_update(None, "user_version", 90).unwrap();

    super::migrate_v91(&conn).unwrap();

    assert_eq!(user_version(&conn), 91);
    assert_eq!(column_shapes(&conn, "library_exclusions"), exclusions);
    assert_eq!(column_shapes(&conn, "render_data_failures"), failures);
    let kept: (i64, String, i64) = conn
        .query_row(
            "SELECT segment_start_ms, segment_title, cue_size FROM library_exclusions",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(kept, (1000, "Encore".to_string(), 8));
}

/// Two tracks, each with a stored spectrogram, loudness, peaks and a failure
/// marker, so a test can see what one track's change takes and what it leaves.
fn seed_render_data(conn: &Connection) {
    for id in [1, 2] {
        conn.execute(
            "INSERT INTO tracks (id, path, added_at, file_mtime, file_size, waveform_peaks)
             VALUES (?1, ?2, 1, 10, 20, x'00')",
            params![id, format!("/m/{id}.flac")],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO track_spectrograms
               (track_id, source_mtime, source_size, format_version, data)
             VALUES (?1, 10, 20, 1, zeroblob(24))",
            [id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO track_loudness
               (track_id, source_mtime, source_size, format_version)
             VALUES (?1, 10, 20, 1)",
            [id],
        )
        .unwrap();
        insert_failure(conn, id, 10, 20, 1, Some("decode")).unwrap();
    }
}

/// Which of the two seeded tracks still hold each kind of render data.
fn render_data_left(conn: &Connection) -> [Vec<i64>; 4] {
    let ids = |sql: &str| -> Vec<i64> {
        let mut statement = conn.prepare(sql).unwrap();
        let rows = statement.query_map([], |row| row.get(0)).unwrap();
        rows.map(Result::unwrap).collect()
    };
    [
        ids("SELECT track_id FROM track_spectrograms ORDER BY track_id"),
        ids("SELECT track_id FROM track_loudness ORDER BY track_id"),
        ids("SELECT track_id FROM render_data_failures ORDER BY track_id"),
        ids("SELECT id FROM tracks WHERE waveform_peaks IS NOT NULL ORDER BY id"),
    ]
}

fn upgraded_from_v90() -> Connection {
    let conn = at_v90();
    super::migrate_v91(&conn).unwrap();
    conn
}

#[test]
fn a_changed_segment_drops_its_failure_marker_with_its_analysis() {
    for (case, conn) in [("fresh", fresh()), ("upgraded", upgraded_from_v90())] {
        seed_render_data(&conn);
        conn.execute("UPDATE tracks SET segment_start_ms = 1000 WHERE id = 1", [])
            .unwrap();
        assert_eq!(
            render_data_left(&conn),
            [vec![2], vec![2], vec![2], vec![2]],
            "{case}: only track 1's render data goes",
        );
    }
}

#[test]
fn a_changed_file_drops_its_failure_marker_with_its_analysis() {
    for (case, conn) in [("fresh", fresh()), ("upgraded", upgraded_from_v90())] {
        seed_render_data(&conn);
        conn.execute("UPDATE tracks SET file_mtime = 11 WHERE id = 1", [])
            .unwrap();
        assert_eq!(
            render_data_left(&conn),
            [vec![2], vec![2], vec![2], vec![2]],
            "{case}: only track 1's render data goes",
        );
    }
}

#[test]
fn both_invalidation_triggers_clear_every_kind_of_render_data() {
    assert_both_triggers_clear_failures(&fresh());
}
