//! The rating and play count the desktop sends for a track a CUE sheet cut
//! from a synced file reach that track on the phone (CUE-17).

use crate::device_sync::track_metadata_list::{
    SegmentMetadataEntry, TrackMetadataEntry, TrackMetadataList, FILE_NAME,
};

use super::super::cue_tests::{scan, write_wav, THREE_TRACKS};

#[test]
fn cue_17_the_desktops_rating_and_play_count_of_a_cue_track_reach_that_track() {
    let dir = tempfile::tempdir().unwrap();
    write_wav(&dir.path().join("album.wav"), 30);
    std::fs::write(dir.path().join("album.cue"), THREE_TRACKS).unwrap();
    let list = TrackMetadataList::new(vec![TrackMetadataEntry {
        device_path: "album.wav".into(),
        rating: 1,
        play_count: 99,
    }])
    .with_segments(vec![SegmentMetadataEntry {
        device_path: "album.wav".into(),
        segment_start_ms: 10_000,
        rating: 4,
        play_count: 7,
    }]);
    std::fs::write(dir.path().join(FILE_NAME), list.encode().unwrap()).unwrap();
    let db = crate::db::Db::open_in_memory().unwrap();

    scan(&db, dir.path());

    let rows: Vec<(i64, i32, i64)> = db
        .conn()
        .prepare("SELECT segment_index, rating, play_count FROM tracks ORDER BY segment_index")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(rows, [(1, 0, 0), (2, 4, 7), (3, 0, 0)]);
}
