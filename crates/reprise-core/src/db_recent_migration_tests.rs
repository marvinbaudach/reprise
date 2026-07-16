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
    assert_eq!(version, 9);
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
    assert_eq!(version, 9);
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
    assert_eq!(version, 9);

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
