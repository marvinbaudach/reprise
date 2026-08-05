use reprise_core::sound_features::SoundFeatures;
use reprise_core::sound_neighbours::{SoundNeighbour, SoundNeighbourResult};
use reprise_core::sound_stats::compute_sound_stats;

use super::{profile_positions, ready_for_matches, shown_track_ids, MIN_READY_FEATURES};

fn feature(timbre: f32, dynamics: f32, tempo: Option<f32>) -> SoundFeatures {
    SoundFeatures {
        band_mean: [0.0; reprise_core::spectrogram::SPECTROGRAM_BAND_COUNT],
        centroid_mean: timbre,
        centroid_var: 0.0,
        frame_crest_db: dynamics,
        tempo,
    }
}

#[test]
fn sound_panel_requires_fifty_profiles_and_the_current_track() {
    assert!(!ready_for_matches(MIN_READY_FEATURES - 1, true));
    assert!(!ready_for_matches(MIN_READY_FEATURES, false));
    assert!(ready_for_matches(MIN_READY_FEATURES, true));
}

#[test]
fn profile_uses_library_percentiles_and_disables_tempo_when_excluded() {
    let values = [
        feature(1.0, 10.0, Some(60.0)),
        feature(2.0, 20.0, Some(90.0)),
        feature(3.0, 30.0, Some(120.0)),
    ];
    let stats = compute_sound_stats(&values);

    let positions = profile_positions(&values[1], &stats, false);

    assert_eq!(positions.timbre, 50.0);
    assert_eq!(positions.dynamics, 50.0);
    assert_eq!(positions.tempo, None);
}

#[test]
fn queue_action_preserves_the_rendered_neighbour_order() {
    let result = SoundNeighbourResult {
        library_count: 42,
        matches: vec![
            SoundNeighbour {
                track_id: 9,
                path: "/9.flac".into(),
                title: "Nine".into(),
                artist: "A".into(),
                distance: 0.1,
                percentile: 99.0,
            },
            SoundNeighbour {
                track_id: 3,
                path: "/3.flac".into(),
                title: "Three".into(),
                artist: "B".into(),
                distance: 0.2,
                percentile: 95.0,
            },
        ],
    };

    assert_eq!(shown_track_ids(&result), vec![9, 3]);
}
