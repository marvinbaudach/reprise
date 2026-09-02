use std::path::PathBuf;

use super::phase_transitions::{playlist_removal_activity, removal_activity};
use super::sanitize::{device_track_path, DevicePathMetadata};
use super::{
    DeviceFileRecord, DevicePlaylistRecord, ManagedDeviceFile, ManagedRemoval, SelectionSource,
};

fn inventory_removal(device_path: &str) -> ManagedRemoval {
    ManagedRemoval::Inventory(DeviceFileRecord {
        device_serial: "serial-1".into(),
        track_id: 1,
        source_path: "/music/Immortal.flac".into(),
        source_size: 1,
        source_mtime: 0,
        device_path: device_path.into(),
        device_size: 1,
        profile_fingerprint: "profile".into(),
        pinned: false,
    })
}

#[test]
fn a_path_written_by_device_track_path_names_the_removed_track_and_artist() {
    let path = device_track_path(
        &DevicePathMetadata {
            album_artist: "Lorna Shore".into(),
            artist: "Lorna Shore".into(),
            album: "Immortal".into(),
            track_number: Some(3),
            title: "Immortal".into(),
            source_path: PathBuf::from("/music/Immortal.flac"),
        },
        Some("opus"),
        1,
    );

    assert_eq!(
        removal_activity(&inventory_removal(&path)),
        "Immortal — Lorna Shore"
    );
}

#[test]
fn a_three_digit_track_number_is_not_part_of_the_removed_title() {
    assert_eq!(
        removal_activity(&inventory_removal("Artist/Album/100 Title.opus")),
        "Title — Artist"
    );
}

#[test]
fn a_four_digit_track_number_written_by_device_track_path_is_not_part_of_the_removed_title() {
    let path = device_track_path(
        &DevicePathMetadata {
            album_artist: "Artist".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            track_number: Some(1_000),
            title: "Title".into(),
            source_path: PathBuf::from("/music/Title.flac"),
        },
        Some("opus"),
        1,
    );

    assert_eq!(
        removal_activity(&inventory_removal(&path)),
        "Title — Artist"
    );
}

#[test]
fn a_collision_suffix_is_not_part_of_the_removed_title() {
    assert_eq!(
        removal_activity(&inventory_removal(
            "Lorna Shore/Immortal/03 Immortal (2).opus"
        )),
        "Immortal — Lorna Shore"
    );
}

#[test]
fn a_two_component_path_names_the_removed_title_without_inventing_an_artist() {
    assert_eq!(
        removal_activity(&inventory_removal("Album/03 Title.opus")),
        "Title"
    );
}

#[test]
fn a_bare_file_name_outside_the_writer_shape_stays_unchanged() {
    assert_eq!(
        removal_activity(&inventory_removal("03 Immortal.opus")),
        "03 Immortal.opus"
    );
}

#[test]
fn an_orphan_path_outside_the_naming_scheme_stays_unchanged() {
    let removal = ManagedRemoval::Orphan(ManagedDeviceFile {
        relative_path: "Loose/stray.opus".into(),
        size_bytes: 1,
    });

    assert_eq!(removal_activity(&removal), "Loose/stray.opus");
}

#[test]
fn a_lossy_component_stays_lossy_when_the_removed_activity_is_reconstructed() {
    let path = device_track_path(
        &DevicePathMetadata {
            album_artist: "AC/DC".into(),
            artist: "AC/DC".into(),
            album: "Power Up".into(),
            track_number: Some(1),
            title: "Shot in the Dark".into(),
            source_path: PathBuf::from("/music/Shot in the Dark.flac"),
        },
        Some("opus"),
        1,
    );

    assert_eq!(
        removal_activity(&inventory_removal(&path)),
        "Shot in the Dark — AC_DC"
    );
}

#[test]
fn a_playlist_removal_uses_the_source_name_the_user_selected() {
    let record = DevicePlaylistRecord {
        device_serial: "serial-1".into(),
        source: SelectionSource::Playlist(7),
        source_name: "Road Trip".into(),
        device_path: "Playlists/road-trip.m3u8".into(),
        last_synced_at: None,
    };

    assert_eq!(playlist_removal_activity(&record), "Road Trip");
}

#[test]
fn an_unnamed_playlist_removal_falls_back_to_its_device_path() {
    let record = DevicePlaylistRecord {
        device_serial: "serial-1".into(),
        source: SelectionSource::Playlist(7),
        source_name: String::new(),
        device_path: "Playlists/road-trip.m3u8".into(),
        last_synced_at: None,
    };

    assert_eq!(
        playlist_removal_activity(&record),
        "Playlists/road-trip.m3u8"
    );
}
