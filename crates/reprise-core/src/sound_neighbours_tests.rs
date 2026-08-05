use crate::sound_distance::DistanceWeights;
use crate::sound_features::SoundFeatures;
use crate::sound_neighbours::{rank_sound_neighbours, SoundCandidate, SoundNeighbourOptions};
use crate::sound_rhythm::RhythmFeatures;
use crate::sound_stats::compute_sound_stats;
use crate::spectrogram::SPECTROGRAM_BAND_COUNT;

fn candidate(id: i64, band: usize, artist: &str, album: &str) -> SoundCandidate {
    let mut band_mean = [0.0; SPECTROGRAM_BAND_COUNT];
    band_mean[band] = 1.0;
    SoundCandidate {
        track_id: id,
        path: format!("/{id}.flac"),
        title: format!("Track {id}"),
        artist: artist.into(),
        album: album.into(),
        album_artist: artist.into(),
        features: SoundFeatures {
            band_mean,
            centroid_mean: band as f32,
            centroid_var: 1.0,
            frame_crest_db: 1.0,
            rhythm: RhythmFeatures::still(),
            tempo: None,
        },
    }
}

#[test]
fn sim_3_neighbours_rank_against_the_whole_library() {
    let current = candidate(1, 0, "Artist", "Album");
    let candidates = [
        current.clone(),
        candidate(2, 0, "Other", "Other"),
        candidate(3, 1, "Other", "Third"),
        candidate(4, 2, "Another", "Fourth"),
    ];
    let stats = compute_sound_stats(
        &candidates
            .iter()
            .map(|candidate| candidate.features.clone())
            .collect::<Vec<_>>(),
    );
    let result = rank_sound_neighbours(
        &current,
        &candidates,
        &stats,
        DistanceWeights::DEFAULT,
        SoundNeighbourOptions {
            exclude_same_album: false,
            exclude_same_artist: false,
            limit: 7,
        },
    );

    assert_eq!(result.library_count, 3);
    assert_eq!(
        result
            .matches
            .iter()
            .map(|row| row.track_id)
            .collect::<Vec<_>>(),
        [2, 3, 4]
    );
    assert!(result.matches[0].percentile > result.matches[1].percentile);
    assert!(result.matches[1].percentile >= result.matches[2].percentile);
}

#[test]
fn sim_3_exclusions_apply_after_whole_library_percentiles() {
    let current = candidate(1, 0, "Artist", "Album");
    let mut same_album = candidate(3, 0, "Other", "Album");
    same_album.album_artist = "Artist".into();
    let candidates = [
        current.clone(),
        candidate(2, 0, "Artist", "Other"),
        same_album,
        candidate(4, 1, "Other", "Other"),
        candidate(5, 1, "Different", "Album"),
    ];
    let stats = compute_sound_stats(
        &candidates
            .iter()
            .map(|candidate| candidate.features.clone())
            .collect::<Vec<_>>(),
    );
    let unfiltered = rank_sound_neighbours(
        &current,
        &candidates,
        &stats,
        DistanceWeights::DEFAULT,
        SoundNeighbourOptions::default(),
    );
    let filtered = rank_sound_neighbours(
        &current,
        &candidates,
        &stats,
        DistanceWeights::DEFAULT,
        SoundNeighbourOptions {
            exclude_same_album: true,
            exclude_same_artist: true,
            limit: 7,
        },
    );

    assert_eq!(filtered.library_count, 4);
    assert_eq!(filtered.matches.len(), 2);
    assert_eq!(filtered.matches[0].track_id, 4);
    assert_eq!(filtered.matches[1].track_id, 5);
    let original = unfiltered
        .matches
        .iter()
        .find(|row| row.track_id == 4)
        .unwrap();
    assert_eq!(filtered.matches[0].percentile, original.percentile);
}

#[test]
fn sound_neighbours_loads_only_current_profiles_for_present_tracks() {
    let db = crate::db::Db::open_in_memory().unwrap();
    for (id, missing_since) in [(1, None), (2, Some(10))] {
        db.conn()
            .execute(
                "INSERT INTO tracks \
                 (id, path, title, artist, album, added_at, missing_since) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
                rusqlite::params![
                    id,
                    format!("/{id}.flac"),
                    format!("Title {id}"),
                    format!("Artist {id}"),
                    format!("Album {id}"),
                    missing_since
                ],
            )
            .unwrap();
        crate::db::set_track_sound_features(&db, id, &candidate(id, 0, "", "").features).unwrap();
    }

    let loaded = crate::sound_neighbours::load_sound_candidates(&db).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].track_id, 1);
    assert_eq!(loaded[0].path, "/1.flac");
    assert_eq!(loaded[0].title, "Title 1");
    assert_eq!(loaded[0].artist, "Artist 1");
    assert_eq!(loaded[0].album, "Album 1");
}

#[test]
fn sound_neighbours_skip_one_unreadable_row_and_keep_the_rest() {
    let db = crate::db::Db::open_in_memory().unwrap();
    for id in [1, 2] {
        db.conn()
            .execute(
                "INSERT INTO tracks (id, path, title, added_at) VALUES (?1, ?2, ?3, 0)",
                rusqlite::params![id, format!("/{id}.flac"), format!("Title {id}")],
            )
            .unwrap();
        crate::db::set_track_sound_features(&db, id, &candidate(id, 0, "", "").features).unwrap();
    }
    db.conn()
        .execute(
            "UPDATE track_sound_features SET data = ?1 WHERE track_id = 1",
            [vec![0_u8; 7]],
        )
        .unwrap();

    let loaded = crate::sound_neighbours::load_sound_candidates(&db).unwrap();

    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].track_id, 2);
}

/// SIM-10: one artist may hold two places, and the list moves on to someone
/// else rather than turning into a back catalogue. Six candidates by "Prolific"
/// sit nearer than the two by other people, so without the cap the list would
/// be all Prolific.
#[test]
fn sim_10_at_most_two_matches_carry_the_same_artist() {
    let current = candidate(1, 0, "Seed", "Seed album");
    let mut candidates = vec![current.clone()];
    for id in 2..8 {
        candidates.push(candidate(id, 0, "Prolific", &format!("Album {id}")));
    }
    candidates.push(candidate(20, 1, "Distant one", "Elsewhere"));
    candidates.push(candidate(21, 2, "Distant two", "Elsewhere too"));
    let stats = compute_sound_stats(
        &candidates
            .iter()
            .map(|candidate| candidate.features.clone())
            .collect::<Vec<_>>(),
    );

    let result = rank_sound_neighbours(
        &current,
        &candidates,
        &stats,
        DistanceWeights::DEFAULT,
        SoundNeighbourOptions {
            exclude_same_album: false,
            exclude_same_artist: false,
            limit: 7,
        },
    );

    let prolific = result
        .matches
        .iter()
        .filter(|neighbour| neighbour.artist == "Prolific")
        .count();
    assert_eq!(prolific, 2, "one artist may hold two places, not six");
    // The nearest match is never displaced by the cap.
    assert_eq!(result.matches[0].artist, "Prolific");
    // What the cap frees goes to other people, not to nobody.
    let artists: Vec<&str> = result
        .matches
        .iter()
        .map(|neighbour| neighbour.artist.as_str())
        .collect();
    assert!(artists.contains(&"Distant one"));
    assert!(artists.contains(&"Distant two"));
}

/// SIM-10: an empty artist tag is not an identity two tracks share, so those
/// tracks must not cap each other out of the list.
#[test]
fn sim_10_tracks_without_an_artist_do_not_cap_each_other() {
    let current = candidate(1, 0, "Seed", "Seed album");
    let mut candidates = vec![current.clone()];
    for id in 2..8 {
        candidates.push(candidate(id, 0, "", &format!("Album {id}")));
    }
    let stats = compute_sound_stats(
        &candidates
            .iter()
            .map(|candidate| candidate.features.clone())
            .collect::<Vec<_>>(),
    );

    let result = rank_sound_neighbours(
        &current,
        &candidates,
        &stats,
        DistanceWeights::DEFAULT,
        SoundNeighbourOptions {
            exclude_same_album: false,
            exclude_same_artist: false,
            limit: 7,
        },
    );

    assert_eq!(result.matches.len(), 6);
}
