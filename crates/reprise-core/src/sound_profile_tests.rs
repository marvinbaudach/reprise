use super::*;

fn dimension(value: f64, confidence: f64) -> ProfileDimension {
    ProfileDimension::new(value, confidence).unwrap()
}

#[test]
fn analysis_versions_current_matches_the_build_constants() {
    let versions = AnalysisVersions::current();
    assert_eq!(
        versions.extractor(),
        crate::audio_analysis::CURRENT_EXTRACTOR_VERSION
    );
    assert_eq!(versions.profile(), CURRENT_PROFILE_VERSION);
    // Equivalent to spelling out the explicit constructor.
    assert_eq!(
        versions,
        AnalysisVersions::new(
            crate::audio_analysis::CURRENT_EXTRACTOR_VERSION,
            CURRENT_PROFILE_VERSION
        )
        .unwrap()
    );
}

fn ready_analysis() -> ReadyAnalysis {
    ReadyAnalysis::new(
        SourceFingerprint::new(20, 30).unwrap(),
        AnalysisVersions::new(2, 3).unwrap(),
        40,
        AudioEvidence::new(
            0.4,
            0.6,
            2_000.0,
            8_000.0,
            0.2,
            3.0,
            Some(TempoEstimate::new(120.0, 0.8).unwrap()),
        )
        .unwrap(),
        SoundProfile::new(
            dimension(0.3, 0.9),
            dimension(0.4, 0.8),
            dimension(0.5, 0.7),
            dimension(0.6, 0.6),
        ),
    )
    .unwrap()
}

#[test]
fn ac_2_normalized_value_rejects_non_finite_and_out_of_range_inputs() {
    assert_eq!(Normalized::new(0.0).unwrap().get(), 0.0);
    assert_eq!(Normalized::new(1.0).unwrap().get(), 1.0);
    for invalid in [-0.01, 1.01, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(Normalized::new(invalid).is_err(), "accepted {invalid}");
    }
    assert!(SourceFingerprint::new(-1, 0).is_err());
    assert!(SourceFingerprint::new(0, -1).is_err());
    assert!(AnalysisVersions::new(0, 1).is_err());
    assert!(AnalysisVersions::new(1, 0).is_err());
    assert!(AudioEvidence::new(0.0, 0.0, f64::NAN, 0.0, 0.0, 0.0, None).is_err());
}

#[test]
fn ready_analysis_round_trips_every_versioned_value() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks
           (id, path, title, artist, added_at, file_mtime, file_size)
         VALUES (1, '/fixture.flac', 'Fixture', 'Artist', 1, 20, 30)",
        [],
    )
    .unwrap();
    let expected = ready_analysis();

    save_ready_analysis(&conn, 1, &expected).unwrap();

    assert_eq!(
        load_analysis(&conn, 1).unwrap(),
        Some(TrackAnalysis::Ready(expected))
    );
}

#[test]
fn analysis_state_uses_source_content_not_path_and_excludes_missing_tracks() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks
           (id, path, title, artist, added_at, file_mtime, file_size)
         VALUES (1, '/old.flac', 'Fixture', 'Artist', 1, 20, 30)",
        [],
    )
    .unwrap();
    let versions = AnalysisVersions::new(2, 3).unwrap();
    assert_eq!(
        analysis_state(&conn, 1, versions).unwrap(),
        AnalysisState::Pending
    );

    save_ready_analysis(&conn, 1, &ready_analysis()).unwrap();
    assert_eq!(
        analysis_state(&conn, 1, versions).unwrap(),
        AnalysisState::Ready
    );

    assert_eq!(
        analysis_state(&conn, 1, AnalysisVersions::new(2, 4).unwrap()).unwrap(),
        AnalysisState::Stale
    );

    conn.execute("UPDATE tracks SET path = '/moved.flac' WHERE id = 1", [])
        .unwrap();
    assert_eq!(
        analysis_state(&conn, 1, versions).unwrap(),
        AnalysisState::Ready
    );

    conn.execute("UPDATE tracks SET file_mtime = 21 WHERE id = 1", [])
        .unwrap();
    assert_eq!(
        analysis_state(&conn, 1, versions).unwrap(),
        AnalysisState::Stale
    );

    conn.execute(
        "UPDATE tracks SET missing_since = 22, missing_reason = 'unknown' WHERE id = 1",
        [],
    )
    .unwrap();
    assert_eq!(
        analysis_state(&conn, 1, versions).unwrap(),
        AnalysisState::Ineligible
    );
}

#[test]
fn failure_state_round_trips_and_unknown_kinds_fall_back_safely() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks
           (id, path, title, artist, added_at, file_mtime, file_size)
         VALUES (1, '/fixture.flac', 'Fixture', 'Artist', 1, 20, 30)",
        [],
    )
    .unwrap();
    let expected = FailedAnalysis::new(
        SourceFingerprint::new(20, 30).unwrap(),
        AnalysisVersions::new(2, 3).unwrap(),
        50,
        FailureKind::UnsupportedFormat,
        "fixture codec",
        2,
        Some(70),
    )
    .unwrap();

    save_failed_analysis(&conn, 1, &expected).unwrap();
    assert_eq!(
        load_analysis(&conn, 1).unwrap(),
        Some(TrackAnalysis::Failed(expected))
    );

    conn.execute(
        "UPDATE track_audio_analysis SET failure_kind = 'future_kind' WHERE track_id = 1",
        [],
    )
    .unwrap();
    let Some(TrackAnalysis::Failed(unknown)) = load_analysis(&conn, 1).unwrap() else {
        panic!("expected stored failure");
    };
    assert_eq!(unknown.kind, FailureKind::Unknown);
}

#[test]
fn pending_work_and_coverage_share_current_present_track_semantics() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    for id in 1_i64..=6 {
        conn.execute(
            "INSERT INTO tracks
               (id, path, title, artist, added_at, file_mtime, file_size)
             VALUES (?1, ?2, 'Fixture', 'Artist', 1, 20, 30)",
            rusqlite::params![id, format!("/fixture-{id}.flac")],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO listen_events (track_id, played_at, ms_played)
             VALUES (?1, 100, 200000)",
            [id],
        )
        .unwrap();
    }
    save_ready_analysis(&conn, 1, &ready_analysis()).unwrap();
    save_ready_analysis(&conn, 2, &ready_analysis()).unwrap();
    conn.execute("UPDATE tracks SET file_size = 31 WHERE id = 2", [])
        .unwrap();
    let failure = FailedAnalysis::new(
        SourceFingerprint::new(20, 30).unwrap(),
        AnalysisVersions::new(2, 3).unwrap(),
        50,
        FailureKind::Decode,
        "fixture failure",
        1,
        None,
    )
    .unwrap();
    save_failed_analysis(&conn, 3, &failure).unwrap();
    conn.execute_batch(
        "UPDATE tracks SET removed_at = 200 WHERE id = 5;
         UPDATE tracks SET missing_since = 200, missing_reason = 'unknown' WHERE id = 6",
    )
    .unwrap();
    let versions = AnalysisVersions::new(2, 3).unwrap();

    let pending = pending_tracks(&conn, versions).unwrap();
    assert_eq!(
        pending.iter().map(|track| track.id).collect::<Vec<_>>(),
        vec![2, 4]
    );
    assert_eq!(
        library_coverage(&conn, versions).unwrap(),
        Coverage::new(1, 4)
    );
    assert_eq!(
        listen_coverage(&conn, versions, 0, 200).unwrap(),
        Coverage::new(1, 4)
    );

    let reprojection = pending_tracks(&conn, AnalysisVersions::new(2, 4).unwrap()).unwrap();
    assert_eq!(
        reprojection
            .iter()
            .find(|track| track.id == 1)
            .map(|track| track.work),
        Some(PendingWork::Reproject)
    );
}

#[test]
fn deleting_a_track_cascades_its_audio_analysis() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks
           (id, path, title, artist, added_at, file_mtime, file_size)
         VALUES (1, '/fixture.flac', 'Fixture', 'Artist', 1, 20, 30)",
        [],
    )
    .unwrap();
    save_ready_analysis(&conn, 1, &ready_analysis()).unwrap();

    conn.execute("DELETE FROM tracks WHERE id = 1", []).unwrap();

    assert_eq!(load_analysis(&conn, 1).unwrap(), None);
}

#[test]
fn empty_and_missing_only_libraries_have_zero_coverage() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let versions = AnalysisVersions::new(2, 3).unwrap();
    assert_eq!(
        library_coverage(&conn, versions).unwrap(),
        Coverage::new(0, 0)
    );

    conn.execute(
        "INSERT INTO tracks
           (id, path, title, artist, added_at, file_mtime, file_size,
            missing_since, missing_reason)
         VALUES (1, '/missing.flac', 'Missing', 'Artist', 1, 20, 30,
                 40, 'unknown')",
        [],
    )
    .unwrap();
    assert_eq!(
        library_coverage(&conn, versions).unwrap(),
        Coverage::new(0, 0)
    );
    assert!(pending_tracks(&conn, versions).unwrap().is_empty());
}

#[test]
fn manipulated_non_finite_or_out_of_range_rows_are_never_loaded_as_ready() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks
           (id, path, title, artist, added_at, file_mtime, file_size)
         VALUES (1, '/fixture.flac', 'Fixture', 'Artist', 1, 20, 30)",
        [],
    )
    .unwrap();
    save_ready_analysis(&conn, 1, &ready_analysis()).unwrap();
    conn.pragma_update(None, "ignore_check_constraints", true)
        .unwrap();
    conn.execute(
        "UPDATE track_audio_analysis SET intensity = 2.0 WHERE track_id = 1",
        [],
    )
    .unwrap();

    assert!(load_analysis(&conn, 1).is_err());
}

#[test]
fn stale_worker_result_is_rejected_at_the_storage_boundary() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks
           (id, path, title, artist, added_at, file_mtime, file_size)
         VALUES (1, '/fixture.flac', 'Fixture', 'Artist', 1, 20, 30)",
        [],
    )
    .unwrap();
    conn.execute("UPDATE tracks SET file_mtime = 21 WHERE id = 1", [])
        .unwrap();

    assert!(!save_ready_analysis(&conn, 1, &ready_analysis()).unwrap());
    let failure = FailedAnalysis::new(
        SourceFingerprint::new(20, 30).unwrap(),
        AnalysisVersions::new(2, 3).unwrap(),
        50,
        FailureKind::Decode,
        "stale decode",
        1,
        None,
    )
    .unwrap();
    assert!(!save_failed_analysis(&conn, 1, &failure).unwrap());
    assert_eq!(load_analysis(&conn, 1).unwrap(), None);
}
