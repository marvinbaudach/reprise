//! Schema v29 migration regressions — the AI-jobs/provenance/role shapes.

use super::*;

/// Reads the persisted `CREATE` text for one table plus every index attached
/// to it, ordered so fresh and upgraded databases compare byte-for-byte.
fn object_schema(conn: &Connection, table: &str) -> Vec<(String, String)> {
    let mut statement = conn
        .prepare(
            "SELECT name, COALESCE(sql, '') FROM sqlite_schema \
             WHERE (type = 'table' AND name = ?1) \
                OR (type = 'index' AND tbl_name = ?1) \
             ORDER BY type, name",
        )
        .unwrap();
    statement
        .query_map([table], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

/// Rolls a fully-migrated database back to just before v29, leaving every
/// earlier shape intact — the upgrade half of the parity tests. The v28
/// change_log table stays, so re-migration resumes at v28 and only replays the
/// v29 AI-jobs step.
fn reset_to_v28(conn: &Connection) {
    conn.execute_batch(
        "DROP TABLE ai_jobs;
         DROP TABLE track_provenance;
         ALTER TABLE playlists DROP COLUMN role;
         ALTER TABLE new_releases DROP COLUMN track_count;
         PRAGMA user_version = 28;",
    )
    .unwrap();
}

fn seed_track(conn: &Connection, id: i64) {
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, added_at, file_mtime, file_size) \
         VALUES (?1, ?2, 'T', 'A', 1, 1, 1)",
        rusqlite::params![id, format!("/music/{id}.flac")],
    )
    .unwrap();
}

#[test]
fn fresh_and_upgraded_databases_have_the_same_ai_jobs_shape() {
    let fresh = open(None).unwrap();
    migrate(&fresh).unwrap();
    let upgraded = open(None).unwrap();
    migrate(&upgraded).unwrap();
    reset_to_v28(&upgraded);
    migrate(&upgraded).unwrap();

    for table in ["ai_jobs", "track_provenance"] {
        assert_eq!(
            object_schema(&upgraded, table),
            object_schema(&fresh, table),
            "{table} schema differs between fresh and upgraded"
        );
    }
    assert_eq!(
        upgraded
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        SUPPORTED_SCHEMA_VERSION
    );
}

#[test]
fn upgrade_preserves_existing_library_rows() {
    let conn = open(None).unwrap();
    migrate(&conn).unwrap();
    reset_to_v28(&conn);
    seed_track(&conn, 1);
    conn.execute(
        "INSERT INTO playlists (id, name, position) VALUES (1, 'Keep', 0)",
        [],
    )
    .unwrap();

    migrate(&conn).unwrap();

    let (title, name): (String, String) = conn
        .query_row(
            "SELECT (SELECT title FROM tracks WHERE id = 1), \
                    (SELECT name FROM playlists WHERE id = 1)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((title.as_str(), name.as_str()), ("T", "Keep"));
    // The role column exists and defaults to NULL for pre-existing playlists.
    let role: Option<String> = conn
        .query_row("SELECT role FROM playlists WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert!(role.is_none());
}

#[test]
fn ai_jobs_columns_match_the_plan() {
    let conn = open(None).unwrap();
    migrate(&conn).unwrap();
    let columns = conn
        .prepare("PRAGMA table_info(ai_jobs)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(
        columns,
        [
            "id",
            "kind",
            "batch_id",
            "source_track_id",
            "params_json",
            "params_fingerprint",
            "status",
            "progress_permille",
            "claimed_by",
            "lease_expires_at",
            "cancel_requested",
            "auto_promote",
            "error_kind",
            "created_at",
            "started_at",
            "finished_at",
            "result_track_id",
        ]
    );
}

#[test]
fn status_and_progress_check_constraints_are_enforced() {
    let conn = open(None).unwrap();
    migrate(&conn).unwrap();
    let insert = |status: &str, permille: i64| {
        conn.execute(
            "INSERT INTO ai_jobs (kind, params_json, params_fingerprint, status, progress_permille, created_at) \
             VALUES ('instrumental', '{}', 'fp', ?1, ?2, 0)",
            rusqlite::params![status, permille],
        )
    };
    assert!(insert("queued", 0).is_ok());
    assert!(
        insert("nonsense", 0).is_err(),
        "bogus status must be rejected"
    );
    assert!(
        insert("queued", 1001).is_err(),
        "permille over 1000 must be rejected"
    );
    assert!(
        insert("queued", -1).is_err(),
        "negative permille must be rejected"
    );
}

#[test]
fn dedup_index_blocks_open_and_successful_but_allows_retry_after_failure() {
    let conn = open(None).unwrap();
    migrate(&conn).unwrap();
    seed_track(&conn, 1);
    let insert = |status: &str, result: Option<i64>| {
        conn.execute(
            "INSERT INTO ai_jobs \
               (kind, source_track_id, params_json, params_fingerprint, status, created_at, result_track_id) \
             VALUES ('instrumental', 1, '{}', 'fp', ?1, 0, ?2)",
            rusqlite::params![status, result],
        )
    };
    // One open job: a second identical open job is refused by the unique index.
    assert!(insert("queued", None).is_ok());
    assert!(insert("queued", None).is_err());

    // A failed attempt does not occupy the dedup slot: retry is allowed.
    conn.execute("DELETE FROM ai_jobs", []).unwrap();
    assert!(insert("failed", None).is_ok());
    assert!(
        insert("queued", None).is_ok(),
        "failure must not block a retry"
    );
}

#[test]
fn done_job_frees_its_dedup_slot_when_the_result_track_is_deleted() {
    let conn = open(None).unwrap();
    migrate(&conn).unwrap();
    seed_track(&conn, 1); // source
    seed_track(&conn, 2); // instrumental result
    conn.execute(
        "INSERT INTO ai_jobs \
           (kind, source_track_id, params_json, params_fingerprint, status, created_at, result_track_id) \
         VALUES ('instrumental', 1, '{}', 'fp', 'done', 0, 2)",
        [],
    )
    .unwrap();
    // A live successful result blocks a duplicate enqueue (skip + reference).
    assert!(conn
        .execute(
            "INSERT INTO ai_jobs (kind, source_track_id, params_json, params_fingerprint, status, created_at) \
             VALUES ('instrumental', 1, '{}', 'fp', 'queued', 0)",
            [],
        )
        .is_err());

    // Deleting the instrumental nulls result_track_id via the FK, dropping the
    // done row out of the partial index — the work becomes re-enqueueable
    // (Beschluss 16: deleting the instrumental is a normal, repeatable delete).
    conn.execute("DELETE FROM tracks WHERE id = 2", []).unwrap();
    let freed_result: Option<i64> = conn
        .query_row("SELECT result_track_id FROM ai_jobs", [], |row| row.get(0))
        .unwrap();
    assert!(freed_result.is_none());
    assert!(conn
        .execute(
            "INSERT INTO ai_jobs (kind, source_track_id, params_json, params_fingerprint, status, created_at) \
             VALUES ('instrumental', 1, '{}', 'fp', 'queued', 0)",
            [],
        )
        .is_ok());
}

#[test]
fn deleting_the_source_track_nulls_references_but_keeps_rows() {
    let conn = open(None).unwrap();
    migrate(&conn).unwrap();
    seed_track(&conn, 1); // source original
    seed_track(&conn, 2); // instrumental result
    conn.execute(
        "INSERT INTO ai_jobs \
           (kind, source_track_id, params_json, params_fingerprint, status, created_at, result_track_id) \
         VALUES ('instrumental', 1, '{}', 'fp', 'done', 0, 2)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO track_provenance (track_id, kind, ai, source_track_id, source_text, created_at) \
         VALUES (2, 'vocals-removed', 1, 1, 'A — T', 0)",
        [],
    )
    .unwrap();

    // Delete the ORIGINAL: the instrumental, its job row and its provenance
    // row all survive; only the source links go NULL (Beschluss 16).
    conn.execute("DELETE FROM tracks WHERE id = 1", []).unwrap();

    let (job_source, prov_source, prov_text): (Option<i64>, Option<i64>, String) = conn
        .query_row(
            "SELECT (SELECT source_track_id FROM ai_jobs), \
                    (SELECT source_track_id FROM track_provenance WHERE track_id = 2), \
                    (SELECT source_text FROM track_provenance WHERE track_id = 2)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert!(job_source.is_none());
    assert!(prov_source.is_none());
    assert_eq!(
        prov_text, "A — T",
        "textual provenance survives the source delete"
    );
    // The instrumental result track itself is untouched.
    let result_exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM tracks WHERE id = 2)",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(result_exists);
}

#[test]
fn deleting_the_instrumental_cascades_its_provenance_row() {
    let conn = open(None).unwrap();
    migrate(&conn).unwrap();
    seed_track(&conn, 2);
    conn.execute(
        "INSERT INTO track_provenance (track_id, kind, ai, created_at) \
         VALUES (2, 'vocals-removed', 1, 0)",
        [],
    )
    .unwrap();
    conn.execute("DELETE FROM tracks WHERE id = 2", []).unwrap();
    let remaining: i64 = conn
        .query_row("SELECT COUNT(*) FROM track_provenance", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(remaining, 0, "provenance is meaningless without its track");
}
