use crate::sound_features::SoundFeatures;
use crate::sound_profile::profile_positions;
use crate::sound_rhythm::RhythmFeatures;
use crate::sound_stats::compute_sound_stats;
use crate::spectrogram::SPECTROGRAM_BAND_COUNT;

fn feature(timbre: f32, dynamics: f32, tempo: Option<f32>) -> SoundFeatures {
    SoundFeatures {
        band_mean: [0.0; SPECTROGRAM_BAND_COUNT],
        centroid_mean: timbre,
        centroid_var: 0.0,
        frame_crest_db: dynamics,
        rhythm: RhythmFeatures::still(),
        tempo,
    }
}

#[test]
fn sim_4_profile_uses_library_percentiles_and_disables_tempo_when_excluded() {
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
