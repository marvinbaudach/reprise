use std::path::PathBuf;

use super::{
    plan_mirror, DesktopAnalysis, DeviceFileRecord, ManagedDeviceFile, MirrorInput,
    MirrorPlaylistSnapshot, MirrorTrack, Mp3Quality, SelectionSource, SyncTrack, TransferProfile,
};

const DEVICE_PATH: &str = "Album Artist/Album/01 Track 1.mp3";
const ANALYSIS_PATH: &str = "Album Artist/Album/01 Track 1.reprise-analysis";

fn wanted_track() -> SyncTrack {
    SyncTrack {
        id: 1,
        source_path: PathBuf::from("/music/one.mp3"),
        original_name: "one.mp3".into(),
        title: "Track 1".into(),
        artist: "Artist".into(),
        album: "Album".into(),
        album_artist: "Album Artist".into(),
        track_number: Some(1),
        duration_ms: 10_000,
        bitrate_kbps: Some(192),
        size_bytes: 240_000,
        source_mtime: 10,
    }
}

fn inventoried_input(managed_files_scanned: bool) -> MirrorInput {
    let track = wanted_track();
    MirrorInput {
        selected: vec![SelectionSource::Playlist(10)],
        playlists: vec![MirrorPlaylistSnapshot {
            source: SelectionSource::Playlist(10),
            name: "Road".into(),
            entries: vec![MirrorTrack::Available(track.clone())],
            stability_margin_track_ids: Vec::new(),
        }],
        profile: TransferProfile::Mp3(Mp3Quality::Kbps256),
        inventory: vec![DeviceFileRecord {
            device_serial: "phone".into(),
            track_id: track.id,
            source_path: track.source_path.to_string_lossy().into_owned(),
            source_size: track.size_bytes,
            source_mtime: track.source_mtime,
            device_path: DEVICE_PATH.into(),
            device_size: track.size_bytes,
            profile_fingerprint: "copy-original-v1".into(),
            pinned: false,
        }],
        playlist_inventory: Vec::new(),
        managed_files: Vec::new(),
        managed_files_scanned,
        desktop_analyses: Vec::new(),
    }
}

#[test]
fn mtp_52_authoritative_scan_recopies_inventoried_track_missing_from_phone() {
    let plan = plan_mirror(inventoried_input(true));

    assert_eq!(plan.copy.len(), 1);
    assert_eq!(plan.copy[0].track.id, 1);
    assert_eq!(plan.copy[0].device_path, DEVICE_PATH);
    assert_eq!(plan.transfer_bytes, 240_000);
    assert!(plan.replace.is_empty());
}

#[test]
fn mtp_52_unscanned_device_keeps_matching_inventory_guard() {
    let plan = plan_mirror(inventoried_input(false));

    assert!(plan.copy.is_empty());
    assert!(plan.replace.is_empty());
    assert_eq!(plan.transfer_bytes, 0);
}

#[test]
fn mtp_52_authoritative_scan_keeps_present_matching_track() {
    let mut input = inventoried_input(true);
    input.managed_files.push(ManagedDeviceFile {
        relative_path: DEVICE_PATH.into(),
        size_bytes: 240_000,
    });

    let plan = plan_mirror(input);

    assert!(plan.copy.is_empty());
    assert!(plan.replace.is_empty());
    assert_eq!(plan.transfer_bytes, 0);
}

#[test]
fn mtp_52_returning_track_rewrites_analysis_without_orphaning_it() {
    let mut input = inventoried_input(true);
    input.desktop_analyses.push(DesktopAnalysis {
        track_id: 1,
        size_bytes: 123,
    });
    input.managed_files.push(ManagedDeviceFile {
        relative_path: ANALYSIS_PATH.into(),
        size_bytes: 122,
    });

    let plan = plan_mirror(input);

    assert_eq!(plan.copy.len(), 1);
    assert_eq!(plan.analysis_writes.len(), 1);
    assert_eq!(plan.analysis_writes[0].track_id, 1);
    assert_eq!(plan.analysis_writes[0].device_path, ANALYSIS_PATH);
    assert_eq!(plan.analysis_writes[0].existing_size_bytes, Some(122));
    assert!(plan.remove.is_empty());
    assert_eq!(plan.transfer_bytes, 240_123);
}
