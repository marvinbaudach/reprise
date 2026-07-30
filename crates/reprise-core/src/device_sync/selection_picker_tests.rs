use std::collections::{HashMap, HashSet};

use crate::connectivity::LocalAvailability;

use super::*;

fn candidate(episode_id: i64, group_id: i64, published_at: i64) -> EpisodeSelectionCandidate {
    EpisodeSelectionCandidate {
        episode_id,
        group_id,
        published_at,
        played: false,
        local: LocalAvailability::Available,
        pinned: false,
    }
}

#[test]
fn mtp_51_explicit_episode_pin_survives_rule_changes_refreshes_and_ageing_out() {
    let initially_pinned = EpisodeSelectionCandidate {
        pinned: true,
        ..candidate(1, 10, 100)
    };
    let first_refresh = vec![
        initially_pinned.clone(),
        candidate(2, 10, 200),
        candidate(3, 10, 300),
    ];
    let latest_one = EpisodeSelectionRule::LatestPerChannel {
        channel_latest: HashMap::from([(10, 1)]),
    };

    assert_eq!(
        select_episodes(&first_refresh, &latest_one).ready,
        [3, 1],
        "the explicit pin stays selected outside the automatic latest-one window"
    );

    let refreshed = vec![
        initially_pinned,
        candidate(2, 10, 200),
        candidate(3, 10, 300),
        candidate(4, 10, 400),
        candidate(5, 10, 500),
    ];
    let latest_two = EpisodeSelectionRule::LatestPerChannel {
        channel_latest: HashMap::from([(10, 2)]),
    };

    assert_eq!(
        select_episodes(&refreshed, &latest_two).ready,
        [5, 4, 1],
        "the same flag survives a changed rule, a refresh, and further ageing out"
    );
}

#[test]
fn mtp_51_podcast_pin_does_not_override_the_unplayed_standing_rule() {
    let played_and_pinned = EpisodeSelectionCandidate {
        played: true,
        pinned: true,
        ..candidate(1, 10, 100)
    };
    let rule = EpisodeSelectionRule::UnplayedDownloadsOnly {
        enabled_shows: HashSet::from([10]),
    };

    assert!(
        select_episodes(&[played_and_pinned], &rule)
            .ready
            .is_empty(),
        "a podcast episode leaves the phone after it is played even if its explicit flag remains"
    );
}

#[test]
fn picker_footer_sums_only_selected_items_and_keeps_missing_sizes_honest() {
    let items = [
        PickerSelectionItem {
            selected: true,
            content_count: 278,
            size_bytes: Some(2_000),
            needs_download: false,
        },
        PickerSelectionItem {
            selected: false,
            content_count: 99,
            size_bytes: Some(50_000),
            needs_download: true,
        },
        PickerSelectionItem {
            selected: true,
            content_count: 134,
            size_bytes: None,
            needs_download: true,
        },
    ];

    assert_eq!(
        summarize_picker_selection(&items),
        PickerSelectionSummary {
            selected_items: 2,
            content_count: 412,
            known_size_bytes: 2_000,
            unknown_size_items: 1,
            needs_download: 1,
        }
    );
}

#[test]
fn everything_is_a_real_playlist_selection_over_the_whole_library() {
    let tracks = [1_i64, 2, 3]
        .into_iter()
        .map(|id| crate::device_sync::SyncTrack {
            id,
            source_path: format!("/{id}.flac").into(),
            original_name: format!("{id}.flac"),
            title: format!("Track {id}"),
            artist: "Artist".into(),
            album: "Album".into(),
            album_artist: "Artist".into(),
            track_number: None,
            duration_ms: 180_000,
            bitrate_kbps: None,
            size_bytes: 1_000,
            source_mtime: 1,
        })
        .collect::<Vec<_>>();

    let snapshot = everything_playlist_snapshot(tracks);

    assert_eq!(snapshot.source, EVERYTHING_SOURCE);
    assert_eq!(snapshot.name, "Everything");
    assert_eq!(snapshot.entries.len(), 3);
}

#[test]
fn frozen_smart_playlist_keeps_its_published_copy_until_refresh_is_enabled() {
    let frozen = SelectionSource::Smart(7);
    let manual = SelectionSource::Playlist(8);
    let write = |source: SelectionSource| crate::device_sync::PlaylistWrite {
        source,
        source_name: "List".into(),
        device_path: "List.m3u".into(),
        entries: Vec::new(),
        contents: String::new(),
    };
    let device_file = |track_id: i64, path: &str| crate::device_sync::DeviceFileRecord {
        device_serial: "phone".into(),
        track_id,
        source_path: format!("/{path}"),
        source_size: 1_024,
        source_mtime: 1,
        device_path: path.into(),
        device_size: 1_024,
        profile_fingerprint: "original".into(),
        pinned: false,
    };
    let mut plan = crate::device_sync::MirrorPlan {
        playlist_writes: vec![write(frozen.clone()), write(manual.clone())],
        remove: vec![
            crate::device_sync::ManagedRemoval::Inventory(device_file(1, "frozen.flac")),
            crate::device_sync::ManagedRemoval::Inventory(device_file(2, "unrelated.flac")),
            crate::device_sync::ManagedRemoval::Orphan(crate::device_sync::ManagedDeviceFile {
                relative_path: "old.flac".into(),
                size_bytes: 1_024,
            }),
        ],
        bytes_freed: 3_072,
        ..Default::default()
    };

    apply_frozen_smart_playlist_policy(&mut plan, &HashSet::from([frozen]), &HashSet::from([1]));

    assert_eq!(
        plan.playlist_writes
            .iter()
            .map(|write| write.source.clone())
            .collect::<Vec<_>>(),
        [manual],
        "manual playlists still publish while the frozen smart copy stays untouched"
    );
    assert_eq!(
        plan.remove,
        [
            crate::device_sync::ManagedRemoval::Inventory(device_file(2, "unrelated.flac")),
            crate::device_sync::ManagedRemoval::Orphan(crate::device_sync::ManagedDeviceFile {
                relative_path: "old.flac".into(),
                size_bytes: 1_024,
            }),
        ],
        "only tracks named by the frozen snapshot are retained; authoritative cleanup continues"
    );
    assert_eq!(plan.bytes_freed, 2_048);
}
