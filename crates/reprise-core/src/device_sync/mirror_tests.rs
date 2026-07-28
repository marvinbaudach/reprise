use std::path::PathBuf;

use super::{
    plan_mirror, DeviceFileRecord, DevicePlaylistRecord, ManagedDeviceFile, ManagedRemoval,
    MirrorBlocker, MirrorInput, MirrorPlaylistSnapshot, MirrorTrack, MirrorWarning, Mp3Quality,
    SelectionSource, SyncTrack, TransferProfile, UnavailableTrack,
};

fn track(
    id: i64,
    path: &str,
    bitrate_kbps: Option<u32>,
    duration_ms: i64,
    size_bytes: u64,
) -> SyncTrack {
    SyncTrack {
        id,
        source_path: PathBuf::from(path),
        original_name: path.rsplit('/').next().unwrap_or(path).into(),
        title: format!("Track {id}"),
        artist: "Artist".into(),
        album: "Album".into(),
        album_artist: "Album Artist".into(),
        track_number: Some(u32::try_from(id).unwrap_or_default()),
        duration_ms,
        bitrate_kbps,
        size_bytes,
        source_mtime: 10,
    }
}

fn playlist(
    source: SelectionSource,
    name: &str,
    entries: Vec<MirrorTrack>,
) -> MirrorPlaylistSnapshot {
    MirrorPlaylistSnapshot {
        source,
        name: name.into(),
        entries,
    }
}

fn input(selected: Vec<SelectionSource>, playlists: Vec<MirrorPlaylistSnapshot>) -> MirrorInput {
    MirrorInput {
        selected,
        playlists,
        profile: TransferProfile::Mp3(Mp3Quality::Kbps256),
        inventory: Vec::new(),
        playlist_inventory: Vec::new(),
        managed_files: Vec::new(),
    }
}

fn inventory(track: &SyncTrack, path: &str, fingerprint: &str) -> DeviceFileRecord {
    DeviceFileRecord {
        device_serial: "phone".into(),
        track_id: track.id,
        source_path: track.source_path.to_string_lossy().into_owned(),
        source_size: track.size_bytes,
        source_mtime: track.source_mtime,
        device_path: path.into(),
        device_size: track.size_bytes,
        profile_fingerprint: fingerprint.into(),
        pinned: false,
    }
}

#[test]
fn overlap_and_playlist_repeats_are_deduplicated_only_for_physical_storage() {
    let first = SelectionSource::Playlist(10);
    let second = SelectionSource::Smart(20);
    let low_mp3 = track(1, "/music/one.mp3", Some(192), 10_000, 240_000);
    let flac = track(2, "/music/two.flac", None, 10_000, 1_000_000);
    let high_mp3 = track(3, "/music/three.mp3", Some(320), 5_000, 200_000);
    let plan = plan_mirror(input(
        vec![first.clone(), second.clone()],
        vec![
            playlist(
                first,
                "Repeated",
                vec![
                    MirrorTrack::Available(low_mp3.clone()),
                    MirrorTrack::Available(flac),
                    MirrorTrack::Available(low_mp3.clone()),
                ],
            ),
            playlist(
                second,
                "Smart snapshot",
                vec![
                    MirrorTrack::Available(low_mp3),
                    MirrorTrack::Available(high_mp3),
                ],
            ),
        ],
    ));

    assert!(plan.blockers.is_empty());
    assert_eq!(plan.desired_files.len(), 3);
    assert_eq!(plan.copy.len(), 3);
    assert!(plan.replace.is_empty());
    assert_eq!(plan.transfer_bytes, 1_825_536);
    assert_eq!(plan.target_bytes, 1_825_536);
    assert_eq!(plan.per_playlist[0].entry_count, 3);
    assert_eq!(plan.per_playlist[0].unique_track_count, 2);
    assert_eq!(plan.per_playlist[0].target_bytes, 1_625_536);
    assert_eq!(plan.per_playlist[1].target_bytes, 440_000);
    assert_eq!(plan.playlist_writes[0].entries.len(), 3);
    assert_eq!(
        plan.playlist_writes[0].entries[0].relative_path,
        plan.playlist_writes[0].entries[2].relative_path
    );
    assert!(plan
        .desired_files
        .iter()
        .all(|file| file.device_path.ends_with(".mp3")));
}

#[test]
fn unavailable_references_are_retained_when_inventoried_and_reported_when_absent() {
    let source = SelectionSource::Playlist(10);
    let retained_track = track(7, "/music/seven.flac", None, 10_000, 1_000_000);
    let retained = inventory(
        &retained_track,
        "Album Artist/Album/07 Track 7.mp3",
        "mp3-cbr-256-v1",
    );
    let mut mirror_input = input(
        vec![source.clone()],
        vec![playlist(
            source,
            "Offline",
            vec![
                MirrorTrack::Unavailable(UnavailableTrack {
                    track_id: 7,
                    title: "Track 7".into(),
                    artist: "Artist".into(),
                    duration_ms: 10_000,
                }),
                MirrorTrack::Unavailable(UnavailableTrack {
                    track_id: 8,
                    title: "Track 8".into(),
                    artist: "Artist".into(),
                    duration_ms: 20_000,
                }),
            ],
        )],
    );
    mirror_input.inventory.push(retained.clone());

    let plan = plan_mirror(mirror_input);

    assert!(plan.blockers.is_empty());
    assert_eq!(plan.retained_unavailable, vec![retained]);
    assert!(plan.remove.is_empty());
    assert_eq!(plan.target_bytes, 1_000_000);
    assert_eq!(plan.per_playlist[0].unavailable_count, 2);
    assert_eq!(plan.playlist_writes[0].entries.len(), 1);
    assert_eq!(
        plan.warnings,
        vec![MirrorWarning::UnavailableNotOnDevice { track_id: 8 }]
    );
}

#[test]
fn an_empty_selection_blocks_without_planning_any_destructive_work() {
    let stale = track(1, "/music/one.flac", None, 10_000, 1_000_000);
    let mut mirror_input = input(Vec::new(), Vec::new());
    mirror_input.inventory.push(inventory(
        &stale,
        "Album Artist/Album/01 Track 1.mp3",
        "mp3-cbr-256-v1",
    ));
    mirror_input.managed_files.push(ManagedDeviceFile {
        relative_path: "Orphan.mp3".into(),
        size_bytes: 10,
    });

    let plan = plan_mirror(mirror_input);

    assert_eq!(plan.blockers, vec![MirrorBlocker::NoPlaylistsSelected]);
    assert!(plan.copy.is_empty());
    assert!(plan.replace.is_empty());
    assert!(plan.remove.is_empty());
    assert!(plan.playlist_writes.is_empty());
    assert!(plan.playlist_removals.is_empty());
}

#[test]
fn mtp_17_untracked_physical_files_are_removed_from_the_authoritative_managed_root() {
    let source = SelectionSource::Playlist(10);
    let wanted = track(1, "/music/one.flac", None, 10_000, 1_000_000);
    let mut mirror_input = input(
        vec![source.clone()],
        vec![playlist(
            source,
            "Safe mirror",
            vec![MirrorTrack::Available(wanted)],
        )],
    );
    mirror_input.managed_files.push(ManagedDeviceFile {
        relative_path: "Existing Artist/Existing Album/Existing Song.flac".into(),
        size_bytes: 1_000_000,
    });

    let plan = plan_mirror(mirror_input);

    assert_eq!(plan.copy.len(), 1);
    assert_eq!(
        plan.remove,
        vec![ManagedRemoval::Orphan(ManagedDeviceFile {
            relative_path: "Existing Artist/Existing Album/Existing Song.flac".into(),
            size_bytes: 1_000_000,
        })]
    );
    assert!(plan.warnings.is_empty());
}

#[test]
fn mtp_17_a_desired_physical_playlist_is_not_removed_from_the_authoritative_managed_root() {
    let source = SelectionSource::Playlist(10);
    let wanted = track(1, "/music/one.flac", None, 10_000, 1_000_000);
    let mut mirror_input = input(
        vec![source.clone()],
        vec![playlist(
            source,
            "Safe mirror",
            vec![MirrorTrack::Available(wanted)],
        )],
    );
    mirror_input.managed_files.push(ManagedDeviceFile {
        relative_path: "Safe mirror.m3u8".into(),
        size_bytes: 128,
    });

    let plan = plan_mirror(mirror_input);

    assert_eq!(plan.playlist_writes[0].device_path, "Safe mirror.m3u8");
    assert!(plan.remove.is_empty());
    assert!(plan.warnings.is_empty());
}

#[test]
fn a_deleted_selected_playlist_blocks_instead_of_becoming_an_empty_source() {
    let present = SelectionSource::Playlist(10);
    let deleted = SelectionSource::Playlist(11);
    let stale = track(9, "/music/nine.flac", None, 10_000, 1_000_000);
    let mut mirror_input = input(
        vec![present.clone(), deleted.clone()],
        vec![playlist(present, "Present", Vec::new())],
    );
    mirror_input.inventory.push(inventory(
        &stale,
        "Album Artist/Album/09 Track 9.mp3",
        "mp3-cbr-256-v1",
    ));

    let plan = plan_mirror(mirror_input);

    assert_eq!(plan.blockers, vec![MirrorBlocker::MissingPlaylist(deleted)]);
    assert!(plan.copy.is_empty());
    assert!(plan.replace.is_empty());
    assert!(plan.remove.is_empty());
    assert!(plan.playlist_writes.is_empty());
}

#[test]
fn removed_inventory_rows_and_untracked_files_stay_inside_the_safe_scope() {
    let source = SelectionSource::Playlist(10);
    let wanted = track(1, "/music/one.mp3", Some(192), 10_000, 240_000);
    let deleted = track(2, "/music/two.mp3", Some(192), 10_000, 240_000);
    let unsafe_track = track(3, "/music/three.mp3", Some(192), 10_000, 240_000);
    let wanted_path = "Album Artist/Album/01 Track 1.mp3";
    let mut mirror_input = input(
        vec![source.clone()],
        vec![playlist(
            source,
            "Mirror",
            vec![MirrorTrack::Available(wanted.clone())],
        )],
    );
    mirror_input.inventory = vec![
        inventory(&wanted, wanted_path, "copy-original-v1"),
        DeviceFileRecord {
            pinned: true,
            ..inventory(
                &deleted,
                "Album Artist/Album/02 Track 2.mp3",
                "copy-original-v1",
            )
        },
        inventory(&unsafe_track, "../outside.mp3", "copy-original-v1"),
    ];
    mirror_input.managed_files = vec![
        ManagedDeviceFile {
            relative_path: wanted_path.into(),
            size_bytes: 240_000,
        },
        ManagedDeviceFile {
            relative_path: "Unknown/Orphan.mp3".into(),
            size_bytes: 123,
        },
        ManagedDeviceFile {
            relative_path: "/absolute/outside.mp3".into(),
            size_bytes: 456,
        },
    ];

    let plan = plan_mirror(mirror_input);

    assert!(plan.copy.is_empty());
    assert!(plan.replace.is_empty());
    assert_eq!(plan.remove.len(), 2);
    assert!(plan.remove.iter().any(|removal| matches!(
        removal,
        ManagedRemoval::Inventory(file) if file.track_id == 2
    )));
    assert!(plan.remove.iter().any(|removal| matches!(
        removal,
        ManagedRemoval::Orphan(file) if file.relative_path == "Unknown/Orphan.mp3"
    )));
    assert!(plan.warnings.contains(&MirrorWarning::UnsafeManagedPath {
        path: "../outside.mp3".into(),
    }));
    assert!(plan.warnings.contains(&MirrorWarning::UnsafeManagedPath {
        path: "/absolute/outside.mp3".into(),
    }));
}

#[test]
fn a_profile_change_is_an_explicit_replacement_not_a_size_guess() {
    let source = SelectionSource::Playlist(10);
    let available = track(1, "/music/one.flac", None, 10_000, 1_000_000);
    let desired_path = "Album Artist/Album/01 Track 1.mp3";
    let mut mirror_input = input(
        vec![source.clone()],
        vec![playlist(
            source,
            "Quality",
            vec![MirrorTrack::Available(available.clone())],
        )],
    );
    let mut old = inventory(&available, desired_path, "mp3-cbr-192-v1");
    old.device_size = 320_000;
    mirror_input.inventory.push(old.clone());

    let plan = plan_mirror(mirror_input);

    assert!(plan.copy.is_empty());
    assert_eq!(plan.replace.len(), 1);
    assert_eq!(plan.replace[0].existing, old);
    assert_eq!(
        plan.replace[0].desired.profile_fingerprint,
        "mp3-cbr-256-v1"
    );
    assert_eq!(plan.transfer_bytes, 1_385_536);
}

#[test]
fn changing_the_profile_does_not_replace_a_lossy_original_that_still_passes_through() {
    let source = SelectionSource::Playlist(10);
    let available = track(1, "/music/one.mp3", Some(128), 10_000, 160_000);
    let desired_path = "Album Artist/Album/01 Track 1.mp3";
    let mut mirror_input = input(
        vec![source.clone()],
        vec![playlist(
            source,
            "Quality",
            vec![MirrorTrack::Available(available.clone())],
        )],
    );
    mirror_input.profile = TransferProfile::Opus160;
    mirror_input
        .inventory
        .push(inventory(&available, desired_path, "copy-original-v1"));

    let plan = plan_mirror(mirror_input);

    assert!(plan.copy.is_empty());
    assert!(plan.replace.is_empty());
    assert_eq!(plan.target_bytes, available.size_bytes);
}

#[test]
fn mirror_paths_follow_the_actual_output_instead_of_the_selected_profile_name() {
    let source = SelectionSource::Playlist(10);
    let flac = track(1, "/music/lossless.flac", None, 10_000, 1_000_000);
    let aac = track(2, "/music/lossy.m4a", Some(128), 10_000, 160_000);
    let playlist = |tracks: Vec<SyncTrack>| {
        vec![playlist(
            source.clone(),
            "Profiles",
            tracks.into_iter().map(MirrorTrack::Available).collect(),
        )]
    };

    let opus = plan_mirror(MirrorInput {
        profile: TransferProfile::Opus160,
        ..input(
            vec![source.clone()],
            playlist(vec![flac.clone(), aac.clone()]),
        )
    });
    assert_eq!(
        opus.desired_files[0].device_path,
        "Album Artist/Album/01 Track 1.opus"
    );
    assert_eq!(
        opus.desired_files[1].device_path,
        "Album Artist/Album/02 Track 2.m4a"
    );

    let original = plan_mirror(MirrorInput {
        profile: TransferProfile::Original,
        ..input(vec![source.clone()], playlist(vec![flac, aac]))
    });
    assert_eq!(
        original.desired_files[0].device_path,
        "Album Artist/Album/01 Track 1.flac"
    );
    assert_eq!(
        original.desired_files[1].device_path,
        "Album Artist/Album/02 Track 2.m4a"
    );
}

#[test]
fn playlist_renames_write_the_new_path_and_remove_old_or_deselected_snapshots() {
    let selected = SelectionSource::Playlist(42);
    let deselected = SelectionSource::Smart(99);
    let mut mirror_input = input(
        vec![selected.clone()],
        vec![playlist(selected.clone(), "Road Trip 2026", Vec::new())],
    );
    mirror_input.playlist_inventory = vec![
        DevicePlaylistRecord {
            device_serial: "phone".into(),
            source: selected,
            source_name: "Road Trip".into(),
            device_path: "Road Trip.m3u8".into(),
            last_synced_at: None,
        },
        DevicePlaylistRecord {
            device_serial: "phone".into(),
            source: deselected,
            source_name: "Old Smart".into(),
            device_path: "Old Smart.m3u8".into(),
            last_synced_at: None,
        },
    ];

    let plan = plan_mirror(mirror_input);

    assert_eq!(plan.playlist_writes.len(), 1);
    assert_eq!(plan.playlist_writes[0].device_path, "Road Trip 2026.m3u8");
    assert_eq!(plan.playlist_removals.len(), 2);
    assert!(plan
        .playlist_removals
        .iter()
        .any(|playlist| playlist.device_path == "Road Trip.m3u8"));
    assert!(plan
        .playlist_removals
        .iter()
        .any(|playlist| playlist.device_path == "Old Smart.m3u8"));
}

#[test]
fn colliding_playlist_names_keep_source_stable_paths_when_input_order_changes() {
    let first = SelectionSource::Playlist(1);
    let second = SelectionSource::Smart(2);
    let forward = plan_mirror(input(
        vec![first.clone(), second.clone()],
        vec![
            playlist(first.clone(), "Same", Vec::new()),
            playlist(second.clone(), "same", Vec::new()),
        ],
    ));
    let reversed = plan_mirror(input(
        vec![second.clone(), first.clone()],
        vec![
            playlist(second.clone(), "same", Vec::new()),
            playlist(first.clone(), "Same", Vec::new()),
        ],
    ));
    let paths = |plan: super::MirrorPlan| {
        plan.playlist_writes
            .into_iter()
            .map(|write| (write.source, write.device_path))
            .collect::<std::collections::HashMap<_, _>>()
    };

    assert_eq!(paths(forward), paths(reversed));
}

#[test]
fn inventory_and_directory_enumeration_order_do_not_change_the_plan() {
    let source = SelectionSource::Playlist(10);
    let wanted = track(1, "/music/one.mp3", Some(192), 10_000, 240_000);
    let stale_two = track(2, "/music/two.mp3", Some(192), 10_000, 240_000);
    let stale_three = track(3, "/music/three.mp3", Some(192), 10_000, 240_000);
    let base_input = || {
        input(
            vec![source.clone()],
            vec![playlist(
                source.clone(),
                "Mirror",
                vec![MirrorTrack::Available(wanted.clone())],
            )],
        )
    };
    let mut forward = base_input();
    forward.inventory = vec![
        inventory(
            &stale_two,
            "Album Artist/Album/02 Track 2.mp3",
            "copy-original-mp3-v1",
        ),
        inventory(
            &stale_three,
            "Album Artist/Album/03 Track 3.mp3",
            "copy-original-mp3-v1",
        ),
    ];
    forward.managed_files = vec![
        ManagedDeviceFile {
            relative_path: "Orphans/B.mp3".into(),
            size_bytes: 2,
        },
        ManagedDeviceFile {
            relative_path: "Orphans/A.mp3".into(),
            size_bytes: 1,
        },
    ];
    let mut reversed = forward.clone();
    reversed.inventory.reverse();
    reversed.managed_files.reverse();

    assert_eq!(plan_mirror(forward), plan_mirror(reversed));
}
