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
fn transfer_profiles_are_exactly_opus_160_mp3_256_and_original() {
    assert_eq!(Mp3Quality::ALL, [Mp3Quality::Kbps256]);
    assert_eq!(TransferProfile::default(), TransferProfile::Opus160);
    assert_eq!(
        TransferProfile::ALL,
        [
            TransferProfile::Opus160,
            TransferProfile::Mp3(Mp3Quality::Kbps256),
            TransferProfile::Original,
        ]
    );
    assert_eq!(Mp3Quality::try_from(256), Ok(Mp3Quality::Kbps256));
    assert!(Mp3Quality::try_from(0).is_err());
    assert!(Mp3Quality::try_from(160).is_err());
    assert!(Mp3Quality::try_from(320).is_err());
    assert_eq!(Mp3Quality::Kbps256.kbps(), 256);
    assert_eq!(
        TransferProfile::from_storage_value("opus_160"),
        Some(TransferProfile::Opus160)
    );
    assert_eq!(
        TransferProfile::from_storage_value("mp3_256"),
        Some(TransferProfile::Mp3(Mp3Quality::Kbps256))
    );
    assert_eq!(
        TransferProfile::from_storage_value("original"),
        Some(TransferProfile::Original)
    );
    assert_eq!(TransferProfile::from_storage_value("mp3_320"), None);
    assert_eq!(TransferProfile::Opus160.storage_value(), "opus_160");
    assert_eq!(
        TransferProfile::Mp3(Mp3Quality::Kbps256).storage_value(),
        "mp3_256"
    );
    assert_eq!(TransferProfile::Original.storage_value(), "original");
}

#[test]
fn mtp_8_lossy_and_ambiguous_sources_are_never_transcoded_to_another_lossy_format() {
    let opus = TransferProfile::Opus160;
    let mp3 = TransferProfile::Mp3(Mp3Quality::Kbps256);

    assert_eq!(
        opus.action_for(&track(1, "/music/low.mp3", Some(96), 10_000, 120_000)),
        TransferAction::CopyOriginal
    );
    assert_eq!(
        opus.action_for(&track(2, "/music/high.mp3", Some(320), 10_000, 400_000)),
        TransferAction::CopyOriginal
    );
    assert_eq!(
        mp3.action_for(&track(3, "/music/source.opus", Some(160), 10_000, 200_000)),
        TransferAction::CopyOriginal
    );
    assert_eq!(
        opus.action_for(&track(4, "/music/source.m4a", None, 10_000, 400_000)),
        TransferAction::CopyOriginal,
        "an ambiguous MP4 audio container is copied conservatively"
    );
    assert_eq!(
        mp3.action_for(&track(5, "/music/source.unknown", None, 10_000, 400_000)),
        TransferAction::CopyOriginal,
        "unknown source encodings must never be assumed lossless"
    );
}

#[test]
fn only_known_lossless_sources_are_encoded_and_original_always_copies() {
    let flac = track(1, "/music/lossless.flac", None, 10_000, 1_000_000);

    assert_eq!(
        TransferProfile::Opus160.action_for(&flac),
        TransferAction::TranscodeOpus160
    );
    assert_eq!(
        TransferProfile::Mp3(Mp3Quality::Kbps256).action_for(&flac),
        TransferAction::TranscodeMp3(Mp3Quality::Kbps256)
    );
    assert_eq!(
        TransferProfile::Original.action_for(&flac),
        TransferAction::CopyOriginal
    );
}

#[test]
fn target_size_reserves_source_derived_metadata_and_mux_overhead_for_transcoding() {
    let profile = TransferProfile::Mp3(Mp3Quality::Kbps256);
    let copied = track(1, "/music/low.mp3", Some(192), 10_000, 240_000);
    let transcoded = track(2, "/music/lossless.flac", None, 10_000, 400_000);
    let unknown_duration = track(3, "/music/lossless.flac", None, 0, 444_000);

    assert_eq!(profile.estimated_target_bytes(&copied), 240_000);
    assert_eq!(profile.estimated_target_bytes(&transcoded), 785_536);
    assert_eq!(
        profile.estimated_target_bytes(&unknown_duration),
        u64::MAX,
        "an unknown duration cannot produce a bounded conservative estimate"
    );
    assert_eq!(
        TransferProfile::Opus160.estimated_target_bytes(&transcoded),
        665_536
    );
    assert_eq!(
        TransferProfile::Original.estimated_target_bytes(&transcoded),
        400_000
    );
}

#[test]
fn playlist_size_projection_uses_profile_bitrate_instead_of_reserving_the_whole_flac() {
    let flac = track(1, "/music/lossless.flac", Some(1_000), 240_000, 31_000_000);

    assert_eq!(
        TransferProfile::Opus160.estimated_target_bytes(&flac),
        5_865_536
    );
    assert_eq!(
        TransferProfile::Mp3(Mp3Quality::Kbps256).estimated_target_bytes(&flac),
        8_745_536
    );
    assert_eq!(
        TransferProfile::Original.estimated_target_bytes(&flac),
        31_000_000
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
    assert_eq!(projection.playlists[1].target_bytes, 440_000);
    assert_eq!(projection.unique_track_count, 3);
    assert_eq!(projection.target_bytes, 1_825_536);
}

#[test]
fn sync_query_exposes_source_bitrate_for_profile_planning() {
    let conn = crate::db::Db::open_in_memory().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("known.mp3");
    std::fs::write(&path, b"mp3").unwrap();
    conn.conn()
        .execute(
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
