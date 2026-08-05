use crate::sound_distance::DistanceWeights;
use crate::sound_features::SoundFeatures;
use crate::sound_neighbours::{rank_sound_neighbours, SoundCandidate, SoundNeighbourOptions};
use crate::sound_stats::compute_sound_stats;
use crate::spectrogram::SPECTROGRAM_BAND_COUNT;

fn candidate(id: i64, band: usize, artist: &str, album: &str) -> SoundCandidate {
    let mut band_mean = [0.0; SPECTROGRAM_BAND_COUNT];
    band_mean[band] = 1.0;
    SoundCandidate {
        track_id: id,
        title: format!("Track {id}"),
        artist: artist.into(),
        album: album.into(),
        features: SoundFeatures {
            band_mean,
            centroid_mean: band as f32,
            centroid_var: 1.0,
            frame_crest_db: 1.0,
            tempo: None,
        },
    }
}

#[test]
fn sound_neighbours_rank_nearest_first_and_percentile_against_the_whole_library() {
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
fn sound_neighbours_apply_album_and_artist_exclusions_after_percentiles() {
    let current = candidate(1, 0, "Artist", "Album");
    let candidates = [
        current.clone(),
        candidate(2, 0, "Artist", "Other"),
        candidate(3, 0, "Other", "Album"),
        candidate(4, 1, "Other", "Other"),
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

    assert_eq!(filtered.library_count, 3);
    assert_eq!(filtered.matches.len(), 1);
    assert_eq!(filtered.matches[0].track_id, 4);
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
    assert_eq!(loaded[0].title, "Title 1");
    assert_eq!(loaded[0].artist, "Artist 1");
    assert_eq!(loaded[0].album, "Album 1");
}
