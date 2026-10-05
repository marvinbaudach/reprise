//! What reaches the spectrogram after the downmix and the resample: ultrasonic
//! tones must not fold into the audible bands, and the LFE channel of a 5.1
//! stream must not take part in the mono mix.

use super::RenderDataSession;
use crate::spectrogram::TrackSpectrogram;

const SECONDS: usize = 2;
/// WAVE channel order of a 5.1 stream: FL FR FC LFE BL BR.
const SURROUND_CHANNELS: usize = 6;
const LFE_INDEX: usize = 3;
/// A cell is one byte of absolute dBFS; anything above this is audible energy.
const SILENT_CELL_LEVEL: u8 = 8;

fn tone(sample_rate_hz: u32, frequency_hz: f64) -> Vec<f32> {
    (0..sample_rate_hz as usize * SECONDS)
        .map(|index| {
            (std::f64::consts::TAU * frequency_hz * index as f64 / f64::from(sample_rate_hz)).sin()
                as f32
                * 0.5
        })
        .collect()
}

fn spectrogram_of(samples: &[f32], sample_rate_hz: u32, channels: u32) -> TrackSpectrogram {
    let mut session = RenderDataSession::new();
    session
        .push_pcm_f32(samples, sample_rate_hz, channels)
        .unwrap();
    session.finish().unwrap().spectrogram
}

fn loudest_cell(spectrogram: &TrackSpectrogram) -> u8 {
    // Skip the first frames: the filter and the FFT window are still warming up.
    (5..spectrogram.frame_count())
        .filter_map(|index| spectrogram.frame(index))
        .flat_map(|frame| frame.iter().copied())
        .max()
        .unwrap_or(0)
}

fn surround_with(channel: usize, signal: &[f32]) -> Vec<f32> {
    signal
        .iter()
        .flat_map(|sample| {
            (0..SURROUND_CHANNELS).map(move |index| if index == channel { *sample } else { 0.0 })
        })
        .collect()
}

#[test]
fn a_30_khz_tone_at_96_khz_does_not_fold_into_the_audible_bands() {
    let spectrogram = spectrogram_of(&tone(96_000, 30_000.0), 96_000, 1);

    assert!(spectrogram.frame_count() > 10);
    assert!(
        loudest_cell(&spectrogram) <= SILENT_CELL_LEVEL,
        "ultrasound reached the spectrogram: {:?}",
        spectrogram.frame(spectrogram.frame_count() - 1)
    );
}

#[test]
fn a_tone_inside_the_range_still_reaches_the_spectrogram() {
    let spectrogram = spectrogram_of(&tone(96_000, 1_000.0), 96_000, 1);

    assert!(loudest_cell(&spectrogram) > 100);
}

#[test]
fn the_lfe_channel_of_a_surround_stream_stays_out_of_the_mono_mix() {
    let signal = tone(48_000, 1_000.0);

    let lfe_only = spectrogram_of(&surround_with(LFE_INDEX, &signal), 48_000, 6);
    let front_left_only = spectrogram_of(&surround_with(0, &signal), 48_000, 6);

    assert!(
        loudest_cell(&lfe_only) <= SILENT_CELL_LEVEL,
        "the LFE channel leaked into the mix"
    );
    assert!(loudest_cell(&front_left_only) > 100);
}

#[test]
fn the_lfe_channel_is_excluded_from_the_i16_path_too() {
    let signal = tone(48_000, 1_000.0)
        .iter()
        .map(|sample| (sample * f32::from(i16::MAX)) as i16)
        .collect::<Vec<_>>();
    let interleaved = signal
        .iter()
        .flat_map(|sample| {
            (0..SURROUND_CHANNELS).map(move |index| if index == LFE_INDEX { *sample } else { 0 })
        })
        .collect::<Vec<_>>();
    let mut session = RenderDataSession::new();
    session.push_pcm_i16(&interleaved, 48_000, 6).unwrap();

    let spectrogram = session.finish().unwrap().spectrogram;

    assert!(loudest_cell(&spectrogram) <= SILENT_CELL_LEVEL);
}
