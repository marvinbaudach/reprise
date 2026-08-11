//! Adapts one already-smoothed mobile spectrogram frame to Song Visuals.

use crate::playback::{BassPressure, SpectrumFrame, SPECTRUM_BAND_COUNT};
use crate::spectrogram::{SPECTROGRAM_CEILING_DBFS, SPECTROGRAM_FLOOR_DBFS};

/// The first seven stored log bands cover approximately 20–139 Hz.
const BASS_BAND_COUNT: usize = 7;

/// Interpolates an already-smoothed logarithmic spectrogram frame to the
/// engine's fixed-width bars without applying another gain or temporal filter.
pub fn spectrum_frame_from_bands(bands: &[f32]) -> SpectrumFrame {
    let sanitized: Vec<f32> = bands.iter().copied().map(sanitize_band).collect();
    let interpolated = std::array::from_fn(|index| interpolate(&sanitized, index));
    SpectrumFrame::from_cava_bars(interpolated).with_bass_pressure(bass_pressure(&sanitized))
}

fn sanitize_band(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn interpolate(bands: &[f32], output_index: usize) -> f32 {
    match bands.len() {
        0 => 0.0,
        1 => bands[0],
        len => {
            let (left, right, fraction) = band_neighbours(len, output_index);
            bands[left] + (bands[right] - bands[left]) * fraction
        }
    }
}

/// Where one output bar reads from, for an input of `len` bands.
///
/// Split out from [`interpolate`] because this is the part that can leave the
/// slice: it is index arithmetic, testable for any length without allocating
/// the bands themselves. Both ends are clamped, not just the right one — for a
/// long enough input the product below leaves f32's exact-integer range and
/// rounds *above* `len - 1`, which put `left` one past the end.
pub(super) fn band_neighbours(len: usize, output_index: usize) -> (usize, usize, f32) {
    let position = output_index as f32 * (len - 1) as f32 / (SPECTRUM_BAND_COUNT - 1) as f32;
    let left = (position.floor() as usize).min(len - 1);
    let right = (left + 1).min(len - 1);
    (left, right, position.fract())
}

fn bass_pressure(bands: &[f32]) -> BassPressure {
    let bass = &bands[..bands.len().min(BASS_BAND_COUNT)];
    if bass.is_empty() {
        return BassPressure::silent();
    }
    let level = bass.iter().sum::<f32>() / bass.len() as f32;
    let level_dbfs =
        SPECTROGRAM_FLOOR_DBFS + level * (SPECTROGRAM_CEILING_DBFS - SPECTROGRAM_FLOOR_DBFS);
    BassPressure {
        level_dbfs,
        baseline_dbfs: level_dbfs,
        impact: level,
        aura: 0.0,
        kick: level,
        pressure: level,
    }
}
