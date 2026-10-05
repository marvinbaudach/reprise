//! A CUE track reads its segment through every track projection, and a
//! whole-file track reads none.

use super::*;
use crate::models::TrackSegment;

fn seed(db: &Db) {
    db.conn()
        .execute_batch(
            "INSERT INTO tracks (id, path, title, duration_ms, added_at, segment_index,
                                 segment_start_ms, segment_end_ms, cue_path)
             VALUES (1, '/m/live.flac', 'Opener', 90000, 1, 1, 0, 90000, '/m/live.cue'),
                    (2, '/m/live.flac', 'Second', 150000, 1, 2, 90000, 240000, NULL),
                    (3, '/m/plain.flac', 'Plain', 60000, 1, 0, NULL, NULL, NULL);",
        )
        .unwrap();
}

#[test]
fn the_row_reader_returns_the_segment_of_a_cue_track() {
    let db = Db::open_in_memory().unwrap();
    seed(&db);

    let mut tracks = query_present_tracks_by_ids(&db, &[1, 2, 3]).unwrap();
    tracks.sort_by_key(|track| track.id);
    let segments: Vec<Option<TrackSegment>> =
        tracks.into_iter().map(|track| track.segment).collect();

    assert_eq!(
        segments,
        [
            Some(TrackSegment {
                index: 1,
                start_ms: 0,
                end_ms: 90_000,
                cue_path: Some("/m/live.cue".into()),
            }),
            Some(TrackSegment {
                index: 2,
                start_ms: 90_000,
                end_ms: 240_000,
                cue_path: None,
            }),
            None,
        ]
    );
}

#[test]
fn the_playback_summary_carries_the_segment() {
    let db = Db::open_in_memory().unwrap();
    seed(&db);

    let second = query_track_summary(&db, 2).unwrap().unwrap();
    let plain = query_track_summary(&db, 3).unwrap().unwrap();

    assert_eq!(second.path, "/m/live.flac");
    assert_eq!(
        second.segment,
        Some(TrackSegment {
            index: 2,
            start_ms: 90_000,
            end_ms: 240_000,
            cue_path: None,
        })
    );
    assert_eq!(plain.segment, None);
}

#[test]
fn a_missing_cue_track_keeps_its_segment_in_the_missing_view() {
    let db = Db::open_in_memory().unwrap();
    seed(&db);
    db.conn()
        .execute(
            "UPDATE tracks SET missing_since = 5, missing_reason = 'deleted' WHERE id = 2",
            [],
        )
        .unwrap();

    let rows = query_missing_rows(&db, &MissingGroupKind::Deleted, 0, 10).unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].segment.as_ref().map(|segment| segment.index),
        Some(2)
    );
}
