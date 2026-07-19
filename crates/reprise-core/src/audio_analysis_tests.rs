use super::*;

const SAMPLE_RATE: u32 = 8_000;
const MAX_STREAMING_FRAME_BUFFER: usize = 256;

fn sine(frequency: f64, seconds: f64, amplitude: f64) -> Vec<f32> {
    let sample_count = (f64::from(SAMPLE_RATE) * seconds) as usize;
    (0..sample_count)
        .map(|index| {
            let phase = std::f64::consts::TAU * frequency * index as f64 / f64::from(SAMPLE_RATE);
            (phase.sin() * amplitude) as f32
        })
        .collect()
}

fn click_track(bpm: u32, seconds: u32) -> Vec<f32> {
    let sample_count = SAMPLE_RATE as usize * seconds as usize;
    let beat_samples = (60 * SAMPLE_RATE / bpm) as usize;
    let click_samples = (SAMPLE_RATE / 100) as usize;
    (0..sample_count)
        .map(|index| {
            if index % beat_samples < click_samples {
                0.9
            } else {
                0.0
            }
        })
        .collect()
}

fn deterministic_noise(sample_count: usize) -> Vec<f32> {
    let mut state = 0x1234_5678_u32;
    (0..sample_count)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state as f32 / u32::MAX as f32 - 0.5) * 0.8
        })
        .collect()
}

#[test]
fn chunk_boundaries_do_not_change_audio_character_results() {
    let samples = sine(440.0, 2.0, 0.5);
    let mut whole =
        AudioEvidenceAccumulator::new(SAMPLE_RATE, samples.len() as u64, 1_000).unwrap();
    whole.push(&samples).unwrap();
    let whole = whole.finish().unwrap();

    let mut chunked =
        AudioEvidenceAccumulator::new(SAMPLE_RATE, samples.len() as u64, 1_000).unwrap();
    for chunk in samples.chunks(137) {
        chunked.push(chunk).unwrap();
    }
    let chunked = chunked.finish().unwrap();

    assert_eq!(whole.waveform_peaks, chunked.waveform_peaks);
    assert_eq!(whole.evidence, chunked.evidence);
    assert_eq!(whole.profile, chunked.profile);
}

fn analyze(samples: &[f32]) -> AnalysisOutput {
    let mut accumulator =
        AudioEvidenceAccumulator::new(SAMPLE_RATE, samples.len() as u64, 1_000).unwrap();
    accumulator.push(samples).unwrap();
    accumulator.finish().unwrap()
}

#[test]
fn higher_frequency_has_a_brighter_profile() {
    let low = analyze(&sine(160.0, 2.0, 0.5));
    let high = analyze(&sine(2_400.0, 2.0, 0.5));

    assert!(high.profile.brightness.value().get() > low.profile.brightness.value().get() + 0.3);
    assert!(high.evidence.spectral_centroid_hz() > low.evidence.spectral_centroid_hz());
    assert!(high.evidence.spectral_rolloff_hz() > low.evidence.spectral_rolloff_hz());
}

#[test]
fn click_tracks_produce_stable_tempo_estimates() {
    for expected in [60, 90, 120, 180] {
        let result = analyze(&click_track(expected, 12));
        let tempo = result.evidence.tempo().expect("tempo estimate");

        assert!(
            (tempo.bpm() - f64::from(expected)).abs() <= 3.0,
            "expected {expected} BPM, got {}",
            tempo.bpm()
        );
        assert!(tempo.confidence().get() >= 0.5);
    }
}

#[test]
fn accumulator_rejects_an_unknown_sample_count() {
    assert!(matches!(
        AudioEvidenceAccumulator::new(SAMPLE_RATE, 0, 1_000),
        Err(AudioExtractionError::InvalidConfiguration)
    ));
}

#[test]
fn crescendo_is_more_dynamic_than_a_constant_tone() {
    let mut crescendo = sine(440.0, 4.0, 1.0);
    let length = crescendo.len() as f32;
    for (index, sample) in crescendo.iter_mut().enumerate() {
        *sample *= 0.05 + 0.85 * index as f32 / length;
    }
    let compressed = sine(440.0, 4.0, 0.5);

    assert!(
        analyze(&crescendo).profile.dynamicity.value().get()
            > analyze(&compressed).profile.dynamicity.value().get() + 0.2
    );
}

#[test]
fn silence_has_zero_evidence_and_a_fixed_zero_waveform() {
    let result = analyze(&vec![0.0; SAMPLE_RATE as usize]);

    assert_eq!(result.waveform_peaks, vec![0; 1_000]);
    assert_eq!(result.profile.intensity.value().get(), 0.0);
    assert_eq!(result.profile.brightness.value().get(), 0.0);
    assert_eq!(result.profile.dynamicity.value().get(), 0.0);
    assert_eq!(result.profile.rhythmicity.value().get(), 0.0);
    assert!(result.evidence.tempo().is_none());
}

#[test]
fn waveform_always_has_the_requested_bucket_count() {
    let result = analyze(&sine(440.0, 0.05, 0.5));

    assert_eq!(result.waveform_peaks.len(), 1_000);
    assert_eq!(result.waveform_peaks.iter().copied().max(), Some(255));
}

#[test]
fn accumulator_memory_is_bounded_by_configuration() {
    let samples = sine(440.0, 30.0, 0.5);
    let mut accumulator =
        AudioEvidenceAccumulator::new(SAMPLE_RATE, samples.len() as u64, 1_000).unwrap();
    let initial_capacity = accumulator.buffered_sample_count();

    for chunk in samples.chunks(97) {
        accumulator.push(chunk).unwrap();
    }

    assert!(accumulator.buffered_sample_count() <= initial_capacity + MAX_STREAMING_FRAME_BUFFER);
}

#[test]
fn invalid_and_malformed_streams_return_errors_without_panicking() {
    let empty = AudioEvidenceAccumulator::new(SAMPLE_RATE, 1, 1_000).unwrap();
    assert!(matches!(
        empty.finish(),
        Err(AudioExtractionError::EmptyAudio)
    ));

    let mut non_finite = AudioEvidenceAccumulator::new(SAMPLE_RATE, 1, 1_000).unwrap();
    assert!(matches!(
        non_finite.push(&[f32::NAN]),
        Err(AudioExtractionError::NonFiniteSample)
    ));

    let mut too_long = AudioEvidenceAccumulator::new(SAMPLE_RATE, 1, 1_000).unwrap();
    assert!(matches!(
        too_long.push(&[0.0, 0.0]),
        Err(AudioExtractionError::TooManySamples)
    ));
}

#[test]
fn stored_evidence_can_be_reprojected_without_pcm() {
    let result = analyze(&sine(880.0, 2.0, 0.4));

    assert_eq!(project_profile(&result.evidence).unwrap(), result.profile);
    for dimension in [
        result.profile.intensity,
        result.profile.brightness,
        result.profile.dynamicity,
        result.profile.rhythmicity,
    ] {
        assert!((0.0..=1.0).contains(&dimension.value().get()));
        assert!((0.0..=1.0).contains(&dimension.confidence().get()));
    }
}

#[test]
fn broadband_noise_has_more_flux_and_brightness_than_a_low_tone() {
    let noise = analyze(&deterministic_noise(SAMPLE_RATE as usize * 2));
    let low = analyze(&sine(160.0, 2.0, 0.4));

    assert!(noise.evidence.spectral_flux() > low.evidence.spectral_flux());
    assert!(noise.profile.brightness.value().get() > low.profile.brightness.value().get());
}

#[test]
fn empty_chunks_and_a_partial_final_frame_are_safe() {
    let samples = [0.25; 17];
    let mut accumulator =
        AudioEvidenceAccumulator::new(SAMPLE_RATE, samples.len() as u64, 1_000).unwrap();
    accumulator.push(&[]).unwrap();
    accumulator.push(&samples[..3]).unwrap();
    accumulator.push(&[]).unwrap();
    accumulator.push(&samples[3..]).unwrap();

    let result = accumulator.finish().unwrap();
    assert_eq!(result.waveform_peaks.len(), 1_000);
    assert!(result.profile.intensity.value().get().is_finite());
}
