use super::*;

#[test]
fn migrate_v7_foreign_key_cascades_on_track_delete() {
    let conn = open(None).unwrap();
    migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (1, '/x/a.flac', 'A', 'B', 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES (1, 100, 200000)",
        [],
    )
    .unwrap();
    conn.execute("DELETE FROM tracks WHERE id = 1", []).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM listen_events WHERE track_id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn migrate_v5_to_v6_creates_lastfm_queue_and_preserves_listenbrainz_rows() {
    let conn = open(None).unwrap();
    conn.execute_batch(SCHEMA_V1).unwrap();
    conn.execute_batch(SCHEMA_V2).unwrap();
    conn.execute_batch(SCHEMA_V3).unwrap();
    conn.execute_batch(SCHEMA_V4).unwrap();
    conn.execute_batch(SCHEMA_V5).unwrap();
    conn.execute(
        "INSERT INTO listenbrainz_queue \
         (listened_at, artist_name, track_name, release_name, duration_ms) \
         VALUES (1, 'Artist', 'Track', NULL, 120000)",
        [],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 5).unwrap();
    migrate(&conn).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 12);
    let lastfm_exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='lastfm_queue')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(lastfm_exists);
    let preserved: i64 = conn
        .query_row("SELECT COUNT(*) FROM listenbrainz_queue", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(preserved, 1);
}

#[test]
fn migrate_v7_to_v8_adds_waveform_peaks_column() {
    let conn = open(None).unwrap();
    migrate(&conn).unwrap();
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 12);
    conn.execute(
        "INSERT INTO tracks (path, title, artist, added_at) VALUES ('/test.flac', 'T', 'A', 0)",
        [],
    )
    .unwrap();
    let peaks: Option<Vec<u8>> = conn
        .query_row(
            "SELECT waveform_peaks FROM tracks WHERE path = '/test.flac'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(peaks.is_none());
    let test_peaks = (0..1000)
        .map(|index| (index % 256) as u8)
        .collect::<Vec<_>>();
    conn.execute(
        "UPDATE tracks SET waveform_peaks = ?1 WHERE path = '/test.flac'",
        [&test_peaks],
    )
    .unwrap();
    let loaded: Option<Vec<u8>> = conn
        .query_row(
            "SELECT waveform_peaks FROM tracks WHERE path = '/test.flac'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(loaded.unwrap().len(), 1000);
}

#[test]
fn waveform_peaks_crud_round_trips() {
    let conn = open(None).unwrap();
    migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (1, '/t.flac', 'T', 'A', 0)",
        [],
    )
    .unwrap();
    assert!(get_waveform_peaks(&conn, 1).unwrap().is_none());
    let peaks = vec![0, 128, 255, 64, 192];
    set_waveform_peaks(&conn, 1, &peaks).unwrap();
    assert_eq!(get_waveform_peaks(&conn, 1).unwrap().unwrap(), peaks);
}

#[test]
fn migrate_v8_to_v9_creates_device_sync_tables_and_cascades_tracks() {
    let conn = open(None).unwrap();
    migrate(&conn).unwrap();
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 12);

    conn.execute(
        "INSERT INTO device_settings (device_serial, device_name) VALUES ('serial-1', 'Pixel')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (1, '/t.flac', 'T', 'A', 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO device_files (device_serial, track_id, device_path, size, mtime, pinned) \
         VALUES ('serial-1', 1, 'Music/Reprise/A/T.opus', 42, 7, 1)",
        [],
    )
    .unwrap();
    conn.execute("DELETE FROM tracks WHERE id = 1", []).unwrap();
    let remaining: i64 = conn
        .query_row("SELECT COUNT(*) FROM device_files", [], |row| row.get(0))
        .unwrap();
    assert_eq!(remaining, 0);

    migrate(&conn).unwrap();
    let settings: (String, i64, i64, i64) = conn
        .query_row(
            "SELECT selection_json, opus_bitrate, ratings_back, remove_deleted \
             FROM device_settings WHERE device_serial = 'serial-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(settings, ("[]".into(), 0, 0, 1));
}

/// Builds a v9 database (every schema step through `SCHEMA_V9`, `user_version`
/// pinned at 9) so v10-specific tests seed rows under the *pre-migration*
/// shape rather than the shape `migrate()` itself would already have applied.
fn open_v9_database() -> Connection {
    let conn = open(None).unwrap();
    conn.execute_batch(SCHEMA_V1).unwrap();
    conn.execute_batch(SCHEMA_V2).unwrap();
    conn.execute_batch(SCHEMA_V3).unwrap();
    conn.execute_batch(SCHEMA_V4).unwrap();
    conn.execute_batch(SCHEMA_V5).unwrap();
    conn.execute_batch(SCHEMA_V6).unwrap();
    conn.execute_batch(SCHEMA_V7).unwrap();
    conn.execute_batch(SCHEMA_V8).unwrap();
    conn.execute_batch(SCHEMA_V9).unwrap();
    conn.pragma_update(None, "user_version", 9).unwrap();
    conn
}

/// Design decision (a) from the schema v10 doc comment: `missing_since IS
/// NULL` becomes the single source of truth for "file is present", retiring
/// the `missing` boolean (which stays populated for now — Task 1.3 drops it
/// separately). A pre-v10 `missing = 1` row has no recorded start date, so
/// the backfill cannot know whether the file is actually deleted or merely
/// unreachable (e.g. an unmounted drive) — it must land on `missing_reason =
/// 'unknown'`, never `'deleted'`, so nothing downstream ever treats a
/// backfilled row as safely auto-removable without re-verification.
#[test]
fn migrate_v9_to_v10_backfills_missing_since_for_missing_tracks() {
    let conn = open_v9_database();
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, added_at, missing) \
         VALUES (1, '/x/missing.flac', 'A', 'B', 0, 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, added_at, missing) \
         VALUES (2, '/x/present.flac', 'C', 'D', 0, 0)",
        [],
    )
    .unwrap();

    migrate(&conn).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 12);

    let (missing_since, missing_reason): (Option<i64>, Option<String>) = conn
        .query_row(
            "SELECT missing_since, missing_reason FROM tracks WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(missing_since.is_some());
    assert_eq!(missing_reason.as_deref(), Some("unknown"));

    let (missing_since, missing_reason): (Option<i64>, Option<String>) = conn
        .query_row(
            "SELECT missing_since, missing_reason FROM tracks WHERE id = 2",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(missing_since.is_none());
    assert!(missing_reason.is_none());
}

/// Design decision (b) from the schema v10 doc comment: existing
/// `import_errors` rows are discarded, not migrated, on the `DROP TABLE` +
/// `CREATE TABLE` rebuild. Unlike `tracks` rows (user data — ratings,
/// playlist positions — that must survive a migration), `import_errors` rows
/// are reproducible scan state with only a free-text `reason` that cannot be
/// safely parsed into the new typed `reason_kind`/`reason_detail` columns;
/// the next scan recreates any row that is still actually failing, correctly
/// typed this time.
#[test]
fn migrate_v9_to_v10_rebuilds_import_errors_table() {
    let conn = open_v9_database();
    conn.execute(
        "INSERT INTO import_errors (path, reason, occurred_at) VALUES ('/a.flac', 'bad tag', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO import_errors (path, reason, occurred_at) VALUES ('/b.flac', 'io error', 2)",
        [],
    )
    .unwrap();

    migrate(&conn).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 12);

    let remaining: i64 = conn
        .query_row("SELECT COUNT(*) FROM import_errors", [], |row| row.get(0))
        .unwrap();
    assert_eq!(remaining, 0);

    conn.execute(
        "INSERT INTO import_errors (path, reason_kind, reason_detail, first_seen, last_seen) \
         VALUES ('/x', 'io', 'd', 1, 1)",
        [],
    )
    .unwrap();
    let seen_count: i64 = conn
        .query_row(
            "SELECT seen_count FROM import_errors WHERE path = '/x'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(seen_count, 1);
}

/// Design decision from the schema v11 doc comment: each task's commit must
/// leave the test suite green, and a shipped migration must never be edited
/// afterwards — so the column-drop gets its own version rather than being
/// retrofitted into v10. The boolean flag plus a timestamp are two truths for
/// one state and can drift; `missing_since IS NULL` is now the single truth
/// for "file is present", and an auto-clean feature (later task) deletes rows
/// based on that date — a row with an unclear boolean/date agreement would be
/// unacceptable there.
#[test]
fn migrate_v10_to_v11_drops_missing_column_and_preserves_data() {
    let conn = open_v9_database();
    conn.execute_batch(SCHEMA_V10).unwrap();
    conn.pragma_update(None, "user_version", 10).unwrap();

    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, added_at, missing) \
         VALUES (1, '/x/missing.flac', 'A', 'B', 0, 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, added_at, missing) \
         VALUES (2, '/x/present.flac', 'C', 'D', 0, 0)",
        [],
    )
    .unwrap();

    migrate(&conn).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 12);

    // Verify data is intact after column drop
    let (path, title, artist): (String, String, String) = conn
        .query_row(
            "SELECT path, title, artist FROM tracks WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(path, "/x/missing.flac");
    assert_eq!(title, "A");
    assert_eq!(artist, "B");

    let (path, title, artist): (String, String, String) = conn
        .query_row(
            "SELECT path, title, artist FROM tracks WHERE id = 2",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(path, "/x/present.flac");
    assert_eq!(title, "C");
    assert_eq!(artist, "D");

    // Attempt to select the missing column should fail with "no such column"
    let missing_select_result =
        conn.query_row("SELECT missing FROM tracks WHERE id = 1", [], |_row| Ok(()));
    assert!(missing_select_result.is_err());
    if let Err(rusqlite::Error::QueryReturnedNoRows) = missing_select_result {
        // This error should not happen — if the column exists, the query would return rows
        panic!("Unexpected: missing column would exist");
    }
    // The real error is a generic rusqlite::Error with "no such column" in its message
}

#[test]
fn v11_to_v12_adds_disc_number_without_losing_tracks() {
    let conn = open_v9_database();
    conn.execute_batch(SCHEMA_V10).unwrap();
    conn.execute_batch(SCHEMA_V11).unwrap();
    conn.pragma_update(None, "user_version", 11).unwrap();
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, track_no, added_at) \
         VALUES (7, '/music/disc-one.flac', 'First', 'Artist', 'Album', 1, 42)",
        [],
    )
    .unwrap();

    migrate(&conn).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 12);
    let preserved: (String, String, Option<i32>) = conn
        .query_row(
            "SELECT path, title, disc_no FROM tracks WHERE id = 7",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        preserved,
        ("/music/disc-one.flac".into(), "First".into(), None)
    );
}
