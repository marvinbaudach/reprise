use std::path::PathBuf;

use rusqlite::Connection;

use super::delta::{compute_delta, SyncCandidate};
use super::m3u::{render_named_playlist, DevicePlaylistEntry};
use super::sanitize::{device_track_path, sanitize_component, DevicePathMetadata};
use super::settings::{
    load_device_files, load_or_create_settings, resolve_selection_track_ids, save_settings,
    set_file_pinned, upsert_device_file, DeviceFileRecord, DeviceSelection, DeviceSettings,
    SelectionSource,
};
use super::transfer::{build_transfer_plan, TransferMode};
use super::SyncTrack;

fn migrated() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn
}

#[test]
fn settings_default_then_round_trip_selection_and_supported_bitrate() {
    let conn = migrated();
    let defaults = load_or_create_settings(&conn, "serial-1", "Pixel 8").unwrap();
    assert_eq!(
        defaults,
        DeviceSettings {
            device_serial: "serial-1".into(),
            device_name: "Pixel 8".into(),
            selection: DeviceSelection::Sources(Vec::new()),
            opus_bitrate: 0,
            ratings_back: false,
            remove_deleted: true,
        }
    );

    let changed = DeviceSettings {
        device_name: "Pixel 8 Pro".into(),
        selection: DeviceSelection::Sources(vec![
            SelectionSource::Playlist(42),
            SelectionSource::Smart(3),
        ]),
        opus_bitrate: 128,
        remove_deleted: false,
        ratings_back: true,
        ..defaults
    };
    save_settings(&conn, &changed).unwrap();
    let loaded = load_or_create_settings(&conn, "serial-1", "ignored").unwrap();
    assert_eq!(loaded.device_name, "Pixel 8 Pro");
    assert_eq!(loaded.selection, changed.selection);
    assert_eq!(loaded.opus_bitrate, 128);
    assert!(!loaded.remove_deleted);
    assert!(!loaded.ratings_back, "ratings-back remains disabled in V1");
}

#[test]
fn entire_library_uses_the_documented_scalar_json_shape() {
    let conn = migrated();
    let mut settings = load_or_create_settings(&conn, "serial-2", "Phone").unwrap();
    settings.selection = DeviceSelection::EntireLibrary;
    save_settings(&conn, &settings).unwrap();

    let raw: String = conn
        .query_row(
            "SELECT selection_json FROM device_settings WHERE device_serial = 'serial-2'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(raw, "\"entire_library\"");
    assert_eq!(
        load_or_create_settings(&conn, "serial-2", "Phone")
            .unwrap()
            .selection,
        DeviceSelection::EntireLibrary
    );
}

#[test]
fn device_file_inventory_round_trips_and_pinning_is_per_device() {
    let conn = migrated();
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (7, '/music/a.flac', 'A', 'B', 0)",
        [],
    )
    .unwrap();
    let record = DeviceFileRecord {
        device_serial: "serial-1".into(),
        track_id: 7,
        device_path: "Music/Reprise/B/A/01 Song.opus".into(),
        size: 123,
        mtime: 456,
        pinned: false,
    };
    upsert_device_file(&conn, &record).unwrap();
    set_file_pinned(&conn, "serial-1", 7, true).unwrap();

    let loaded = load_device_files(&conn, "serial-1").unwrap();
    assert_eq!(loaded.len(), 1);
    assert!(loaded[0].pinned);
    assert!(load_device_files(&conn, "other").unwrap().is_empty());
}

#[test]
fn delta_copies_new_or_changed_tracks_and_only_removes_unpinned_unselected_tracks() {
    let selected = vec![
        SyncCandidate {
            track_id: 1,
            device_path: "Artist/Album/01 One.flac".into(),
            transfer_bytes: 100,
            source_mtime: 10,
        },
        SyncCandidate {
            track_id: 2,
            device_path: "Artist/Album/02 Two.opus".into(),
            transfer_bytes: 40,
            source_mtime: 20,
        },
    ];
    let files = vec![
        DeviceFileRecord {
            device_serial: "phone".into(),
            track_id: 1,
            device_path: "Artist/Album/01 One.flac".into(),
            size: 100,
            mtime: 10,
            pinned: false,
        },
        DeviceFileRecord {
            device_serial: "phone".into(),
            track_id: 2,
            device_path: "Artist/Album/02 Two.flac".into(),
            size: 90,
            mtime: 20,
            pinned: false,
        },
        DeviceFileRecord {
            device_serial: "phone".into(),
            track_id: 3,
            device_path: "Old/Three.flac".into(),
            size: 30,
            mtime: 1,
            pinned: false,
        },
        DeviceFileRecord {
            device_serial: "phone".into(),
            track_id: 4,
            device_path: "Pinned/Four.flac".into(),
            size: 30,
            mtime: 1,
            pinned: true,
        },
    ];

    let delta = compute_delta(&selected, &files, true);
    assert_eq!(delta.to_copy, vec![2]);
    assert_eq!(delta.to_remove, vec![3]);
    assert_eq!(delta.bytes, 40);
    assert!(delta.est_secs >= 1);
    assert!(compute_delta(&selected, &files, false).to_remove.is_empty());
}

#[test]
fn fat_safe_paths_use_album_hierarchy_truncate_components_and_suffix_collisions() {
    assert_eq!(sanitize_component("A?B:C*D", "Unknown"), "A_B_C_D");
    let long = "é".repeat(200);
    assert!(sanitize_component(&long, "Unknown").len() <= 120);

    let metadata = DevicePathMetadata {
        album_artist: "Artist".into(),
        artist: "Fallback Artist".into(),
        album: "Album".into(),
        track_number: Some(3),
        title: "Title".into(),
        source_path: PathBuf::from("/library/source.flac"),
    };
    assert_eq!(
        device_track_path(&metadata, Some("opus"), 1),
        "Artist/Album/03 Title.opus"
    );
    assert_eq!(
        device_track_path(&metadata, None, 2),
        "Artist/Album/03 Title (2).flac"
    );
}

#[test]
fn named_playlist_is_replaced_with_relative_utf8_entries() {
    let rendered = render_named_playlist(&[
        DevicePlaylistEntry {
            relative_path: "Artist/Album/01 One.opus".into(),
            duration_secs: 42,
            display: "Artist - One".into(),
        },
        DevicePlaylistEntry {
            relative_path: "Artist/Album/02 Two.mp3".into(),
            duration_secs: 3,
            display: "Artist\nInjected - Two".into(),
        },
    ]);
    assert_eq!(
        rendered,
        "#EXTM3U\n#EXTINF:42,Artist - One\nArtist/Album/01 One.opus\n\
         #EXTINF:3,Artist Injected - Two\nArtist/Album/02 Two.mp3\n"
    );
}

#[test]
fn transfer_plan_transcodes_only_lossless_sources_and_resolves_name_collisions() {
    let tracks = vec![
        SyncTrack {
            id: 1,
            source_path: "/library/one.flac".into(),
            original_name: "one.flac".into(),
            title: "same".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            album_artist: "Artist".into(),
            track_number: Some(1),
            duration_ms: 80_000,
            size_bytes: 1_000_000,
            source_mtime: 10,
        },
        SyncTrack {
            id: 2,
            source_path: "/library/two.mp3".into(),
            original_name: "two.mp3".into(),
            title: "Same".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            album_artist: "Artist".into(),
            track_number: Some(1),
            duration_ms: 80_000,
            size_bytes: 500_000,
            source_mtime: 20,
        },
    ];

    let plan = build_transfer_plan(tracks, 128);
    assert_eq!(plan[0].mode, TransferMode::TranscodeOpus { bitrate: 128 });
    assert_eq!(plan[0].device_path, "Artist/Album/01 same.opus");
    assert_eq!(plan[0].expected_bytes, 1_280_000);
    assert_eq!(plan[1].mode, TransferMode::Copy);
    assert_eq!(plan[1].device_path, "Artist/Album/01 Same (2).mp3");
    assert_eq!(plan[1].expected_bytes, 500_000);
}

#[test]
fn zero_bitrate_preserves_lossless_files_without_transcoding() {
    let track = SyncTrack {
        id: 1,
        source_path: "/library/one.flac".into(),
        original_name: "one.flac".into(),
        title: "One".into(),
        artist: "Artist".into(),
        album: "Album".into(),
        album_artist: String::new(),
        track_number: Some(1),
        duration_ms: 80_000,
        size_bytes: 1_000_000,
        source_mtime: 10,
    };
    let plan = build_transfer_plan(vec![track], 0);
    assert_eq!(plan[0].mode, TransferMode::Copy);
    assert_eq!(plan[0].device_path, "Artist/Album/01 One.flac");
}

#[test]
fn selection_resolves_playlist_union_without_duplicates_and_entire_library_exclusively() {
    let conn = migrated();
    conn.execute_batch(
        "INSERT INTO tracks (id, path, title, artist, added_at) VALUES
           (1, '/1.flac', 'One', 'A', 0),
           (2, '/2.flac', 'Two', 'A', 0),
           (3, '/3.flac', 'Three', 'A', 0);
         INSERT INTO playlists (id, name, position) VALUES
           (10, 'First', 0), (11, 'Second', 1);
         INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES
           (10, 1, 0), (10, 2, 1), (11, 2, 0), (11, 3, 1);",
    )
    .unwrap();

    assert_eq!(
        resolve_selection_track_ids(
            &conn,
            &DeviceSelection::Sources(vec![
                SelectionSource::Playlist(10),
                SelectionSource::Playlist(11),
            ]),
        )
        .unwrap(),
        vec![1, 2, 3]
    );
    assert_eq!(
        resolve_selection_track_ids(&conn, &DeviceSelection::EntireLibrary).unwrap(),
        vec![1, 3, 2]
    );
}
