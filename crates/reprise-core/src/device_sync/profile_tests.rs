use std::path::PathBuf;

use super::{
    project_playlist_sizes, Mp3Quality, PlaylistTracks, SelectionSource, SyncTrack, TransferAction,
    TransferProfile,
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
        original_name: path.rsplit('/').next().unwrap_or(path).to_string(),
        title: format!("Track {id}"),
        artist: "Artist".into(),
        album: "Album".into(),
        album_artist: "Album Artist".into(),
        track_number: Some(u32::try_from(id).unwrap_or_default()),
        duration_ms,
        bitrate_kbps,
        size_bytes,
        source_mtime: 1,
    }
}

#[test]
fn mp3_profile_defaults_to_256_and_only_accepts_supported_qualities() {
    assert_eq!(
        Mp3Quality::ALL,
        [
            Mp3Quality::Kbps128,
            Mp3Quality::Kbps192,
            Mp3Quality::Kbps256,
            Mp3Quality::Kbps320,
        ]
    );
    assert_eq!(
        TransferProfile::default(),
        TransferProfile::Mp3(Mp3Quality::Kbps256)
    );
    assert_eq!(Mp3Quality::try_from(192), Ok(Mp3Quality::Kbps192));
    assert!(Mp3Quality::try_from(0).is_err());
    assert!(Mp3Quality::try_from(160).is_err());
    assert_eq!(Mp3Quality::Kbps320.kbps(), 320);
    assert_eq!(
        TransferProfile::Mp3(Mp3Quality::Kbps256).fingerprint(),
        "mp3-cbr-256-v1"
    );
}

#[test]
fn mp3_at_or_below_the_profile_is_copied_and_everything_else_is_transcoded() {
    let profile = TransferProfile::Mp3(Mp3Quality::Kbps256);

    assert_eq!(
        profile.action_for(&track(1, "/music/low.mp3", Some(192), 10_000, 240_000)),
        TransferAction::CopyOriginal
    );
    assert_eq!(
        profile.action_for(&track(2, "/music/exact.MP3", Some(256), 10_000, 320_000)),
        TransferAction::CopyOriginal
    );
    assert_eq!(
        profile.action_for(&track(3, "/music/high.mp3", Some(320), 10_000, 400_000)),
        TransferAction::TranscodeMp3(Mp3Quality::Kbps256)
    );
    assert_eq!(
        profile.action_for(&track(4, "/music/unknown.mp3", None, 10_000, 400_000)),
        TransferAction::TranscodeMp3(Mp3Quality::Kbps256)
    );
    assert_eq!(
        profile.action_for(&track(5, "/music/invalid.mp3", Some(0), 10_000, 400_000)),
        TransferAction::TranscodeMp3(Mp3Quality::Kbps256)
    );
    assert_eq!(
        profile.action_for(&track(6, "/music/lossless.flac", None, 10_000, 1_000_000)),
        TransferAction::TranscodeMp3(Mp3Quality::Kbps256)
    );
}

#[test]
fn target_size_reserves_source_derived_metadata_and_mux_overhead_for_transcoding() {
    let profile = TransferProfile::Mp3(Mp3Quality::Kbps256);
    let copied = track(1, "/music/low.mp3", Some(192), 10_000, 240_000);
    let transcoded = track(2, "/music/high.mp3", Some(320), 10_000, 400_000);
    let unknown_duration = track(3, "/music/high.mp3", Some(320), 0, 444_000);

    assert_eq!(profile.estimated_target_bytes(&copied), 240_000);
    assert_eq!(profile.estimated_target_bytes(&transcoded), 785_536);
    assert_eq!(
        profile.estimated_target_bytes(&unknown_duration),
        u64::MAX,
        "an unknown duration cannot produce a bounded conservative estimate"
    );
}

#[test]
fn playlist_projection_preserves_entries_but_deduplicates_physical_tracks() {
    let low_mp3 = track(1, "/music/one.mp3", Some(192), 10_000, 240_000);
    let flac = track(2, "/music/two.flac", None, 10_000, 1_000_000);
    let high_mp3 = track(3, "/music/three.mp3", Some(320), 5_000, 200_000);
    let playlists = vec![
        PlaylistTracks {
            source: SelectionSource::Playlist(10),
            name: "Repeated".into(),
            tracks: vec![low_mp3.clone(), flac, low_mp3.clone()],
        },
        PlaylistTracks {
            source: SelectionSource::Smart(11),
            name: "Smart snapshot".into(),
            tracks: vec![low_mp3, high_mp3],
        },
    ];

    let projection = project_playlist_sizes(&playlists, TransferProfile::Mp3(Mp3Quality::Kbps256));

    assert_eq!(projection.playlists.len(), 2);
    assert_eq!(projection.playlists[0].entry_count, 3);
    assert_eq!(projection.playlists[0].unique_track_count, 2);
    assert_eq!(projection.playlists[0].target_bytes, 1_625_536);
    assert_eq!(projection.playlists[1].entry_count, 2);
    assert_eq!(projection.playlists[1].unique_track_count, 2);
    assert_eq!(projection.playlists[1].target_bytes, 665_536);
    assert_eq!(projection.unique_track_count, 3);
    assert_eq!(projection.target_bytes, 2_051_072);
}

#[test]
fn sync_query_exposes_source_bitrate_for_profile_planning() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("known.mp3");
    std::fs::write(&path, b"mp3").unwrap();
    conn.execute(
        "INSERT INTO tracks (
             id, path, title, artist, album, duration_ms, bitrate_kbps, added_at
         ) VALUES (1, ?1, 'Known', 'Artist', 'Album', 10000, 192, 0)",
        [path.to_string_lossy().as_ref()],
    )
    .unwrap();

    let tracks = crate::queries::query_sync_tracks(&conn, &[1]).unwrap();

    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].bitrate_kbps, Some(192));
}
