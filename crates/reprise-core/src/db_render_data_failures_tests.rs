use super::*;
use crate::db::{
    pending_render_data_tracks, pending_segment_render_data_files, set_track_render_data,
    track_source_fingerprint,
};
use crate::spectrogram::TrackSpectrogram;
use crate::waveform::TrackRenderData;

fn database() -> Db {
    let db = Db::open_in_memory().unwrap();
    db.conn()
        .execute_batch(
            "INSERT INTO tracks (id, path, title, added_at, file_mtime, file_size, device, inode)
             VALUES (1, '/broken.flac', '', 0, 11, 22, 33, 44),
                    (2, '/fine.flac', '', 0, 11, 22, 33, 45),
                    (3, '/no-stat.flac', '', 0, 11, 22, NULL, NULL);
             INSERT INTO tracks (id, path, title, added_at, file_mtime, file_size, device, inode,
                                 segment_index, segment_start_ms, segment_end_ms)
             VALUES (10, '/live.flac', '', 0, 11, 22, 33, 90, 1, 0, 3000),
                    (11, '/live.flac', '', 0, 11, 22, 33, 90, 2, 3000, 8000);",
        )
        .unwrap();
    db
}

/// Records a failure of a decode that began on the track as it is now.
fn fail(db: &Db, track_id: i64, reason: &str) {
    let source = track_source_fingerprint(db, track_id).unwrap().unwrap();
    let outcome = record_render_data_failure(db, track_id, source, None, reason).unwrap();
    assert_eq!(outcome, SpectrogramStoreOutcome::Stored);
}

fn pending_ids(db: &Db) -> Vec<i64> {
    let whole = pending_render_data_tracks(db).unwrap();
    let segments = pending_segment_render_data_files(db).unwrap();
    whole
        .into_iter()
        .map(|track| track.track_id)
        .chain(
            segments
                .into_iter()
                .flat_map(|file| file.tracks)
                .map(|track| track.track_id),
        )
        .collect()
}

#[test]
fn a_recorded_failure_takes_the_track_out_of_the_pending_work() {
    let db = database();

    fail(&db, 1, "rate changed mid-stream");
    fail(&db, 11, "the stream ended before the track");

    assert!(render_data_failed(&db, 1).unwrap());
    assert!(render_data_failed(&db, 11).unwrap());
    assert!(!render_data_failed(&db, 2).unwrap());
    assert_eq!(pending_ids(&db), [2, 3, 10]);
}

#[test]
fn a_track_without_a_stat_identity_can_be_marked() {
    let db = database();

    fail(&db, 3, "decode error");

    assert!(render_data_failed(&db, 3).unwrap());
    assert!(!pending_ids(&db).contains(&3));
}

#[test]
fn a_changed_file_is_pending_again() {
    let db = database();
    fail(&db, 1, "decode error");

    db.conn()
        .execute("UPDATE tracks SET file_mtime = 12 WHERE id = 1", [])
        .unwrap();

    assert!(!render_data_failed(&db, 1).unwrap());
    assert!(pending_ids(&db).contains(&1));
}

#[test]
fn a_re_cut_track_is_pending_again() {
    let db = database();
    fail(&db, 11, "the stream ended before the track");

    db.conn()
        .execute(
            "UPDATE tracks SET segment_start_ms = 2500 WHERE id = 11",
            [],
        )
        .unwrap();

    assert!(!render_data_failed(&db, 11).unwrap());
    assert!(pending_ids(&db).contains(&11));
}

#[test]
fn a_failure_from_another_analysis_format_does_not_hold() {
    let db = database();
    fail(&db, 1, "decode error");

    db.conn()
        .execute(
            "UPDATE render_data_failures SET format_version = ?1 WHERE track_id = 1",
            [SPECTROGRAM_FORMAT_VERSION + 1],
        )
        .unwrap();

    assert!(!render_data_failed(&db, 1).unwrap());
    assert!(pending_ids(&db).contains(&1));
}

#[test]
fn clearing_or_a_successful_store_forgets_the_failure() {
    let db = database();
    fail(&db, 1, "decode error");
    fail(&db, 2, "decode error");

    clear_render_data_failure(&db, 1).unwrap();
    let source = track_source_fingerprint(&db, 2).unwrap().unwrap();
    set_track_render_data(
        &db,
        2,
        source,
        &TrackRenderData {
            waveform_peaks: vec![1],
            spectrogram: TrackSpectrogram::empty(),
            loudness: None,
            decoded_end_ms: None,
        },
    )
    .unwrap();

    assert!(!render_data_failed(&db, 1).unwrap());
    assert!(!render_data_failed(&db, 2).unwrap());
    let rows: i64 = db
        .conn()
        .query_row("SELECT count(*) FROM render_data_failures", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(rows, 0);
}

#[test]
fn recording_again_keeps_one_row_with_the_latest_reason() {
    let db = database();

    fail(&db, 1, "first");
    fail(&db, 1, "second");
    let gone = record_render_data_failure(
        &db,
        999,
        TrackSourceFingerprint {
            mtime_seconds: 11,
            size_bytes: 22,
            device: None,
            inode: None,
        },
        None,
        "a track that is gone",
    )
    .unwrap();
    assert_eq!(gone, SpectrogramStoreOutcome::SourceChanged);

    let reasons: Vec<String> = db
        .conn()
        .prepare("SELECT reason FROM render_data_failures")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(reasons, ["second"]);
}

#[test]
fn a_failure_recorded_after_the_file_changed_leaves_the_track_pending() {
    let db = database();
    let started = track_source_fingerprint(&db, 1).unwrap().unwrap();
    // A rescan saw the file rewritten while the decode ran, and the decode
    // then failed on the file as it was.
    db.conn()
        .execute("UPDATE tracks SET file_mtime = 12 WHERE id = 1", [])
        .unwrap();

    let outcome =
        record_render_data_failure(&db, 1, started, None, "truncated while it was rewritten")
            .unwrap();

    assert_eq!(outcome, SpectrogramStoreOutcome::SourceChanged);
    assert!(!render_data_failed(&db, 1).unwrap());
    assert!(pending_ids(&db).contains(&1));
}

#[test]
fn a_failure_recorded_after_the_track_was_re_cut_leaves_it_pending() {
    let db = database();
    let started = track_source_fingerprint(&db, 11).unwrap().unwrap();
    let cut = SegmentBounds {
        start_ms: 3_000,
        end_ms: 8_000,
        last_in_file: true,
    };
    db.conn()
        .execute(
            "UPDATE tracks SET segment_start_ms = 2500 WHERE id = 11",
            [],
        )
        .unwrap();

    let outcome = record_render_data_failure(&db, 11, started, Some(cut), "ended early").unwrap();

    assert_eq!(outcome, SpectrogramStoreOutcome::SourceChanged);
    assert!(!render_data_failed(&db, 11).unwrap());
    assert!(pending_ids(&db).contains(&11));
}
