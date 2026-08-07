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
