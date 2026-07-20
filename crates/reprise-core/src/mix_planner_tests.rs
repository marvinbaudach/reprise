use rusqlite::{params, Connection};

use crate::mix_planner::{
    approve_mix_draft, load_mix_draft, plan_candidates, plan_mix, plan_mix_draft,
    profile_target_for_tracks, query_candidates, CandidateProfile, CriteriaMode, EnergyCurve,
    Familiarity, MixCandidate, MixDiagnostic, MixIntent, MixSource, ProfileTarget, SelectionReason,
    Variety, MAX_CANDIDATES, MAX_EXPLICIT_TRACK_IDS,
};

fn candidate(id: i64, artist: &str, intensity: f64, duration_ms: i64) -> MixCandidate {
    MixCandidate {
        track_id: id,
        title: format!("Track {id}"),
        artist: artist.to_string(),
        album: "Album".to_string(),
        genre: "Rock".to_string(),
        duration_ms,
        rating: 0,
        play_count: 0,
        profile: Some(CandidateProfile {
            intensity,
            brightness: intensity,
            dynamicity: intensity,
            rhythmicity: intensity,
            tempo_bpm: None,
        }),
    }
}

fn planning_intent(duration_ms: i64, curve: EnergyCurve) -> MixIntent {
    MixIntent::new(
        MixSource::Library,
        vec![99],
        CriteriaMode::AudioCharacter,
        ProfileTarget::new(0.2, 0.2, 0.2, 0.2).unwrap(),
        duration_ms,
        Familiarity::Balanced,
        Variety::Balanced,
        curve,
    )
    .unwrap()
}

fn migrated() -> Connection {
    crate::db::open_migrated(None).unwrap()
}

fn insert_track(conn: &Connection, id: i64, present: bool, analyzed: bool) {
    conn.execute(
        "INSERT INTO tracks
         (id, path, title, artist, album, genre, duration_ms, rating, play_count,
          added_at, file_mtime, file_size, missing_since)
         VALUES (?1, ?2, ?3, ?4, 'Album', 'Rock', 180000, 3, ?1, 1, 10, 20, ?5)",
        params![
            id,
            format!("/fixture/{id}.flac"),
            format!("Track {id}"),
            format!("Artist {}", id % 7),
            (!present).then_some(10)
        ],
    )
    .unwrap();
    if analyzed {
        conn.execute(
            "INSERT INTO track_audio_analysis
             (track_id, source_mtime, source_size, extractor_version, profile_version,
              analyzed_at, status, loudness_rms, dynamic_range, spectral_centroid_hz,
              spectral_rolloff_hz, spectral_flux, onset_rate, intensity,
              intensity_confidence, brightness, brightness_confidence, dynamicity,
              dynamicity_confidence, rhythmicity, rhythmicity_confidence)
             VALUES (?1, 10, 20, 1, 1, 30, 'ready', 0.1, 0.2, 1000, 2000,
                     0.3, 0.4, ?2, 0.9, 0.4, 0.9, 0.5, 0.9, 0.6, 0.9)",
            params![id, id as f64 / 1000.0],
        )
        .unwrap();
    }
}

#[test]
fn mix_intent_json_round_trip_is_canonical_and_rejects_unknown_or_invalid_values() {
    let intent = MixIntent::new(
        MixSource::Library,
        vec![7, 3],
        CriteriaMode::Balanced,
        ProfileTarget::new(0.25, 0.5, 0.75, 1.0).unwrap(),
        3_600_000,
        Familiarity::Balanced,
        Variety::Wide,
        EnergyCurve::Arc,
    )
    .unwrap();
    let json = intent.to_json().unwrap();
    assert_eq!(
        MixIntent::from_json(&json).unwrap().to_json().unwrap(),
        json
    );
    assert!(MixIntent::from_json(&json.replace("\"source\"", "\"unknown\":1,\"source\"")).is_err());
    assert!(MixIntent::from_json(&json.replace("0.25", "1.25")).is_err());
    assert!(MixIntent::new(
        MixSource::Tracks(vec![1; MAX_EXPLICIT_TRACK_IDS + 1]),
        vec![1],
        CriteriaMode::AudioCharacter,
        ProfileTarget::neutral(),
        1,
        Familiarity::Balanced,
        Variety::Balanced,
        EnergyCurve::Flat,
    )
    .is_err());
}

#[test]
fn profile_target_intent_supports_stats_and_agent_entry_without_seed_tracks() {
    let intent = MixIntent::from_target(
        MixSource::Library,
        ProfileTarget::new(0.8, 0.2, 0.3, 0.4).unwrap(),
        3_600_000,
        Familiarity::Balanced,
        Variety::Balanced,
        EnergyCurve::Flat,
    )
    .unwrap();
    assert!(intent.seeds().is_empty());
    assert_eq!(intent.criteria(), CriteriaMode::AudioCharacter);
    assert!(MixIntent::new(
        MixSource::Library,
        Vec::new(),
        CriteriaMode::AudioCharacter,
        ProfileTarget::neutral(),
        3_600_000,
        Familiarity::Balanced,
        Variety::Balanced,
        EnergyCurve::Flat,
    )
    .is_err());
}

#[test]
fn ac_15_candidate_query_is_bounded_stable_and_excludes_ineligible_analysis() {
    let conn = migrated();
    for id in 1..=(MAX_CANDIDATES as i64 + 20) {
        insert_track(&conn, id, id != 2, id != 3);
    }
    let intent = MixIntent::new(
        MixSource::Library,
        vec![1],
        CriteriaMode::AudioCharacter,
        ProfileTarget::neutral(),
        3_600_000,
        Familiarity::Balanced,
        Variety::Balanced,
        EnergyCurve::Flat,
    )
    .unwrap()
    .excluding_tracks(vec![4])
    .unwrap();
    let candidates = query_candidates(&conn, &intent).unwrap();
    assert_eq!(candidates.len(), MAX_CANDIDATES);
    assert_eq!(candidates[0].track_id, 5);
    assert!(candidates
        .windows(2)
        .all(|pair| pair[0].track_id < pair[1].track_id));
    assert!(candidates
        .iter()
        .all(|candidate| candidate.profile.is_some()));
    assert!(
        query_candidates(&conn, &intent.clone().with_min_confidence(0.95).unwrap())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn ac_13_genre_intent_uses_present_source_membership_without_inventing_audio_profiles() {
    let conn = migrated();
    insert_track(&conn, 1, true, false);
    insert_track(&conn, 2, true, true);
    insert_track(&conn, 3, false, true);
    conn.execute(
        "INSERT INTO playlists (id, name, position) VALUES (9, 'Seeds', 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (9, 1, 0), (9, 3, 1)",
        [],
    )
    .unwrap();
    let intent = MixIntent::new(
        MixSource::Playlist(9),
        vec![1],
        CriteriaMode::Genre,
        ProfileTarget::neutral(),
        600_000,
        Familiarity::Discover,
        Variety::Cohesive,
        EnergyCurve::Flat,
    )
    .unwrap()
    .including_seeds(true);
    let candidates = query_candidates(&conn, &intent).unwrap();
    assert_eq!(
        candidates
            .iter()
            .map(|item| item.track_id)
            .collect::<Vec<_>>(),
        vec![1]
    );
    assert_eq!(candidates[0].profile, None);
}

#[test]
fn ac_15_planning_is_deterministic_weighted_and_explainable() {
    let intent = planning_intent(360_000, EnergyCurve::Flat);
    let candidates = vec![
        candidate(3, "C", 0.9, 180_000),
        candidate(2, "B", 0.3, 180_000),
        candidate(1, "A", 0.2, 180_000),
    ];
    let first = plan_candidates(&intent, candidates.clone()).unwrap();
    let second = plan_candidates(&intent, candidates).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first
            .tracks
            .iter()
            .map(|track| track.track_id)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(first.tracks[0]
        .reasons
        .contains(&SelectionReason::IntensityMatch));
}

#[test]
fn artist_gap_is_kept_when_possible_and_reported_when_relaxed() {
    let intent = planning_intent(900_000, EnergyCurve::Flat);
    let diverse = vec![
        candidate(1, "A", 0.20, 180_000),
        candidate(2, "A", 0.21, 180_000),
        candidate(3, "B", 0.22, 180_000),
        candidate(4, "C", 0.23, 180_000),
        candidate(5, "D", 0.24, 180_000),
        candidate(6, "A", 0.25, 180_000),
    ];
    let draft = plan_candidates(&intent, diverse).unwrap();
    assert_eq!(
        draft
            .tracks
            .iter()
            .map(|track| track.artist.as_str())
            .collect::<Vec<_>>(),
        vec!["A", "B", "C", "D", "A"]
    );
    let relaxed = plan_candidates(
        &planning_intent(360_000, EnergyCurve::Flat),
        vec![
            candidate(1, "A", 0.2, 180_000),
            candidate(2, "A", 0.3, 180_000),
        ],
    )
    .unwrap();
    assert!(relaxed
        .diagnostics
        .contains(&MixDiagnostic::ArtistGapRelaxed));
}

#[test]
fn duration_chooses_smaller_deviation_and_curve_only_reorders_membership() {
    let candidates = vec![
        candidate(1, "A", 0.1, 200_000),
        candidate(2, "B", 0.2, 200_000),
        candidate(3, "C", 0.3, 200_000),
        candidate(4, "D", 0.4, 200_000),
    ];
    let flat = plan_candidates(
        &planning_intent(510_000, EnergyCurve::Flat),
        candidates.clone(),
    )
    .unwrap();
    let rise = plan_candidates(&planning_intent(510_000, EnergyCurve::Rise), candidates).unwrap();
    assert_eq!(flat.total_duration_ms, 600_000);
    let mut flat_ids = flat
        .tracks
        .iter()
        .map(|track| track.track_id)
        .collect::<Vec<_>>();
    let mut rise_ids = rise
        .tracks
        .iter()
        .map(|track| track.track_id)
        .collect::<Vec<_>>();
    flat_ids.sort_unstable();
    rise_ids.sort_unstable();
    assert_eq!(flat_ids, rise_ids);
    assert!(rise
        .tracks
        .windows(2)
        .all(|pair| pair[0].profile_intensity <= pair[1].profile_intensity));
}

#[test]
fn selected_seed_profiles_produce_the_exact_average_target() {
    let conn = migrated();
    insert_track(&conn, 100, true, true);
    insert_track(&conn, 300, true, true);
    let target = profile_target_for_tracks(&conn, &[300, 100]).unwrap();
    assert_eq!(target.values()[0], 0.2);
    insert_track(&conn, 400, true, false);
    assert!(profile_target_for_tracks(&conn, &[100, 400]).is_err());
}

#[test]
fn genre_planning_derives_normalized_genre_evidence_from_selected_seeds() {
    let conn = migrated();
    insert_track(&conn, 1, true, false);
    insert_track(&conn, 2, true, false);
    conn.execute("UPDATE tracks SET genre = '  Röck  ' WHERE id = 1", [])
        .unwrap();
    let intent = MixIntent::new(
        MixSource::Library,
        vec![1],
        CriteriaMode::Genre,
        ProfileTarget::neutral(),
        180_000,
        Familiarity::Balanced,
        Variety::Cohesive,
        EnergyCurve::Flat,
    )
    .unwrap();
    let draft = plan_mix(&conn, &intent).unwrap();
    assert_eq!(draft.tracks[0].track_id, 2);
    assert!(draft.tracks[0]
        .reasons
        .contains(&SelectionReason::GenreMatch));
}

#[test]
fn durable_draft_round_trips_and_approval_uses_exact_preview_idempotently() {
    let mut conn = migrated();
    for id in 1..=4 {
        insert_track(&conn, id, true, true);
    }
    let intent = MixIntent::new(
        MixSource::Library,
        vec![1],
        CriteriaMode::AudioCharacter,
        ProfileTarget::neutral(),
        360_000,
        Familiarity::Balanced,
        Variety::Balanced,
        EnergyCurve::Flat,
    )
    .unwrap();
    let draft = plan_mix_draft(&conn, &intent, 1_000, 600).unwrap();
    assert_eq!(
        load_mix_draft(&conn, &draft.draft_id, 1_001).unwrap(),
        draft
    );
    let expected_ids = draft
        .tracks
        .iter()
        .map(|track| track.track_id)
        .collect::<Vec<_>>();
    let first = approve_mix_draft(
        &mut conn,
        &draft.draft_id,
        "Similar mix",
        "request-1",
        1_002,
    )
    .unwrap();
    let second =
        approve_mix_draft(&mut conn, &draft.draft_id, "Ignored", "request-1", 1_003).unwrap();
    assert_eq!(first, second);
    let stored = crate::queries::query_track_ids(
        &conn,
        &crate::view_source::ViewSource::Playlist(first.playlist_id),
        "playlist_order",
        "asc",
        "",
        &[],
    )
    .unwrap();
    assert_eq!(stored, expected_ids);
}

#[test]
fn draft_staleness_is_scoped_to_selected_tracks() {
    let mut conn = migrated();
    for id in 1..=5 {
        insert_track(&conn, id, true, true);
    }
    let draft = plan_mix_draft(
        &conn,
        &planning_intent(360_000, EnergyCurve::Flat),
        1_000,
        600,
    )
    .unwrap();
    let unrelated = (1..=5)
        .find(|id| !draft.tracks.iter().any(|track| track.track_id == *id))
        .unwrap();
    conn.execute(
        "UPDATE tracks SET file_mtime = 99 WHERE id = ?1",
        [unrelated],
    )
    .unwrap();
    assert!(approve_mix_draft(
        &mut conn,
        &draft.draft_id,
        "Still valid",
        "unrelated",
        1_001
    )
    .is_ok());

    let second = plan_mix_draft(
        &conn,
        &planning_intent(540_000, EnergyCurve::Flat),
        2_000,
        600,
    )
    .unwrap();
    let selected = second.tracks[0].track_id;
    conn.execute(
        "UPDATE tracks SET file_mtime = 88 WHERE id = ?1",
        [selected],
    )
    .unwrap();
    assert!(approve_mix_draft(&mut conn, &second.draft_id, "Stale", "selected", 2_001).is_err());
}
