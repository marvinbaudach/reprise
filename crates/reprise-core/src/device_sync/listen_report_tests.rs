use super::*;

fn seeded_database() -> crate::db::Db {
    let db = crate::db::Db::open_in_memory().unwrap();
    for (id, source_path, device_path) in [
        (1, "/music/one.flac", "Artist/Album/01 One.opus"),
        (2, "/music/two.flac", "Artist/Album/02 Two.opus"),
    ] {
        db.conn()
            .execute(
                "INSERT INTO tracks
                 (id, path, title, artist, album, album_artist, genre, duration_ms,
                  rating, play_count, last_played_at, added_at)
                 VALUES (?1, ?2, ?3, 'Artist', 'Album', 'Artist', 'Rock', 200000,
                         3, 2, 50, 1)",
                rusqlite::params![id, source_path, format!("Song {id}")],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO device_files
                 (device_serial, track_id, source_path, source_size, source_mtime,
                  device_path, device_size, profile_fingerprint, pinned)
                 VALUES ('phone-a', ?1, ?2, 100, 1, ?3, 80, 'opus-v1', 0)",
                rusqlite::params![id, source_path, device_path],
            )
            .unwrap();
    }
    db
}

fn listen(sequence: u64, device_path: &str, played_at: i64) -> ListenEntry {
    ListenEntry {
        sequence,
        device_path: device_path.into(),
        segment_start_ms: None,
        played_at,
        ms_played: 150_000,
    }
}

fn rating(sequence: u64, device_path: &str, value: i32, rated_at: i64) -> RatingEntry {
    RatingEntry {
        sequence,
        device_path: device_path.into(),
        segment_start_ms: None,
        rating: value,
        rated_at,
    }
}

fn sample_report() -> ListenReport {
    ListenReport::new(
        vec![ListenEntry {
            sequence: 7,
            device_path: "Artist/Album/01 Song.opus".into(),
            segment_start_ms: None,
            played_at: 1_754_600_001,
            ms_played: 183_421,
        }],
        vec![RatingEntry {
            sequence: u64::MAX,
            device_path: "Artist/Album/02 Next.opus".into(),
            segment_start_ms: Some(40_013),
            rating: 5,
            rated_at: 1_754_600_002,
        }],
    )
}

#[test]
fn report_round_trips_both_counted_sections_and_full_width_sequences() {
    let report = sample_report();

    let encoded = report.encode().unwrap();

    assert_eq!(&encoded[..8], b"RPT-BACK");
    assert_eq!(u16::from_le_bytes([encoded[8], encoded[9]]), FORMAT_VERSION);
    assert_eq!(ListenReport::decode(&encoded).unwrap(), report);
}

#[test]
fn acknowledgement_round_trips_the_full_width_high_water_mark() {
    let acknowledgement = ListenReportAcknowledgement::new(u64::MAX);

    let encoded = acknowledgement.encode();

    assert_eq!(
        ListenReportAcknowledgement::decode(&encoded).unwrap(),
        acknowledgement
    );
}

#[test]
fn acknowledgement_decode_rejects_wrong_magic_version_and_truncation() {
    let encoded = ListenReportAcknowledgement::new(41).encode();
    let mut wrong_magic = encoded.clone();
    wrong_magic[0] ^= 0xff;
    let mut future = encoded.clone();
    future[8..10].copy_from_slice(&9_u16.to_le_bytes());

    assert_eq!(
        ListenReportAcknowledgement::decode(&wrong_magic),
        Err(ListenReportError::InvalidMagic)
    );
    assert_eq!(
        ListenReportAcknowledgement::decode(&future),
        Err(ListenReportError::UnsupportedVersion(9))
    );
    assert_eq!(
        ListenReportAcknowledgement::decode(&encoded[..encoded.len() - 1]),
        Err(ListenReportError::UnexpectedEnd)
    );
}

#[test]
fn report_decode_rejects_wrong_magic_as_an_ordinary_error() {
    let mut encoded = sample_report().encode().unwrap();
    encoded[0] ^= 0xff;

    assert_eq!(
        ListenReport::decode(&encoded),
        Err(ListenReportError::InvalidMagic)
    );
}

#[test]
fn report_decode_rejects_an_unknown_version_as_an_ordinary_error() {
    let mut encoded = sample_report().encode().unwrap();
    encoded[8..10].copy_from_slice(&9_u16.to_le_bytes());

    assert_eq!(
        ListenReport::decode(&encoded),
        Err(ListenReportError::UnsupportedVersion(9))
    );
}

#[test]
fn report_decode_rejects_a_truncated_body_as_an_ordinary_error() {
    let mut encoded = sample_report().encode().unwrap();
    encoded.pop();

    assert_eq!(
        ListenReport::decode(&encoded),
        Err(ListenReportError::UnexpectedEnd)
    );
}

#[test]
fn report_decode_rejects_a_declared_path_larger_than_the_buffer() {
    let mut encoded = sample_report().encode().unwrap();
    let first_path_length = 10 + 4 + 8;
    encoded[first_path_length..first_path_length + 4].copy_from_slice(&u32::MAX.to_le_bytes());

    assert_eq!(
        ListenReport::decode(&encoded),
        Err(ListenReportError::UnexpectedEnd)
    );
}

#[test]
fn applying_a_listen_uses_the_local_play_and_history_mutations_together() {
    let db = seeded_database();
    let report = ListenReport::new(
        vec![listen(1, "Artist/Album/01 One.opus", 1_754_600_100)],
        Vec::new(),
    );

    let summary = apply_listen_report(&db, "phone-a", &report).unwrap();

    assert_eq!(
        summary,
        ListenReportApplySummary {
            listens_applied: 1,
            ratings_applied: 0,
            ratings_ignored: 0,
            unresolved: 0,
            unresolved_paths: Vec::new(),
            acknowledged_sequence: Some(1),
        }
    );
    assert_eq!(
        db.conn()
            .query_row(
                "SELECT play_count, last_played_at FROM tracks WHERE id = 1",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .unwrap(),
        (3, 1_754_600_100)
    );
    assert_eq!(
        db.conn()
            .query_row(
                "SELECT track_id, played_at, ms_played, title, path FROM listen_events",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .unwrap(),
        (
            1,
            1_754_600_100,
            150_000,
            "Song 1".into(),
            "/music/one.flac".into()
        )
    );
}

#[test]
fn returned_listens_keep_the_newest_last_played_at_without_losing_history() {
    let db = seeded_database();
    db.conn()
        .execute("UPDATE tracks SET last_played_at = NULL WHERE id = 2", [])
        .unwrap();
    let older = ListenReport::new(vec![listen(1, "Artist/Album/01 One.opus", 40)], Vec::new());

    apply_listen_report(&db, "phone-a", &older).unwrap();

    assert_eq!(
        db.conn()
            .query_row(
                "SELECT play_count, last_played_at FROM tracks WHERE id = 1",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?)),
            )
            .unwrap(),
        (3, Some(50)),
        "an older returned listen still counts without back-dating the track"
    );
    assert_eq!(
        db.conn()
            .query_row("SELECT track_id, played_at FROM listen_events", [], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            },)
            .unwrap(),
        (1, 40),
        "the older listen must remain in the event history"
    );

    let later = ListenReport::new(
        vec![
            listen(2, "Artist/Album/01 One.opus", 70),
            listen(3, "Artist/Album/01 One.opus", 60),
            listen(4, "Artist/Album/02 Two.opus", 30),
        ],
        Vec::new(),
    );
    apply_listen_report(&db, "phone-a", &later).unwrap();

    assert_eq!(
        db.conn()
            .prepare("SELECT id, play_count, last_played_at FROM tracks ORDER BY id")
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        vec![(1, 5, Some(70)), (2, 3, Some(30))],
        "the newest moment wins regardless of report order, and NULL accepts a moment"
    );
    assert_eq!(
        db.conn()
            .prepare("SELECT track_id, played_at FROM listen_events ORDER BY played_at")
            .unwrap()
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        vec![(2, 30), (1, 40), (1, 60), (1, 70)]
    );
}

#[test]
fn applying_the_same_sequence_twice_is_a_no_op_even_without_a_similar_event() {
    let db = seeded_database();
    let first = ListenReport::new(
        vec![listen(7, "Artist/Album/01 One.opus", 1_754_600_100)],
        Vec::new(),
    );
    apply_listen_report(&db, "phone-a", &first).unwrap();
    db.conn().execute("DELETE FROM listen_events", []).unwrap();
    let replay = ListenReport::new(
        vec![listen(7, "Artist/Album/01 One.opus", 1_754_699_999)],
        Vec::new(),
    );

    let summary = apply_listen_report(&db, "phone-a", &replay).unwrap();

    assert_eq!(summary.listens_applied, 0);
    assert_eq!(summary.acknowledged_sequence, Some(7));
    assert_eq!(
        db.conn()
            .query_row("SELECT play_count FROM tracks WHERE id = 1", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        3
    );
    assert_eq!(
        db.conn()
            .query_row("SELECT COUNT(*) FROM listen_events", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        0,
        "idempotency must come from the acknowledged sequence, not row similarity"
    );
}

#[test]
fn only_a_strictly_newer_rating_timestamp_wins_and_null_loses() {
    let db = seeded_database();
    db.conn()
        .execute("UPDATE tracks SET rated_at = 100 WHERE id = 1", [])
        .unwrap();
    db.conn()
        .execute("UPDATE tracks SET rated_at = NULL WHERE id = 2", [])
        .unwrap();
    let report = ListenReport::new(
        Vec::new(),
        vec![
            rating(1, "Artist/Album/01 One.opus", 5, 99),
            rating(2, "Artist/Album/01 One.opus", 4, 100),
            rating(3, "Artist/Album/01 One.opus", 2, 101),
            rating(4, "Artist/Album/02 Two.opus", 5, 1),
        ],
    );

    let summary = apply_listen_report(&db, "phone-a", &report).unwrap();

    assert_eq!(summary.ratings_applied, 2);
    assert_eq!(summary.ratings_ignored, 2);
    assert_eq!(
        db.conn()
            .prepare("SELECT rating, rated_at FROM tracks ORDER BY id")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, i32>(0)?, row.get::<_, Option<i64>>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        vec![(2, Some(101)), (5, Some(1))]
    );
}

#[test]
fn an_unresolved_path_is_counted_and_acknowledged_then_stays_applied() {
    let db = seeded_database();
    let report = ListenReport::new(
        vec![listen(11, "Deleted/Last Week.opus", 1_754_600_100)],
        Vec::new(),
    );

    let first = apply_listen_report(&db, "phone-a", &report).unwrap();
    db.conn()
        .execute(
            "UPDATE device_files SET device_path = 'Deleted/Last Week.opus'
              WHERE device_serial = 'phone-a' AND track_id = 1",
            [],
        )
        .unwrap();
    let second = apply_listen_report(&db, "phone-a", &report).unwrap();

    assert_eq!(first.unresolved, 1);
    assert_eq!(first.unresolved_paths, ["Deleted/Last Week.opus"]);
    assert_eq!(first.acknowledged_sequence, Some(11));
    assert_eq!(second.listens_applied, 0);
    assert_eq!(
        db.conn()
            .query_row("SELECT play_count FROM tracks WHERE id = 1", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        2,
        "a later path match cannot resurrect an already acknowledged action"
    );
}

#[test]
fn acknowledgements_are_scoped_per_device_and_keep_u64_max() {
    let db = seeded_database();
    db.conn()
        .execute(
            "INSERT INTO device_files
             (device_serial, track_id, source_path, source_size, source_mtime,
              device_path, device_size, profile_fingerprint, pinned)
             SELECT 'phone-b', track_id, source_path, source_size, source_mtime,
                    device_path, device_size, profile_fingerprint, pinned
               FROM device_files WHERE device_serial = 'phone-a'",
            [],
        )
        .unwrap();
    let report = ListenReport::new(
        vec![listen(u64::MAX, "Artist/Album/01 One.opus", 1_754_600_100)],
        Vec::new(),
    );

    apply_listen_report(&db, "phone-a", &report).unwrap();
    apply_listen_report(&db, "phone-b", &report).unwrap();
    let replay = apply_listen_report(&db, "phone-a", &report).unwrap();

    assert_eq!(replay.listens_applied, 0);
    assert_eq!(replay.acknowledged_sequence, Some(u64::MAX));
    assert_eq!(
        db.conn()
            .query_row("SELECT play_count FROM tracks WHERE id = 1", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        4,
        "the same sequence from two devices is two distinct actions"
    );
}

#[test]
fn a_history_insert_failure_rolls_back_the_play_and_acknowledgement() {
    let db = seeded_database();
    db.conn()
        .execute_batch(
            "CREATE TRIGGER reject_phone_history BEFORE INSERT ON listen_events
             BEGIN SELECT RAISE(ABORT, 'injected history failure'); END;",
        )
        .unwrap();
    let report = ListenReport::new(
        vec![listen(1, "Artist/Album/01 One.opus", 1_754_600_100)],
        Vec::new(),
    );

    assert!(apply_listen_report(&db, "phone-a", &report).is_err());

    assert_eq!(
        db.conn()
            .query_row("SELECT play_count FROM tracks WHERE id = 1", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        2
    );
    assert_eq!(
        db.conn()
            .query_row(
                "SELECT COUNT(*) FROM device_listen_report_state",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
}

#[test]
fn cue_17_a_phone_listen_without_a_start_of_a_file_cut_into_tracks_is_counted_unresolved() {
    let db = crate::db::Db::open_in_memory().unwrap();
    for (id, index) in [(5, 2), (6, 1)] {
        db.conn()
            .execute(
                "INSERT INTO tracks (id, path, title, added_at, segment_index, play_count, rating)
                 VALUES (?1, '/music/live.flac', ?2, 1, ?3, 0, 0)",
                rusqlite::params![id, format!("Part {index}"), index],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO device_files
                 (device_serial, track_id, source_path, source_size, source_mtime,
                  device_path, device_size, profile_fingerprint, pinned)
                 VALUES ('phone-a', ?1, '/music/live.flac', 100, 1, 'Live/live.opus', 80,
                         'opus-v1', 0)",
                [id],
            )
            .unwrap();
    }
    let report = ListenReport::new(
        vec![listen(3, "Live/live.opus", 1_754_600_100)],
        vec![rating(4, "Live/live.opus", 5, 1_754_600_200)],
    );

    let summary = apply_listen_report(&db, "phone-a", &report).unwrap();

    assert_eq!((summary.listens_applied, summary.unresolved), (0, 2));
    assert_eq!(summary.acknowledged_sequence, Some(4));
    let touched: i64 = db
        .conn()
        .query_row(
            "SELECT count(*) FROM tracks WHERE play_count > 0 OR rating > 0",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        touched, 0,
        "a listen of the whole file is no track's listen"
    );
}

/// A CUE file of three tracks, synced as one device file; only `rows` have a
/// device inventory row of their own.
fn cue_album_database(rows: &[i64]) -> crate::db::Db {
    let db = crate::db::Db::open_in_memory().unwrap();
    for (id, index, start_ms) in [(5, 1, 0), (6, 2, 10_013), (7, 3, 20_000)] {
        db.conn()
            .execute(
                "INSERT INTO tracks (id, path, title, added_at, segment_index, segment_start_ms,
                                     segment_end_ms, play_count, rating)
                 VALUES (?1, '/music/live.flac', ?2, 1, ?3, ?4, ?4 + 9000, 0, 0)",
                rusqlite::params![id, format!("Part {index}"), index, start_ms],
            )
            .unwrap();
    }
    for id in rows {
        db.conn()
            .execute(
                "INSERT INTO device_files
                 (device_serial, track_id, source_path, source_size, source_mtime,
                  device_path, device_size, profile_fingerprint, pinned)
                 VALUES ('phone-a', ?1, '/music/live.flac', 100, 1, 'Live/live.opus', 80,
                         'opus-v1', 0)",
                [id],
            )
            .unwrap();
    }
    db
}

fn counts(db: &crate::db::Db) -> Vec<(i64, i64, i32)> {
    db.conn()
        .prepare("SELECT id, play_count, rating FROM tracks ORDER BY id")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn cue_17_a_phone_listen_and_rating_of_cue_tracks_count_for_those_tracks() {
    let db = cue_album_database(&[5, 6, 7]);
    let report = ListenReport::new(
        vec![ListenEntry {
            segment_start_ms: Some(10_013),
            ..listen(3, "Live/live.opus", 1_754_600_100)
        }],
        vec![RatingEntry {
            segment_start_ms: Some(20_000),
            ..rating(4, "Live/live.opus", 5, 1_754_600_200)
        }],
    );
    let received = ListenReport::decode(&report.encode().unwrap()).unwrap();

    let summary = apply_listen_report(&db, "phone-a", &received).unwrap();

    assert_eq!(received, report);
    assert_eq!(
        (
            summary.listens_applied,
            summary.ratings_applied,
            summary.unresolved
        ),
        (1, 1, 0)
    );
    assert_eq!(counts(&db), [(5, 0, 0), (6, 1, 0), (7, 0, 5)]);
}

#[test]
fn cue_17_a_cue_track_without_a_row_of_its_own_is_found_through_its_file() {
    let db = cue_album_database(&[5]);
    let report = ListenReport::new(
        vec![ListenEntry {
            segment_start_ms: Some(20_000),
            ..listen(3, "Live/live.opus", 1_754_600_100)
        }],
        Vec::new(),
    );

    let summary = apply_listen_report(&db, "phone-a", &report).unwrap();

    assert_eq!(summary.listens_applied, 1);
    assert_eq!(counts(&db), [(5, 0, 0), (6, 0, 0), (7, 1, 0)]);
}

#[test]
fn cue_17_a_start_no_track_of_the_file_has_is_counted_unresolved() {
    let db = cue_album_database(&[5, 6, 7]);
    let report = ListenReport::new(
        vec![ListenEntry {
            segment_start_ms: Some(15_000),
            ..listen(3, "Live/live.opus", 1_754_600_100)
        }],
        Vec::new(),
    );

    let summary = apply_listen_report(&db, "phone-a", &report).unwrap();

    assert_eq!((summary.listens_applied, summary.unresolved), (0, 1));
    assert_eq!(summary.unresolved_paths, ["Live/live.opus"]);
    assert_eq!(summary.acknowledged_sequence, Some(3));
}

#[test]
fn cue_17_a_report_in_the_first_format_is_refused() {
    let mut encoded = sample_report().encode().unwrap();
    encoded[8..10].copy_from_slice(&1_u16.to_le_bytes());

    assert_eq!(
        ListenReport::decode(&encoded),
        Err(ListenReportError::UnsupportedVersion(1))
    );
}
