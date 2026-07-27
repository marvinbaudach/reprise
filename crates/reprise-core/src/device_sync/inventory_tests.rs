use rusqlite::Connection;

use super::settings::{
    delete_device_playlist, load_device_files, load_device_playlists, load_or_create_settings,
    save_settings, upsert_device_file, upsert_device_playlist, DeviceFileRecord,
    DevicePlaylistRecord,
};
use super::{DeviceSelection, Mp3Quality, SelectionSource, TransferProfile, REPRISE_DEVICE_DIR};

fn open_legacy_v33() -> Connection {
    let conn = crate::db::open(None).unwrap();
    conn.execute_batch(
        "CREATE TABLE tracks (
           id INTEGER PRIMARY KEY,
           path TEXT NOT NULL UNIQUE,
           file_size INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE device_settings (
           device_serial TEXT PRIMARY KEY,
           device_name TEXT NOT NULL,
           selection_json TEXT NOT NULL DEFAULT '[]',
           opus_bitrate INTEGER NOT NULL DEFAULT 0,
           ratings_back INTEGER NOT NULL DEFAULT 0,
           remove_deleted INTEGER NOT NULL DEFAULT 1
         );
         CREATE TABLE device_files (
           device_serial TEXT NOT NULL,
           track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
           device_path TEXT NOT NULL,
           size INTEGER NOT NULL,
           mtime INTEGER NOT NULL,
           pinned INTEGER NOT NULL DEFAULT 0,
           PRIMARY KEY (device_serial, track_id)
         );
         CREATE INDEX idx_device_files_serial ON device_files(device_serial);
         PRAGMA user_version = 33;",
    )
    .unwrap();
    conn
}

#[test]
fn v36_migration_preserves_managed_files_without_the_track_cascade() {
    let conn = open_legacy_v33();
    conn.execute(
        "INSERT INTO device_settings (
           device_serial, device_name, selection_json, opus_bitrate
         ) VALUES ('phone', 'Pixel', '\"entire_library\"', 128)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tracks (id, path, file_size) VALUES (7, '/library/old.flac', 1234)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO device_files (
           device_serial, track_id, device_path, size, mtime, pinned
         ) VALUES (
           'phone', 7, 'Music/Reprise/Artist/Album/01 Old.opus', 456, 99, 1
         )",
        [],
    )
    .unwrap();

    crate::db_device_sync::migrate_v36(&conn).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 36);
    let settings = load_or_create_settings(&conn, "phone", "ignored").unwrap();
    assert_eq!(settings.selection, DeviceSelection::Sources(Vec::new()));
    assert_eq!(settings.profile, TransferProfile::Mp3(Mp3Quality::Kbps256));

    let files = load_device_files(&conn, "phone").unwrap();
    assert_eq!(
        files,
        vec![DeviceFileRecord {
            device_serial: "phone".into(),
            track_id: 7,
            source_path: "/library/old.flac".into(),
            source_size: 1234,
            source_mtime: 99,
            device_path: "Music/Reprise/Artist/Album/01 Old.opus".into(),
            device_size: 456,
            profile_fingerprint: "legacy-opus-v1".into(),
            pinned: true,
        }]
    );

    conn.execute("DELETE FROM tracks WHERE id = 7", []).unwrap();
    assert_eq!(load_device_files(&conn, "phone").unwrap(), files);
}

#[test]
fn v36_migration_preserves_legacy_orphans_even_if_foreign_keys_were_disabled() {
    let conn = open_legacy_v33();
    conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
    conn.execute(
        "INSERT INTO device_files (
           device_serial, track_id, device_path, size, mtime, pinned
         ) VALUES (
           'phone', 88, 'Music/Reprise/Unknown/Orphan.opus', 456, 99, 0
         )",
        [],
    )
    .unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();

    crate::db_device_sync::migrate_v36(&conn).unwrap();

    assert_eq!(
        load_device_files(&conn, "phone").unwrap(),
        vec![DeviceFileRecord {
            device_serial: "phone".into(),
            track_id: 88,
            source_path: String::new(),
            source_size: 0,
            source_mtime: 99,
            device_path: "Music/Reprise/Unknown/Orphan.opus".into(),
            device_size: 456,
            profile_fingerprint: "legacy-opus-v1".into(),
            pinned: false,
        }]
    );
}

#[test]
fn v36_migration_keeps_valid_playlist_selection_and_marks_it_unconfigured_only_when_empty() {
    let conn = open_legacy_v33();
    conn.execute_batch(
        "INSERT INTO device_settings (
           device_serial, device_name, selection_json, opus_bitrate
         ) VALUES
           ('configured', 'Pixel', '[\"playlist:10\",\"smart:20\"]', 192),
           ('empty', 'Tablet', '[]', 0);",
    )
    .unwrap();

    crate::db_device_sync::migrate_v36(&conn).unwrap();

    assert_eq!(
        load_or_create_settings(&conn, "configured", "ignored")
            .unwrap()
            .selection,
        DeviceSelection::Sources(vec![
            SelectionSource::Playlist(10),
            SelectionSource::Smart(20),
        ])
    );
    assert_eq!(
        load_or_create_settings(&conn, "empty", "ignored")
            .unwrap()
            .selection,
        DeviceSelection::Sources(Vec::new())
    );
}

#[test]
fn settings_default_to_mp3_256_and_round_trip_a_supported_profile() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let mut settings = load_or_create_settings(&conn, "new-phone", "New Phone").unwrap();
    assert_eq!(settings.profile, TransferProfile::default());

    settings.selection = DeviceSelection::Sources(vec![SelectionSource::Playlist(42)]);
    settings.profile = TransferProfile::Mp3(Mp3Quality::Kbps320);
    save_settings(&conn, &settings).unwrap();

    assert_eq!(
        load_or_create_settings(&conn, "new-phone", "ignored").unwrap(),
        settings
    );
}

#[test]
fn managed_file_inventory_round_trips_explicit_source_device_and_profile_facts() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let record = DeviceFileRecord {
        device_serial: "phone".into(),
        track_id: 11,
        source_path: "/library/one.flac".into(),
        source_size: 1_000_000,
        source_mtime: 123,
        device_path: format!("{REPRISE_DEVICE_DIR}/Artist/Album/01 One.mp3"),
        device_size: 320_000,
        profile_fingerprint: "mp3-cbr-256-v1".into(),
        pinned: false,
    };

    upsert_device_file(&conn, &record).unwrap();

    assert_eq!(load_device_files(&conn, "phone").unwrap(), vec![record]);
}

#[test]
fn managed_playlist_inventory_tracks_renames_and_deletes_by_source_identity() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let original = DevicePlaylistRecord {
        device_serial: "phone".into(),
        source: SelectionSource::Playlist(42),
        source_name: "Road Trip".into(),
        device_path: format!("{REPRISE_DEVICE_DIR}/Playlists/Road Trip.m3u8"),
    };
    upsert_device_playlist(&conn, &original).unwrap();

    let renamed = DevicePlaylistRecord {
        source_name: "Road Trip 2026".into(),
        device_path: format!("{REPRISE_DEVICE_DIR}/Playlists/Road Trip 2026.m3u8"),
        ..original
    };
    upsert_device_playlist(&conn, &renamed).unwrap();
    assert_eq!(
        load_device_playlists(&conn, "phone").unwrap(),
        vec![renamed.clone()]
    );

    assert!(delete_device_playlist(&conn, "phone", &SelectionSource::Playlist(42)).unwrap());
    assert!(load_device_playlists(&conn, "phone").unwrap().is_empty());
}
