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
use super::transfer::{build_transfer_plan, build_transfer_plan_with_inventory, TransferMode};
use super::{Mp3Quality, SyncTrack, TransferProfile};

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
            profile: TransferProfile::default(),
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
        opus_bitrate: 256,
        remove_deleted: false,
        ratings_back: true,
        ..defaults
    };
    save_settings(&conn, &changed).unwrap();
    let loaded = load_or_create_settings(&conn, "serial-1", "ignored").unwrap();
    assert_eq!(loaded.device_name, "Pixel 8 Pro");
    assert_eq!(loaded.selection, changed.selection);
    assert_eq!(loaded.opus_bitrate, 256);
    assert!(!loaded.remove_deleted);
    assert!(!loaded.ratings_back, "ratings-back remains disabled in V1");
}

#[test]
fn entire_library_is_persisted_as_an_unconfigured_selection() {
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
    assert_eq!(raw, "[]");
    assert_eq!(
        load_or_create_settings(&conn, "serial-2", "Phone")
            .unwrap()
            .selection,
        DeviceSelection::Sources(Vec::new())
    );
}

#[test]
fn unknown_selection_json_is_rejected_instead_of_becoming_an_empty_selection() {
    let conn = migrated();
    load_or_create_settings(&conn, "serial-unknown", "Phone").unwrap();
    conn.execute(
        "UPDATE device_settings SET selection_json = '{}' WHERE device_serial = 'serial-unknown'",
        [],
    )
    .unwrap();

    assert!(load_or_create_settings(&conn, "serial-unknown", "Phone").is_err());
}

#[test]
fn empty_selection_json_remains_a_valid_empty_source_selection() {
    let conn = migrated();
    load_or_create_settings(&conn, "serial-empty", "Phone").unwrap();

    assert_eq!(
        load_or_create_settings(&conn, "serial-empty", "Phone")
            .unwrap()
            .selection,
        DeviceSelection::Sources(Vec::new())
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
        source_path: "/music/a.flac".into(),
        source_size: 999,
        source_mtime: 456,
        device_path: "Music/Reprise/B/A/01 Song.opus".into(),
        device_size: 123,
        profile_fingerprint: "legacy-opus-v1".into(),
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
            source_path: "/library/one.flac".into(),
            source_size: 100,
            source_mtime: 10,
            device_path: "Artist/Album/01 One.flac".into(),
            profile_fingerprint: "legacy-v1".into(),
            transfer_bytes: 100,
        },
        SyncCandidate {
            track_id: 2,
            source_path: "/library/two.flac".into(),
            source_size: 90,
            source_mtime: 20,
            device_path: "Artist/Album/02 Two.opus".into(),
            profile_fingerprint: "legacy-opus-128-v1".into(),
            transfer_bytes: 40,
        },
    ];
    let files = vec![
        DeviceFileRecord {
            device_serial: "phone".into(),
            track_id: 1,
            source_path: "/library/one.flac".into(),
            source_size: 100,
            source_mtime: 10,
            device_path: "Artist/Album/01 One.flac".into(),
            device_size: 100,
            profile_fingerprint: "legacy-v1".into(),
            pinned: false,
        },
        DeviceFileRecord {
            device_serial: "phone".into(),
            track_id: 2,
            source_path: "/library/two.flac".into(),
            source_size: 90,
            source_mtime: 20,
            device_path: "Artist/Album/02 Two.flac".into(),
            device_size: 90,
            profile_fingerprint: "legacy-v1".into(),
            pinned: false,
        },
        DeviceFileRecord {
            device_serial: "phone".into(),
            track_id: 3,
            source_path: "/library/three.flac".into(),
            source_size: 30,
            source_mtime: 1,
            device_path: "Old/Three.flac".into(),
            device_size: 30,
            profile_fingerprint: "legacy-v1".into(),
            pinned: false,
        },
        DeviceFileRecord {
            device_serial: "phone".into(),
            track_id: 4,
            source_path: "/library/four.flac".into(),
            source_size: 30,
            source_mtime: 1,
            device_path: "Pinned/Four.flac".into(),
            device_size: 30,
            profile_fingerprint: "legacy-v1".into(),
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
fn delta_recopies_when_the_explicit_profile_fingerprint_changes() {
    let selected = vec![SyncCandidate {
        track_id: 1,
        source_path: "/library/one.flac".into(),
        source_size: 1_000_000,
        source_mtime: 10,
        device_path: "Artist/Album/01 One.opus".into(),
        profile_fingerprint: "legacy-opus-64-v1".into(),
        transfer_bytes: 640_000,
    }];
    let files = vec![DeviceFileRecord {
        device_serial: "phone".into(),
        track_id: 1,
        source_path: "/library/one.flac".into(),
        source_size: 1_000_000,
        source_mtime: 10,
        device_path: "Artist/Album/01 One.opus".into(),
        device_size: 1_280_000,
        profile_fingerprint: "legacy-opus-128-v1".into(),
        pinned: false,
    }];

    let delta = compute_delta(&selected, &files, true);

    assert_eq!(delta.to_copy, [1]);
    assert_eq!(delta.bytes, 640_000);
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
fn component_truncation_does_not_reintroduce_a_trailing_dot() {
    let input = format!("{}.b", "a".repeat(119));

    let sanitized = sanitize_component(&input, "Unknown");

    assert_eq!(sanitized, "a".repeat(119));
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
fn transfer_plan_applies_the_mp3_profile_and_resolves_name_collisions() {
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
            bitrate_kbps: None,
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
            bitrate_kbps: Some(96),
            size_bytes: 500_000,
            source_mtime: 20,
        },
    ];

    let plan = build_transfer_plan(tracks, TransferProfile::Mp3(Mp3Quality::Kbps256));
    assert_eq!(
        plan[0].mode,
        TransferMode::TranscodeMp3 {
            quality: Mp3Quality::Kbps256
        }
    );
    assert_eq!(plan[0].device_path, "Artist/Album/01 same.mp3");
    assert_eq!(plan[0].expected_bytes, 3_625_536);
    assert_eq!(plan[1].mode, TransferMode::Copy);
    assert_eq!(plan[1].device_path, "Artist/Album/01 Same (2).mp3");
    assert_eq!(plan[1].expected_bytes, 500_000);
}

#[test]
fn collision_suffixes_are_stable_when_track_input_order_changes() {
    let track = |id, source: &str| SyncTrack {
        id,
        source_path: source.into(),
        original_name: source.into(),
        title: "Same".into(),
        artist: "Artist".into(),
        album: "Album".into(),
        album_artist: "Artist".into(),
        track_number: Some(1),
        duration_ms: 80_000,
        bitrate_kbps: None,
        size_bytes: 500_000,
        source_mtime: 10,
    };
    let ascending = build_transfer_plan(
        vec![track(1, "/library/one.mp3"), track(2, "/library/two.mp3")],
        TransferProfile::default(),
    );
    let reversed = build_transfer_plan(
        vec![track(2, "/library/two.mp3"), track(1, "/library/one.mp3")],
        TransferProfile::default(),
    );
    let paths = |plan: Vec<super::transfer::TransferPlanEntry>| {
        plan.into_iter()
            .map(|entry| (entry.track.id, entry.device_path))
            .collect::<std::collections::HashMap<_, _>>()
    };

    assert_eq!(paths(ascending), paths(reversed));
}

#[test]
fn collision_suffixes_preserve_selected_and_pinned_inventory_slots() {
    let track = |id, source: &str| SyncTrack {
        id,
        source_path: source.into(),
        original_name: source.into(),
        title: "Same".into(),
        artist: "Artist".into(),
        album: "Album".into(),
        album_artist: "Artist".into(),
        track_number: Some(1),
        duration_ms: 80_000,
        bitrate_kbps: None,
        size_bytes: 500_000,
        source_mtime: 10,
    };
    let inventory = vec![
        DeviceFileRecord {
            device_serial: "phone".into(),
            track_id: 2,
            source_path: "/library/two.mp3".into(),
            source_size: 500_000,
            source_mtime: 10,
            device_path: "Artist/Album/01 Same.mp3".into(),
            device_size: 500_000,
            profile_fingerprint: "legacy-v1".into(),
            pinned: false,
        },
        DeviceFileRecord {
            device_serial: "phone".into(),
            track_id: 99,
            source_path: "/library/old.mp3".into(),
            source_size: 500_000,
            source_mtime: 10,
            device_path: "Artist/Album/01 Same (2).mp3".into(),
            device_size: 500_000,
            profile_fingerprint: "legacy-v1".into(),
            pinned: true,
        },
    ];

    let plan = build_transfer_plan_with_inventory(
        vec![track(1, "/library/one.mp3"), track(2, "/library/two.mp3")],
        TransferProfile::default(),
        &inventory,
    );
    let paths = plan
        .into_iter()
        .map(|entry| (entry.track.id, entry.device_path))
        .collect::<std::collections::HashMap<_, _>>();

    assert_eq!(paths[&2], "Artist/Album/01 Same.mp3");
    assert_eq!(paths[&1], "Artist/Album/01 Same (3).mp3");
}

#[test]
fn default_profile_transcodes_lossless_files_to_opus() {
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
        bitrate_kbps: None,
        size_bytes: 1_000_000,
        source_mtime: 10,
    };
    let plan = build_transfer_plan(vec![track], TransferProfile::default());
    assert_eq!(plan[0].mode, TransferMode::TranscodeOpus160);
    assert_eq!(plan[0].device_path, "Artist/Album/01 One.opus");
}

#[test]
fn unknown_duration_has_no_bounded_conservative_transcode_estimate() {
    let track = SyncTrack {
        id: 1,
        source_path: "/library/one.flac".into(),
        original_name: "one.flac".into(),
        title: "One".into(),
        artist: "Artist".into(),
        album: "Album".into(),
        album_artist: String::new(),
        track_number: Some(1),
        duration_ms: 0,
        bitrate_kbps: None,
        size_bytes: 1_000_000,
        source_mtime: 10,
    };

    let at_opus =
        build_transfer_plan(vec![track.clone()], TransferProfile::Opus160)[0].expected_bytes;
    let at_mp3 = build_transfer_plan(vec![track], TransferProfile::Mp3(Mp3Quality::Kbps256))[0]
        .expected_bytes;

    assert_eq!(at_opus, u64::MAX);
    assert_eq!(at_mp3, u64::MAX);
}

#[test]
fn selection_resolves_playlist_union_without_duplicates_and_disables_entire_library() {
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
        Vec::<i64>::new()
    );
}

/// A legacy in-memory `EntireLibrary` value is inert even if it reaches the
/// old V1 pipeline. The durable v34 migration and settings encoder both turn
/// it into an unconfigured selection, while this guard prevents an outdated
/// caller from reviving the retired whole-library mode.
#[test]
fn entire_library_legacy_value_computes_no_copy_delta() {
    let conn = migrated();
    let dir = tempfile::tempdir().unwrap();
    for (id, title) in [(1, "One"), (2, "Two"), (3, "Three")] {
        let path = dir.path().join(format!("{title}.flac"));
        std::fs::write(&path, b"flac").unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, album, duration_ms, added_at) \
             VALUES (?1, ?2, ?3, 'Artist', 'Album', 180000, 0)",
            rusqlite::params![id, path.to_string_lossy(), title],
        )
        .unwrap();
    }
    // A missing-flagged row and a row whose file vanished must be skipped.
    conn.execute_batch(
        "INSERT INTO tracks (id, path, title, artist, album, added_at, missing_since) VALUES
         (4, '/gone/away.flac', 'Vanished', 'Artist', 'Album', 0, NULL),
         (5, '/marked/missing.flac', 'Missing', 'Artist', 'Album', 0, 1);",
    )
    .unwrap();

    let ids = resolve_selection_track_ids(&conn, &DeviceSelection::EntireLibrary).unwrap();
    let tracks = crate::queries::query_sync_tracks(&conn, &ids).unwrap();
    let plan = build_transfer_plan(tracks, TransferProfile::default());
    let candidates = plan
        .iter()
        .map(|entry| SyncCandidate {
            track_id: entry.track.id,
            source_path: entry.track.source_path.to_string_lossy().into_owned(),
            source_size: entry.track.size_bytes,
            source_mtime: entry.track.source_mtime,
            device_path: entry.device_path.clone(),
            profile_fingerprint: entry.mode.fingerprint(),
            transfer_bytes: entry.expected_bytes,
        })
        .collect::<Vec<_>>();
    let files = load_device_files(&conn, "serial-fresh").unwrap();
    assert!(files.is_empty(), "fresh device starts with no inventory");

    let delta = compute_delta(&candidates, &files, true);
    let mut to_copy = delta.to_copy.clone();
    to_copy.sort_unstable();
    assert!(to_copy.is_empty());
    assert!(delta.to_remove.is_empty());
    assert_eq!(delta.bytes, 0);
}
