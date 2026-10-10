use super::*;

#[test]
fn track_metadata_list_round_trips_device_identity_rating_and_play_count() {
    let list = TrackMetadataList::new(vec![
        TrackMetadataEntry {
            device_path: "Artist/Album/01 Song.opus".into(),
            rating: 4,
            play_count: 27,
        },
        TrackMetadataEntry {
            device_path: "Artist/Album/02 Next.opus".into(),
            rating: 5,
            play_count: 3,
        },
    ]);

    let encoded = list.encode().unwrap();

    assert_eq!(&encoded[..8], b"RPT-LIST");
    assert_eq!(u16::from_le_bytes([encoded[8], encoded[9]]), FORMAT_VERSION);
    assert_eq!(TrackMetadataList::decode(&encoded).unwrap(), list);
    assert_eq!(
        list.entries[0].rating, 4,
        "ratings are not flattened to a heart"
    );
}

#[test]
fn track_metadata_list_rejects_a_recognisable_future_version() {
    let mut encoded = TrackMetadataList::new(Vec::new()).encode().unwrap();
    encoded[8..10].copy_from_slice(&9_u16.to_le_bytes());

    assert_eq!(
        TrackMetadataList::decode(&encoded),
        Err(TrackMetadataListError::UnsupportedVersion(9))
    );
}

#[test]
fn track_metadata_list_path_is_recognised_case_insensitively() {
    assert!(is_list_path(std::path::Path::new(
        "REPRISE-TRACK-METADATA.RPL"
    )));
    assert!(!is_list_path(std::path::Path::new("Song.opus")));
}

#[test]
fn cue_17_the_list_carries_each_cue_track_with_its_start() {
    let list = TrackMetadataList::new(vec![TrackMetadataEntry {
        device_path: "Artist/Album/01 Song.opus".into(),
        rating: 3,
        play_count: 2,
    }])
    .with_segments(vec![SegmentMetadataEntry {
        device_path: "Band/Album/album.opus".into(),
        segment_start_ms: 10_013,
        rating: 4,
        play_count: 7,
    }]);

    let decoded = TrackMetadataList::decode(&list.encode().unwrap()).unwrap();

    assert_eq!(decoded, list);
    assert_eq!(decoded.segments[0].segment_start_ms, 10_013);
}

#[test]
fn track_metadata_list_decode_rejects_every_truncation_without_panicking() {
    let encoded = TrackMetadataList::new(vec![TrackMetadataEntry {
        device_path: "Artist/Album/01 Song.opus".into(),
        rating: 4,
        play_count: 27,
    }])
    .with_segments(vec![SegmentMetadataEntry {
        device_path: "Artist/Album/Cut.flac".into(),
        segment_start_ms: 61_000,
        rating: 3,
        play_count: 2,
    }])
    .encode()
    .unwrap();

    for length in 0..encoded.len() {
        assert!(
            TrackMetadataList::decode(&encoded[..length]).is_err(),
            "a {length}-byte prefix of {} bytes must be an error",
            encoded.len()
        );
    }
}

#[test]
fn track_metadata_list_decode_rejects_garbage_after_a_valid_header() {
    let mut garbage = b"RPT-LIST".to_vec();
    garbage.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    garbage.extend_from_slice(&[0xFF; 64]);

    assert!(TrackMetadataList::decode(&garbage).is_err());
    assert!(TrackMetadataList::decode(&[0xFF; 64]).is_err());
}
