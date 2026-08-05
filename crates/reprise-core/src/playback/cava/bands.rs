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
