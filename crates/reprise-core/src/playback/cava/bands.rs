use crate::playback::spectral::{BandPlan, BandPlanError};

use super::{CavaConfig, CavaError};

const BASE_FFT_SIZE: usize = 512;
const BASS_SPLIT_HZ: f32 = 100.0;

pub(super) fn band_plan(config: CavaConfig) -> Result<BandPlan, CavaError> {
    let fft_size = fft_size_for_rate(config.sample_rate_hz);
    BandPlan::new(
        config.sample_rate_hz,
        config.bar_count,
        config.low_cutoff_hz,
        config.high_cutoff_hz,
        fft_size,
        fft_size * 2,
        BASS_SPLIT_HZ,
    )
    .map_err(|error| match error {
        BandPlanError::TooManyBands => CavaError::InvalidBarCount,
        BandPlanError::InvalidConfiguration => CavaError::InvalidCutoffRange,
    })
}

/// CAVA's per-band frequency compensation, one factor per bar.
///
/// Absolute spectral energy falls off towards the treble and a wide band sums
/// more bins than a narrow one, so an uncompensated bar chart is bass-heavy and
/// its shape depends on the FFT grid. `cavacore` corrects both with this curve.
/// It is a property of *drawing* a spectrum, not of measuring one, which is why
/// it lives beside the renderer instead of in the shared band plan: the stored
/// spectrogram deliberately keeps raw absolute levels.
///
/// A single global sensitivity scalar cannot stand in for this — it moves every
/// bar by the same amount and therefore cannot restore a shape.
pub(super) fn equalizer_curve(config: CavaConfig, plan: &BandPlan) -> Vec<f32> {
    let fft_size = fft_size_for_rate(config.sample_rate_hz);
    let bass_fft_size = fft_size * 2;
    let cutoffs = plan.cutoff_frequencies_hz();
    plan.bands()
        .iter()
        .enumerate()
        .map(|(index, band)| {
            let window_size = if band.use_bass_fft {
                bass_fft_size
            } else {
                fft_size
            };
            let bin_count = band.upper_bin.saturating_sub(band.lower_bin) + 1;
            2.0_f32.powi(-28) * cutoffs[index + 1].powf(0.85)
                / (window_size as f32).log2()
                / bin_count as f32
        })
        .collect()
}

pub(super) fn fft_size_for_rate(sample_rate_hz: u32) -> usize {
    match sample_rate_hz {
        0..=8_125 => BASE_FFT_SIZE,
        8_126..=16_250 => BASE_FFT_SIZE * 2,
        16_251..=32_500 => BASE_FFT_SIZE * 4,
        32_501..=75_000 => BASE_FFT_SIZE * 8,
        75_001..=150_000 => BASE_FFT_SIZE * 16,
        150_001..=300_000 => BASE_FFT_SIZE * 32,
        _ => BASE_FFT_SIZE * 64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equalizer_curve_matches_cavas_pinned_compensation() {
        let config = CavaConfig::new(44_100, 10);
        let plan = band_plan(config).unwrap();
        // `cavacore`'s own values for this layout. Dropping the compensation
        // flattens the live picture in a way the global sensitivity scalar
        // cannot undo, so these are pinned rather than merely bounded.
        let expected = [
            1.9958803e-9,
            1.8505456e-9,
            3.736_56e-9,
            3.2925376e-9,
            3.0513663e-9,
            2.8075227e-9,
            2.5947273e-9,
            2.4007636e-9,
            2.2097977e-9,
            2.0417181e-9,
        ];

        let curve = equalizer_curve(config, &plan);

        assert_eq!(curve.len(), expected.len());
        for (index, (actual, expected)) in curve.iter().zip(expected).enumerate() {
            assert!(
                (actual - expected).abs() <= expected * 1.0e-5,
                "band {index}: expected {expected:e}, got {actual:e}"
            );
        }
    }
}
