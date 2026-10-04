use super::RenderDataSession;

const SAMPLE_RATE: u32 = 48_000;
const SECONDS: usize = 4;

fn sine(amplitude: f32, channels: u32) -> Vec<f32> {
    (0..SAMPLE_RATE as usize * SECONDS)
        .flat_map(|index| {
            let phase = std::f32::consts::TAU * 1_000.0 * index as f32 / SAMPLE_RATE as f32;
            std::iter::repeat_n(phase.sin() * amplitude, channels as usize)
        })
        .collect()
}

fn measured(amplitude: f32, channels: u32) -> crate::library::loudness::MeasuredLoudness {
    let mut session = RenderDataSession::new();
    session
        .push_pcm_f32(&sine(amplitude, channels), SAMPLE_RATE, channels)
        .unwrap();
    session.finish().unwrap().loudness.unwrap()
}

#[test]
fn halving_amplitude_reduces_integrated_loudness_by_six_decibels() {
    let full = measured(1.0, 1);
    let half = measured(0.5, 1);

    assert!(((half.integrated_lufs - full.integrated_lufs) + 6.02).abs() <= 0.05);
}

#[test]
fn identical_stereo_is_three_decibels_louder_than_mono() {
    let mono = measured(0.5, 1);
    let stereo = measured(0.5, 2);

    assert!(((stereo.integrated_lufs - mono.integrated_lufs) - 3.01).abs() <= 0.05);
}

#[test]
fn silence_has_no_measured_loudness() {
    let mut session = RenderDataSession::new();
    session
        .push_pcm_f32(&vec![0.0; SAMPLE_RATE as usize * SECONDS], SAMPLE_RATE, 1)
        .unwrap();

    assert_eq!(session.finish().unwrap().loudness, None);
}

#[test]
fn a_full_scale_sine_has_a_unit_true_peak() {
    let loudness = measured(1.0, 1);

    assert!((loudness.true_peak - 1.0).abs() <= 0.02, "{loudness:?}");
}
