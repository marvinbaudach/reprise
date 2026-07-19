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
    assert_eq!(version, 16);
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
    assert_eq!(version, 16);
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
    assert_eq!(version, 16);

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
    assert_eq!(version, 16);

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
    assert_eq!(version, 16);

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
    assert_eq!(version, 16);

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

fn open_v11_database() -> Connection {
    let conn = open_v9_database();
    conn.execute_batch(SCHEMA_V10).unwrap();
    conn.execute_batch(SCHEMA_V11).unwrap();
    conn.pragma_update(None, "user_version", 11).unwrap();
    conn
}

fn open_v12_database() -> Connection {
    let conn = open_v11_database();
    conn.execute_batch(SCHEMA_V12).unwrap();
    conn.pragma_update(None, "user_version", 12).unwrap();
    conn
}

#[test]
fn net_2_migration_preserves_existing_cover_usage() {
    let conn = open_v12_database();
    let cover_cache = tempfile::tempdir().unwrap();
    let portrait_cache = tempfile::tempdir().unwrap();
    std::fs::write(cover_cache.path().join("used.jpg"), b"cached").unwrap();

    migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();

    assert!(crate::modules::is_enabled(&conn, &crate::modules::COVER_DOWNLOAD_MODULE).unwrap());
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 16);
}

#[test]
fn net_2_migration_preserves_existing_portrait_usage() {
    let conn = open_v12_database();
    let cover_cache = tempfile::tempdir().unwrap();
    let portrait_cache = tempfile::tempdir().unwrap();
    std::fs::write(portrait_cache.path().join("used.png"), b"cached").unwrap();

    migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();

    assert!(crate::modules::is_enabled(&conn, &crate::modules::ARTIST_PORTRAITS_MODULE).unwrap());
}

#[test]
fn net_2_migration_preserves_online_lyrics_for_existing_databases() {
    let conn = open_v12_database();
    let cover_cache = tempfile::tempdir().unwrap();
    let portrait_cache = tempfile::tempdir().unwrap();

    migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();

    assert!(crate::modules::is_enabled(&conn, &crate::modules::ONLINE_LYRICS_MODULE).unwrap());
}

#[test]
fn net_2_migration_carries_artist_news_opt_in_to_new_releases() {
    let conn = open_v12_database();
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('module.artist_news.enabled', '1')",
        [],
    )
    .unwrap();
    let cover_cache = tempfile::tempdir().unwrap();
    let portrait_cache = tempfile::tempdir().unwrap();

    migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();

    assert!(crate::modules::is_enabled(&conn, &crate::modules::NEW_RELEASES_MODULE).unwrap());
}

#[test]
fn net_2_migration_ignores_negative_cache_markers() {
    let conn = open_v12_database();
    let cover_cache = tempfile::tempdir().unwrap();
    let portrait_cache = tempfile::tempdir().unwrap();
    std::fs::write(cover_cache.path().join("miss.notfound"), b"").unwrap();
    std::fs::write(portrait_cache.path().join("miss.notfound"), b"").unwrap();

    migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();

    assert!(!crate::modules::is_enabled(&conn, &crate::modules::COVER_DOWNLOAD_MODULE).unwrap());
    assert!(!crate::modules::is_enabled(&conn, &crate::modules::ARTIST_PORTRAITS_MODULE).unwrap());
}

#[test]
fn net_2_migration_preserves_explicit_opt_outs() {
    let conn = open_v12_database();
    for key in [
        "module.cover_download.enabled",
        "module.artist_portraits.enabled",
        "module.online_lyrics.enabled",
        "module.new_releases.enabled",
    ] {
        conn.execute("INSERT INTO settings (key, value) VALUES (?1, '0')", [key])
            .unwrap();
    }
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('module.artist_news.enabled', '1')",
        [],
    )
    .unwrap();
    let cover_cache = tempfile::tempdir().unwrap();
    let portrait_cache = tempfile::tempdir().unwrap();
    std::fs::write(cover_cache.path().join("used.jpg"), b"cached").unwrap();
    std::fs::write(portrait_cache.path().join("used.png"), b"cached").unwrap();

    migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();

    for module in [
        &crate::modules::COVER_DOWNLOAD_MODULE,
        &crate::modules::ARTIST_PORTRAITS_MODULE,
        &crate::modules::ONLINE_LYRICS_MODULE,
        &crate::modules::NEW_RELEASES_MODULE,
    ] {
        assert!(!crate::modules::is_enabled(&conn, module).unwrap());
    }
}

fn assert_new_releases_schema(conn: &Connection) {
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 16);

    let track_columns = conn
        .prepare("PRAGMA table_info(tracks)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(track_columns.iter().any(|column| column == "artist_mbid"));
    assert!(track_columns
        .iter()
        .any(|column| column == "artist_mbid_negative"));
    assert!(track_columns.iter().any(|column| column == "disc_no"));

    let release_columns = conn
        .prepare("PRAGMA table_info(new_releases)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        release_columns,
        [
            "release_group_mbid",
            "artist_name",
            "artist_mbid",
            "title",
            "release_type",
            "first_release_date",
            "fetched_at",
            "seen_at",
            "hidden",
            "fallback_accent",
        ]
    );
}

#[test]
fn fresh_database_runs_the_new_releases_migration_sequence() {
    let conn = open(None).unwrap();
    let cover_cache = tempfile::tempdir().unwrap();
    let portrait_cache = tempfile::tempdir().unwrap();
    std::fs::write(cover_cache.path().join("old.jpg"), b"cached").unwrap();
    std::fs::write(portrait_cache.path().join("old.png"), b"cached").unwrap();
    migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();

    assert_new_releases_schema(&conn);
    for module in [
        &crate::modules::COVER_DOWNLOAD_MODULE,
        &crate::modules::ARTIST_PORTRAITS_MODULE,
        &crate::modules::ONLINE_LYRICS_MODULE,
        &crate::modules::NEW_RELEASES_MODULE,
    ] {
        assert!(!crate::modules::is_enabled(&conn, module).unwrap());
    }
}

#[test]
fn v11_database_runs_the_same_new_releases_migration_sequence() {
    let conn = open_v11_database();
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, added_at) \
         VALUES (1, '/music/a.flac', 'A', 'Artist', 0)",
        [],
    )
    .unwrap();

    let cover_cache = tempfile::tempdir().unwrap();
    let portrait_cache = tempfile::tempdir().unwrap();
    migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();

    assert_new_releases_schema(&conn);
    let preserved: (String, Option<String>, i64) = conn
        .query_row(
            "SELECT title, artist_mbid, artist_mbid_negative FROM tracks WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(preserved, ("A".into(), None, 0));
    let network_settings: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM settings WHERE key LIKE 'module.%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(network_settings, 1);
    assert!(crate::modules::is_enabled(&conn, &crate::modules::ONLINE_LYRICS_MODULE).unwrap());
}

#[test]
fn migrate_v12_to_v13_indexes_present_title_order_without_changing_rows() {
    let mut conn = open_v11_database();
    conn.execute_batch(SCHEMA_V12).unwrap();
    conn.pragma_update(None, "user_version", 12).unwrap();
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, added_at) \
         VALUES (1, '/x/z.flac', 'Zulu', 'B', 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, added_at) \
         VALUES (2, '/x/a.flac', 'alpha', 'A', 0)",
        [],
    )
    .unwrap();

    migrate(&conn).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 16);
    let index_sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master \
             WHERE type = 'index' AND name = 'idx_tracks_present_title_nocase'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(index_sql.contains("title COLLATE NOCASE"));
    assert!(index_sql.contains("missing_since IS NULL AND removed_at IS NULL"));

    let query = crate::queries::build_track_query("title", "asc", false);
    let mut statement = conn
        .prepare(&format!("EXPLAIN QUERY PLAN {query}"))
        .unwrap();
    let details = statement
        .query_map(rusqlite::params![200, 0], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert!(details
        .iter()
        .any(|detail| detail.contains("USING INDEX idx_tracks_present_title_nocase")));
    assert!(!details
        .iter()
        .any(|detail| detail.contains("USE TEMP B-TREE FOR ORDER BY")));
    drop(statement);

    let titles = crate::queries::query_track_window(
        &mut conn,
        &crate::view_source::ViewSource::Library,
        "title",
        "asc",
        "",
        0,
        200,
        &[],
    )
    .unwrap()
    .into_iter()
    .map(|track| track.title)
    .collect::<Vec<_>>();
    assert_eq!(titles, ["alpha", "Zulu"]);
}

#[test]
fn migrate_v13_to_v14_indexes_present_album_order_without_changing_rows() {
    let mut conn = open_v9_database();
    conn.execute_batch(SCHEMA_V10).unwrap();
    conn.pragma_update(None, "user_version", 10).unwrap();
    conn.execute_batch(SCHEMA_V11).unwrap();
    conn.pragma_update(None, "user_version", 11).unwrap();
    conn.execute_batch(SCHEMA_V12).unwrap();
    conn.pragma_update(None, "user_version", 12).unwrap();
    conn.execute_batch(SCHEMA_V13).unwrap();
    conn.pragma_update(None, "user_version", 13).unwrap();
    for (id, title, album, track_no) in [
        (1, "Later", "alpha", 5),
        (2, "Last album", "Zulu", 2),
        (3, "Earlier", "alpha", 1),
    ] {
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, album, track_no, added_at) \
             VALUES (?1, ?2, ?3, 'Artist', ?4, ?5, 0)",
            rusqlite::params![id, format!("/x/{id}.flac"), title, album, track_no],
        )
        .unwrap();
    }

    migrate(&conn).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 16);
    let index_sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master \
             WHERE type = 'index' AND name = 'idx_tracks_present_album_order'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(index_sql.contains("album COLLATE NOCASE, track_no"));
    assert!(index_sql.contains("missing_since IS NULL AND removed_at IS NULL"));

    let query = crate::queries::build_track_query("album", "asc", false);
    let mut statement = conn
        .prepare(&format!("EXPLAIN QUERY PLAN {query}"))
        .unwrap();
    let details = statement
        .query_map(rusqlite::params![200, 0], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert!(details
        .iter()
        .any(|detail| detail.contains("USING INDEX idx_tracks_present_album_order")));
    assert!(!details
        .iter()
        .any(|detail| detail.contains("USE TEMP B-TREE FOR ORDER BY")));
    drop(statement);

    for query in [
        "SELECT count(*) FROM tracks \
         WHERE missing_since IS NULL AND removed_at IS NULL \
         AND (title LIKE '%needle%' ESCAPE '\\' OR artist LIKE '%needle%' ESCAPE '\\' \
         OR album LIKE '%needle%' ESCAPE '\\' OR genre LIKE '%needle%' ESCAPE '\\')",
        "SELECT count(*), coalesce(sum(duration_ms), 0) FROM tracks \
         WHERE missing_since IS NULL AND removed_at IS NULL",
    ] {
        let mut aggregate_plan = conn
            .prepare(&format!("EXPLAIN QUERY PLAN {query}"))
            .unwrap();
        let details = aggregate_plan
            .query_map([], |row| row.get::<_, String>(3))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert!(
            details
                .iter()
                .any(|detail| detail.contains("USING INDEX idx_tracks_present_title_nocase")),
            "aggregate query regressed to a cache-hostile plan: {details:?}"
        );
    }

    let titles = crate::queries::query_track_window(
        &mut conn,
        &crate::view_source::ViewSource::Library,
        "album",
        "asc",
        "",
        0,
        200,
        &[],
    )
    .unwrap()
    .into_iter()
    .map(|track| track.title)
    .collect::<Vec<_>>();
    assert_eq!(titles, ["Earlier", "Later", "Last album"]);
}

#[test]
fn migrate_v14_to_v15_adds_disc_number_without_losing_tracks() {
    let conn = open_v11_database();
    conn.execute_batch(SCHEMA_V12).unwrap();
    conn.pragma_update(None, "user_version", 12).unwrap();
    conn.execute_batch(SCHEMA_V13).unwrap();
    conn.pragma_update(None, "user_version", 13).unwrap();
    conn.execute_batch(SCHEMA_V14).unwrap();
    conn.pragma_update(None, "user_version", 14).unwrap();
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
    assert_eq!(version, 16);
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
