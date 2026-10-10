//! CUE-19: the length the analysis decoded becomes the last CUE track's duration.

use super::*;

fn source() -> TrackSourceFingerprint {
    TrackSourceFingerprint {
        mtime_seconds: 11,
        size_bytes: 22,
        device: Some(33),
        inode: Some(44),
    }
}

/// A two-track CUE file whose header says 14 s: the second track starts at 8 s
/// and so claims to last 6 s, while the file really holds 20 s.
fn album_with_understated_header() -> Db {
    let db = Db::open_in_memory().unwrap();
    db.conn()
        .execute_batch(
            "INSERT INTO tracks (id, path, title, added_at, file_mtime, file_size, device,
                                 inode, segment_index, segment_start_ms, segment_end_ms,
                                 duration_ms)
             VALUES (1, '/a.flac', 'S1', 0, 11, 22, 33, 44, 1, 0, 8000, 8000),
                    (2, '/a.flac', 'S2', 0, 11, 22, 33, 44, 2, 8000, 14000, 6000);",
        )
        .unwrap();
    db
}

fn data_ending_at(decoded_end_ms: Option<i64>) -> TrackRenderData {
    TrackRenderData {
        waveform_peaks: vec![4],
        spectrogram: TrackSpectrogram::empty(),
        loudness: None,
        decoded_end_ms,
    }
}

fn bounds(start_ms: i64, end_ms: i64, last_in_file: bool) -> SegmentBounds {
    SegmentBounds {
        start_ms,
        end_ms,
        last_in_file,
    }
}

fn duration_ms(db: &Db, track_id: i64) -> i64 {
    db.conn()
        .query_row(
            "SELECT duration_ms FROM tracks WHERE id = ?1",
            [track_id],
            |row| row.get(0),
        )
        .unwrap()
}

fn cut(db: &Db, track_id: i64) -> (i64, i64) {
    db.conn()
        .query_row(
            "SELECT segment_start_ms, segment_end_ms FROM tracks WHERE id = ?1",
            [track_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

#[test]
fn cue_19_the_decoded_end_becomes_the_last_tracks_duration() {
    let db = album_with_understated_header();

    let outcome = set_segment_render_data(
        &db,
        2,
        source(),
        bounds(8_000, 14_000, true),
        &data_ending_at(Some(20_000)),
    )
    .unwrap();

    assert_eq!(outcome, SpectrogramStoreOutcome::Stored);
    assert_eq!(duration_ms(&db, 2), 12_000);
}

#[test]
fn cue_19_the_recorded_cut_and_the_stored_analysis_stay_as_they_were() {
    // The cut is what the scan, the move detection and the stale-analysis
    // trigger know the track by; only its duration is learned.
    let db = album_with_understated_header();

    set_segment_render_data(
        &db,
        2,
        source(),
        bounds(8_000, 14_000, true),
        &data_ending_at(Some(20_000)),
    )
    .unwrap();

    assert_eq!(cut(&db, 2), (8_000, 14_000));
    assert_eq!(get_waveform_peaks(&db, 2).unwrap(), Some(vec![4]));
}

#[test]
fn cue_19_a_shorter_decoded_end_than_the_header_claims_is_stored_too() {
    let db = album_with_understated_header();

    set_segment_render_data(
        &db,
        2,
        source(),
        bounds(8_000, 14_000, true),
        &data_ending_at(Some(11_500)),
    )
    .unwrap();

    assert_eq!(duration_ms(&db, 2), 3_500);
}

#[test]
fn cue_19_a_track_that_is_not_the_last_keeps_its_duration() {
    let db = album_with_understated_header();

    set_segment_render_data(
        &db,
        1,
        source(),
        bounds(0, 8_000, false),
        &data_ending_at(Some(20_000)),
    )
    .unwrap();

    assert_eq!(duration_ms(&db, 1), 8_000);
}

#[test]
fn cue_19_a_track_whose_successor_was_only_removed_keeps_its_duration() {
    // Its stretch ends where the removed track starts, so the end of the file
    // says nothing about it (see `track_is_last_in_file`).
    let db = album_with_understated_header();
    db.conn()
        .execute(
            "INSERT INTO library_exclusions (path, device, inode, segment_index, excluded_at)
             VALUES ('/a.flac', 33, 44, 3, 0)",
            [],
        )
        .unwrap();

    set_segment_render_data(
        &db,
        2,
        source(),
        bounds(8_000, 14_000, true),
        &data_ending_at(Some(20_000)),
    )
    .unwrap();

    assert_eq!(duration_ms(&db, 2), 6_000);
}

#[test]
fn cue_19_an_analysis_of_a_changed_file_teaches_nothing() {
    let db = album_with_understated_header();
    let changed = TrackSourceFingerprint {
        mtime_seconds: 99,
        ..source()
    };

    let outcome = set_segment_render_data(
        &db,
        2,
        changed,
        bounds(8_000, 14_000, true),
        &data_ending_at(Some(20_000)),
    )
    .unwrap();

    assert_eq!(outcome, SpectrogramStoreOutcome::SourceChanged);
    assert_eq!(duration_ms(&db, 2), 6_000);
}

#[test]
fn cue_19_a_decoded_end_before_the_tracks_start_teaches_nothing() {
    let db = album_with_understated_header();

    set_segment_render_data(
        &db,
        2,
        source(),
        bounds(8_000, 14_000, true),
        &data_ending_at(Some(8_000)),
    )
    .unwrap();

    assert_eq!(duration_ms(&db, 2), 6_000);
}

#[test]
fn cue_19_an_analysis_that_reports_no_end_leaves_the_duration_alone() {
    let db = album_with_understated_header();

    set_segment_render_data(
        &db,
        2,
        source(),
        bounds(8_000, 14_000, true),
        &data_ending_at(None),
    )
    .unwrap();

    assert_eq!(duration_ms(&db, 2), 6_000);
}
