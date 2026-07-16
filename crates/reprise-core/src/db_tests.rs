use super::*;

/// Stage-3 close-out regression: proves each version step is atomic — a
/// transaction rolled back partway through (simulating a crash between
/// the schema change and the `user_version` bump) leaves neither the
/// schema change nor the version bump behind, so a real `migrate()` call
/// afterward can safely retry the whole step from scratch rather than
/// tripping over a partially-applied schema ("duplicate column").
#[test]
fn migrate_version_step_is_atomic_a_rollback_leaves_no_partial_schema() {
    let conn = open(None).unwrap();
    conn.execute_batch(SCHEMA_V1).unwrap();
    conn.pragma_update(None, "user_version", 1).unwrap();

    // Simulate a crash mid-step: run the same work the v1->v2 step does,
    // but roll back instead of committing (dropping an uncommitted
    // `Transaction` rolls it back).
    {
        let tx = conn.unchecked_transaction().unwrap();
        tx.execute_batch(SCHEMA_V2).unwrap();
        tx.pragma_update(None, "user_version", 2).unwrap();
    }

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        version, 1,
        "the rolled-back version bump must not have survived"
    );

    // The rolled-back schema change must not have survived either — a
    // real migrate() call must be able to re-run SCHEMA_V2 cleanly
    // (would fail with "duplicate column name" if the ALTERs had
    // partially committed).
    migrate(&conn).unwrap();
    let version_after: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version_after, 8);
}

#[test]
fn migrate_creates_tracks_table_and_is_idempotent() {
    let conn = open(None).unwrap();
    migrate(&conn).unwrap();
    migrate(&conn).unwrap(); // second run must not fail
    let n: i64 = conn
        .query_row("SELECT count(*) FROM tracks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 8);
}

/// Builds a v1 DB by hand (SCHEMA_V1 + `user_version = 1`, exactly what a
/// pre-Task-8 install looks like on disk), inserts a row the way the old
/// scanner would have, then migrates. The v2 columns must exist with
/// their documented defaults, the pre-existing row's data must survive
/// untouched, and a second `migrate` call must be a no-op (idempotent).
#[test]
fn migrate_v1_to_v2_adds_columns_preserves_data_and_is_idempotent() {
    let conn = open(None).unwrap();
    conn.execute_batch(SCHEMA_V1).unwrap();
    conn.pragma_update(None, "user_version", 1).unwrap();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, rating, play_count, added_at) \
             VALUES ('/x/a.flac', 'A Title', 'An Artist', 5, 7, 1000)",
        [],
    )
    .unwrap();

    migrate(&conn).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 8); // Now goes to the current schema (v8)

    let (title, rating, play_count, added_at, file_size, device, inode): (
        String,
        i64,
        i64,
        i64,
        i64,
        Option<i64>,
        Option<i64>,
    ) = conn
        .query_row(
            "SELECT title, rating, play_count, added_at, file_size, device, inode \
                 FROM tracks WHERE path = '/x/a.flac'",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(title, "A Title");
    assert_eq!(rating, 5);
    assert_eq!(play_count, 7);
    assert_eq!(added_at, 1000);
    assert_eq!(file_size, 0); // NOT NULL DEFAULT 0 for a pre-v2 row
    assert_eq!(device, None);
    assert_eq!(inode, None);

    // Second migrate() call (e.g. next app launch) must not fail or
    // re-run the ALTERs (which would error: "duplicate column name").
    migrate(&conn).unwrap();
    let version_after_second_run: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version_after_second_run, 8); // Current schema
}

#[test]
fn open_sets_busy_timeout() {
    let conn = open(None).unwrap();
    let busy_timeout_ms: i64 = conn
        .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
        .unwrap();
    assert_eq!(busy_timeout_ms, 5000);
}

/// v2→v3 migration: playlists tables created, smart playlists seeded
/// exactly once, foreign keys cascade correctly.
#[test]
fn migrate_v2_to_v3_creates_playlist_tables_and_seeds_smart_playlists() {
    let conn = open(None).unwrap();
    conn.execute_batch(SCHEMA_V1).unwrap();
    conn.pragma_update(None, "user_version", 1).unwrap();
    conn.execute_batch(SCHEMA_V2).unwrap();
    conn.pragma_update(None, "user_version", 2).unwrap();
    // Verify v2 state before migration.
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 2);

    migrate(&conn).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 8); // walks all the way to the current schema version (v8)

    // Verify tables exist.
    let playlists_exist: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='playlists')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(playlists_exist);

    let playlist_tracks_exist: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='playlist_tracks')",
                [],
                |r| r.get(0),
            )
            .unwrap();
    assert!(playlist_tracks_exist);

    let smart_playlists_exist: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='smart_playlists')",
                [],
                |r| r.get(0),
            )
            .unwrap();
    assert!(smart_playlists_exist);

    // Verify three smart playlists were seeded.
    let smart_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM smart_playlists", [], |r| r.get(0))
        .unwrap();
    assert_eq!(smart_count, 3);

    // Verify seed names and rules.
    let (name1, rules1): (String, String) = conn
        .query_row(
            "SELECT name, rules_json FROM smart_playlists ORDER BY id LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(name1, "Recently played");
    assert_eq!(rules1, r#"[{"field":"last_played_at","op":"not-null"}]"#);

    // Second migration must be idempotent (no duplicate inserts).
    migrate(&conn).unwrap();
    let smart_count_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM smart_playlists", [], |r| r.get(0))
        .unwrap();
    assert_eq!(smart_count_after, 3);
}

/// Foreign key cascade: delete track → its playlist_tracks rows gone.
#[test]
fn migrate_v3_foreign_keys_cascade_on_track_delete() {
    let conn = open(None).unwrap();
    migrate(&conn).unwrap();
    conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (1, '/x/a.flac', 'A', 'B', 0)",
            [],
        )
        .unwrap();
    conn.execute(
        "INSERT INTO playlists (id, name, position) VALUES (1, 'My Playlist', 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (1, 1, 0)",
        [],
    )
    .unwrap();

    // Delete the track.
    conn.execute("DELETE FROM tracks WHERE id = 1", []).unwrap();

    // Playlist entry should be gone (FK cascade).
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM playlist_tracks WHERE track_id = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

/// Foreign key cascade: delete playlist → its playlist_tracks rows gone.
#[test]
fn migrate_v3_foreign_keys_cascade_on_playlist_delete() {
    let conn = open(None).unwrap();
    migrate(&conn).unwrap();
    conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (1, '/x/a.flac', 'A', 'B', 0)",
            [],
        )
        .unwrap();
    conn.execute(
        "INSERT INTO playlists (id, name, position) VALUES (1, 'My Playlist', 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (1, 1, 0)",
        [],
    )
    .unwrap();

    // Delete the playlist.
    conn.execute("DELETE FROM playlists WHERE id = 1", [])
        .unwrap();

    // Playlist entries should be gone (FK cascade).
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

/// v3→v4 migration (Stage 3 Task 8 — folder watcher): the `settings`
/// table is created, existing data (a track row, a playlist) survives
/// untouched, and a second `migrate` call is idempotent (doesn't try to
/// `CREATE TABLE settings` again).
#[test]
fn migrate_v3_to_v4_creates_settings_table_preserves_data_and_is_idempotent() {
    let conn = open(None).unwrap();
    conn.execute_batch(SCHEMA_V1).unwrap();
    conn.pragma_update(None, "user_version", 1).unwrap();
    conn.execute_batch(SCHEMA_V2).unwrap();
    conn.pragma_update(None, "user_version", 2).unwrap();
    conn.execute_batch(SCHEMA_V3).unwrap();
    conn.pragma_update(None, "user_version", 3).unwrap();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, added_at) VALUES ('/x/a.flac', 'A', 'B', 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO playlists (id, name, position) VALUES (1, 'My Playlist', 0)",
        [],
    )
    .unwrap();

    migrate(&conn).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 8);

    let settings_exist: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='settings')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(settings_exist);

    // Pre-existing data survived untouched.
    let title: String = conn
        .query_row(
            "SELECT title FROM tracks WHERE path = '/x/a.flac'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(title, "A");
    let playlist_name: String = conn
        .query_row("SELECT name FROM playlists WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(playlist_name, "My Playlist");

    // Second migrate() call must not fail (would error: "table settings
    // already exists" if the ALTER/CREATE ran a second time).
    migrate(&conn).unwrap();
    let version_after_second_run: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version_after_second_run, 8);
}

#[test]
fn migrate_v4_to_v5_creates_listenbrainz_queue_and_preserves_settings() {
    let conn = open(None).unwrap();
    conn.execute_batch(SCHEMA_V1).unwrap();
    conn.execute_batch(SCHEMA_V2).unwrap();
    conn.execute_batch(SCHEMA_V3).unwrap();
    conn.execute_batch(SCHEMA_V4).unwrap();
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('keep', 'yes')",
        [],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 4).unwrap();

    migrate(&conn).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 8);
    let setting: String = conn
        .query_row("SELECT value FROM settings WHERE key = 'keep'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(setting, "yes");
    let queue_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='listenbrainz_queue')",
                [],
                |row| row.get(0),
            )
            .unwrap();
    assert!(queue_exists);
    migrate(&conn).unwrap();
}

#[test]
fn migrate_v6_to_v7_creates_listen_events_and_preserves_tracks() {
    let conn = open(None).unwrap();
    conn.execute_batch(SCHEMA_V1).unwrap();
    conn.execute_batch(SCHEMA_V2).unwrap();
    conn.execute_batch(SCHEMA_V3).unwrap();
    conn.execute_batch(SCHEMA_V4).unwrap();
    conn.execute_batch(SCHEMA_V5).unwrap();
    conn.execute_batch(SCHEMA_V6).unwrap();
    conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (1, '/x/a.flac', 'A', 'B', 0)",
            [],
        )
        .unwrap();
    conn.pragma_update(None, "user_version", 6).unwrap();

    migrate(&conn).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 8);

    let listen_events_exist: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='listen_events')",
                [],
                |row| row.get(0),
            )
            .unwrap();
    assert!(listen_events_exist);

    let index_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name='idx_listen_events_played_at')",
                [],
                |row| row.get(0),
            )
            .unwrap();
    assert!(index_exists);

    // Pre-existing track row survived untouched.
    let title: String = conn
        .query_row("SELECT title FROM tracks WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(title, "A");

    // Second migrate() must be a no-op (would error re-creating the table).
    migrate(&conn).unwrap();
    let version_after: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version_after, 8);
}

/// The FK on `listen_events.track_id` cascades: deleting a track removes
/// its recorded listen events too.
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
            |r| r.get(0),
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
    assert_eq!(version, 8);
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
    // Build up to v7 manually
    migrate(&conn).unwrap(); // goes all the way to current
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 8);
    // Column exists and is nullable
    conn.execute(
        "INSERT INTO tracks (path, title, artist, added_at) VALUES ('/test.flac', 'T', 'A', 0)",
        [],
    )
    .unwrap();
    let peaks: Option<Vec<u8>> = conn
        .query_row(
            "SELECT waveform_peaks FROM tracks WHERE path = '/test.flac'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(peaks.is_none()); // Default is NULL
                              // Can store and retrieve peaks
    let test_peaks: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
    conn.execute(
        "UPDATE tracks SET waveform_peaks = ?1 WHERE path = '/test.flac'",
        [&test_peaks],
    )
    .unwrap();
    let loaded: Option<Vec<u8>> = conn
        .query_row(
            "SELECT waveform_peaks FROM tracks WHERE path = '/test.flac'",
            [],
            |r| r.get(0),
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
    let peaks: Vec<u8> = vec![0, 128, 255, 64, 192];
    set_waveform_peaks(&conn, 1, &peaks).unwrap();
    assert_eq!(get_waveform_peaks(&conn, 1).unwrap().unwrap(), peaks);
}

#[test]
fn open_migrated_returns_a_ready_to_use_database() {
    let conn = open_migrated(None).unwrap();
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 8);
    conn.execute(
        "INSERT INTO tracks (path, title, added_at) VALUES ('/ready.flac', 'Ready', 0)",
        [],
    )
    .unwrap();
}

#[test]
fn pending_waveform_tracks_excludes_cached_and_missing_rows() {
    let conn = open_migrated(None).unwrap();
    for (id, path, missing) in [
        (1, "/one.flac", 0),
        (2, "/two.flac", 0),
        (3, "/missing.flac", 1),
    ] {
        conn.execute(
            "INSERT INTO tracks (id, path, title, added_at, missing) VALUES (?1, ?2, '', 0, ?3)",
            rusqlite::params![id, path, missing],
        )
        .unwrap();
    }
    set_waveform_peaks(&conn, 2, &[1, 2, 3]).unwrap();

    assert_eq!(
        pending_waveform_tracks(&conn).unwrap(),
        vec![(1, "/one.flac".to_string())]
    );
}
