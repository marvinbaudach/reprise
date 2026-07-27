use std::path::PathBuf;

use super::{
    project_sync_page, DeviceFileRecord, DeviceStorageAccess, DeviceStorageSnapshot, MirrorBlocker,
    MirrorPlaylistSnapshot, MirrorTrack, Mp3Quality, SelectionSource, SyncPageInput, SyncTrack,
    TransferProfile,
};

fn track(id: i64) -> SyncTrack {
    SyncTrack {
        id,
        source_path: PathBuf::from(format!("/music/{id}.flac")),
        original_name: format!("{id}.flac"),
        title: format!("Track {id}"),
        artist: "Artist".into(),
        album: "Album".into(),
        album_artist: "Artist".into(),
        track_number: None,
        duration_ms: 1_000,
        bitrate_kbps: None,
        size_bytes: 100,
        source_mtime: 1,
    }
}

#[test]
fn page_projection_deduplicates_selected_tracks_but_keeps_playlist_repeats() {
    let road = SelectionSource::Playlist(1);
    let mix = SelectionSource::Playlist(2);
    let projection = project_sync_page(SyncPageInput {
        selected: vec![road.clone(), mix.clone()],
        playlists: vec![
            MirrorPlaylistSnapshot {
                source: road,
                name: "Road".into(),
                entries: vec![
                    MirrorTrack::Available(track(1)),
                    MirrorTrack::Available(track(2)),
                    MirrorTrack::Available(track(1)),
                ],
            },
            MirrorPlaylistSnapshot {
                source: mix,
                name: "Mix".into(),
                entries: vec![
                    MirrorTrack::Available(track(2)),
                    MirrorTrack::Available(track(3)),
                ],
            },
        ],
        profile: TransferProfile::Mp3(Mp3Quality::Kbps256),
        storage: DeviceStorageSnapshot {
            total_bytes: Some(2_000_000),
            free_bytes: Some(1_000_000),
            ..Default::default()
        },
        ..SyncPageInput::default()
    });

    assert_eq!(projection.page.unique_track_count, 3);
    assert_eq!(projection.page.profile_options, TransferProfile::ALL);
    assert_eq!(
        projection.page.profile,
        TransferProfile::Mp3(Mp3Quality::Kbps256)
    );
    assert_eq!(projection.page.target_bytes, 292_908);
    assert_eq!(projection.page.playlists[1].entry_count, 3);
    assert_eq!(projection.page.playlists[1].unique_track_count, 2);
}

#[test]
fn empty_selection_keeps_controls_destructive_work_and_storage_projection_blocked() {
    let mut projection = project_sync_page(SyncPageInput::default());
    projection.page.update_controls(true, true, false);

    assert_eq!(
        projection.page.blockers,
        [MirrorBlocker::NoPlaylistsSelected]
    );
    assert_eq!(projection.page.changes.removals, 0);
    assert!(!projection.page.controls.can_start);
}

#[test]
fn controls_do_not_offer_a_start_when_transfers_exceed_current_free_space() {
    let source = SelectionSource::Playlist(1);
    let mut projection = project_sync_page(SyncPageInput {
        selected: vec![source.clone()],
        playlists: vec![MirrorPlaylistSnapshot {
            source,
            name: "Road".into(),
            entries: vec![MirrorTrack::Available(track(1))],
        }],
        inventory: vec![DeviceFileRecord {
            device_serial: "phone".into(),
            track_id: 99,
            source_path: "/music/99.flac".into(),
            source_size: 100_000,
            source_mtime: 1,
            device_path: "Artist/Album/99.mp3".into(),
            device_size: 100_000,
            profile_fingerprint: "mp3-cbr-256-v1".into(),
            pinned: false,
        }],
        storage: DeviceStorageSnapshot {
            total_bytes: Some(1_000_000),
            free_bytes: Some(10_000),
            reprise_music_bytes: 100_000,
            ..Default::default()
        },
        ..SyncPageInput::default()
    });
    projection.page.update_controls(true, true, false);

    assert_eq!(projection.page.changes.transfer_bytes, 85_636);
    assert!(matches!(
        projection.page.storage.state,
        super::StorageProjectionState::Fits
    ));
    assert!(!projection.page.controls.can_start);
}

#[test]
fn controls_do_not_offer_a_start_for_a_known_read_only_target() {
    let source = SelectionSource::Playlist(1);
    let mut projection = project_sync_page(SyncPageInput {
        selected: vec![source.clone()],
        playlists: vec![MirrorPlaylistSnapshot {
            source,
            name: "Road".into(),
            entries: vec![MirrorTrack::Available(track(1))],
        }],
        storage: DeviceStorageSnapshot {
            access: DeviceStorageAccess::ReadOnly,
            total_bytes: Some(1_000_000),
            free_bytes: Some(500_000),
            ..DeviceStorageSnapshot::default()
        },
        ..SyncPageInput::default()
    });

    projection.page.update_controls(true, true, false);

    assert!(!projection.page.controls.can_start);
}
