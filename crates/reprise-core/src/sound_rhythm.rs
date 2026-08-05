//! Temporal structure derived from the stored spectrogram.
//!
//! Band means, spectral centroid and frame crest describe how a track is
//! *equalized* and how it was mastered. Measured over a real 1793-track
//! library on 2026-08-05 that found the same album 11.98x above chance and the
//! same genre 1.07x — an EQ and mastering fingerprint, nothing more. What
//! separates genres is movement: where in the spectrum it sits, how often it
//! happens, how evenly, and how metronomic it is.
//!
//! Everything here is a pure function of [`TrackSpectrogram`] — 24 logarithmic
//! bands at 20 fps, one `u8` of absolute dBFS per cell. No new column, no
//! second decode: the stored rendering data already carries the movement, it
//! was simply never read.

use crate::spectrogram::{
    cell_dbfs, cell_energy, TrackSpectrogram, SPECTROGRAM_BAND_COUNT, SPECTROGRAM_FRAME_RATE_HZ,
};

/// How many of the lowest bands carry the beat for the pulse estimate.
/// Eight of 24 logarithmic bands from 20 Hz reach roughly 200 Hz — kick,
/// bass and the low end of the snare.
const BASS_BAND_COUNT: usize = 8;
/// Slowest period the pulse window looks for.
const MIN_TEMPO_BPM: f32 = 60.0;
/// Fastest period the pulse window looks for.
const MAX_TEMPO_BPM: f32 = 200.0;
/// Below this correlation the strongest lag is not a beat.
const MIN_TEMPO_CORRELATION: f32 = 0.2;
/// … and it must also stand this far above the other lags.
const TEMPO_BACKGROUND_RATIO: f32 = 1.4;
/// Half-width of the adaptive onset window: ±0.5 s at 20 fps.
const ONSET_WINDOW_FRAMES: usize = 10;
/// A peak counts as an onset only this far above its own local mean.
const ONSET_THRESHOLD_FACTOR: f32 = 1.5;
/// … plus this absolute floor, summed over all bands in dB. Without it a
/// silent track's quantization noise would peak-pick into a beat.
const ONSET_FLOOR_DB: f32 = 1.0;
/// Two onsets closer than this are one onset: at 20 fps a single transient
/// smears over two frames, and 100 ms is the shortest interval the grid can
/// honestly resolve.
const ONSET_MIN_GAP_FRAMES: usize = 2;
const ENERGY_EPSILON: f32 = 1.0e-20;
/// Below this mean flux (in summed dB per frame) a track does not move enough
/// for a relative measure of its unevenness to mean anything.
const FLUX_EPSILON_DB: f32 = 1.0e-6;

/// How a track moves, as opposed to how it is equalized.
#[derive(Debug, Clone, PartialEq)]
pub struct RhythmFeatures {
    /// Mean positive frame-to-frame rise per band, in dB, normalized to unit
    /// L2 length. The rhythmic counterpart of `band_mean`: it says *where* the
    /// movement sits — a busy kick band against steady guitars.
    pub band_flux: [f32; SPECTROGRAM_BAND_COUNT],
    /// Peak-picked onsets per second. A blast beat against a ballad.
    pub onset_rate: f32,
    /// Mean summed positive flux per frame, in dB. How much the sound moves
    /// at all.
    pub flux_mean: f32,
    /// Coefficient of variation of that flux over the frames. How unevenly it
    /// moves: a steady loop against a track that stops and starts.
    pub flux_variation: f32,
    /// Normalized autocorrelation peak of the bass onset envelope inside the
    /// musical period window, `0.0..=1.0`. How metronomic the track is.
    pub pulse_strength: f32,
}

impl RhythmFeatures {
    /// A track that does not move: silence, or one unchanging tone.
    pub fn still() -> Self {
        Self {
            band_flux: [0.0; SPECTROGRAM_BAND_COUNT],
            onset_rate: 0.0,
            flux_mean: 0.0,
            flux_variation: 0.0,
            pulse_strength: 0.0,
        }
    }
}

/// Derives the temporal profile without file or database access.
pub fn derive_rhythm_features(spectrogram: &TrackSpectrogram) -> RhythmFeatures {
    let Some((band_rise, frame_flux)) = positive_flux(spectrogram) else {
        return RhythmFeatures::still();
    };
    let flux_mean = mean(&frame_flux).unwrap_or(0.0);
    RhythmFeatures {
        band_flux: l2_normalized(band_rise),
        onset_rate: onset_rate(&frame_flux),
        flux_mean,
        flux_variation: variation(&frame_flux, flux_mean),
        pulse_strength: bass_pulse_peak(&bass_frame_energy(spectrogram))
            .map_or(0.0, |peak| peak.correlation.clamp(0.0, 1.0)),
    }
}

/// Tempo in BPM, or `None` when no lag in the musical window stands out from
/// the rest.
///
/// Shares [`bass_pulse_peak`] with `pulse_strength`: the tempo is that peak's
/// period, its strength is that peak's height. Two answers from one question,
/// never two questions.
pub fn estimate_tempo(spectrogram: &TrackSpectrogram) -> Option<f32> {
    let peak = bass_pulse_peak(&bass_frame_energy(spectrogram))?;
    if peak.correlation < MIN_TEMPO_CORRELATION
        || peak.correlation < peak.background * TEMPO_BACKGROUND_RATIO
    {
        return None;
    }
    Some(SPECTROGRAM_FRAME_RATE_HZ as f32 * 60.0 / peak.lag_frames as f32)
}

/// `(summed positive rise per band, summed positive rise per frame)`, both in
/// dB, or `None` for a spectrogram with no frame pair to compare.
fn positive_flux(
    spectrogram: &TrackSpectrogram,
) -> Option<([f32; SPECTROGRAM_BAND_COUNT], Vec<f32>)> {
    let frame_count = spectrogram.frame_count();
    if frame_count < 2 {
        return None;
    }
    let mut band_rise = [0.0_f32; SPECTROGRAM_BAND_COUNT];
    let mut frame_flux = Vec::with_capacity(frame_count - 1);
    for index in 1..frame_count {
        let (Some(previous), Some(current)) =
            (spectrogram.frame(index - 1), spectrogram.frame(index))
        else {
            continue;
        };
        let mut total = 0.0;
        for (band, (before, after)) in previous.iter().zip(current).enumerate() {
            // Only the rise: a decay is the same event's tail, and counting it
            // would double every onset and blur the two apart.
            let rise = (cell_dbfs(*after) - cell_dbfs(*before)).max(0.0);
            band_rise[band] += rise;
            total += rise;
        }
        frame_flux.push(total);
    }
    if frame_flux.is_empty() {
        return None;
    }
    let divisor = frame_flux.len() as f32;
    for value in &mut band_rise {
        *value /= divisor;
    }
    Some((band_rise, frame_flux))
}

/// The comparison shape: only the direction of the flux vector carries, its
/// length is already the separate `flux_mean` scalar.
fn l2_normalized(mut vector: [f32; SPECTROGRAM_BAND_COUNT]) -> [f32; SPECTROGRAM_BAND_COUNT] {
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > ENERGY_EPSILON {
        for value in &mut vector {
            *value /= norm;
        }
    }
    vector
}

/// Onsets per second, peak-picked against an adaptive threshold.
fn onset_rate(frame_flux: &[f32]) -> f32 {
    let seconds = frame_flux.len() as f32 / SPECTROGRAM_FRAME_RATE_HZ as f32;
    if seconds <= 0.0 {
        return 0.0;
    }
    count_onsets(frame_flux) as f32 / seconds
}

/// Local maxima of the flux envelope that stand above the mean of their own
/// neighbourhood.
///
/// Adaptive rather than absolute: a quiet passage has quiet onsets, and a
/// fixed threshold would either miss them or count a loud passage's texture as
/// a beat.
fn count_onsets(frame_flux: &[f32]) -> usize {
    let mut count = 0;
    let mut previous_onset: Option<usize> = None;
    for (index, value) in frame_flux.iter().copied().enumerate() {
        let rising = index == 0 || value >= frame_flux[index - 1];
        let falling = index + 1 == frame_flux.len() || value > frame_flux[index + 1];
        if !rising || !falling {
            continue;
        }
        let start = index.saturating_sub(ONSET_WINDOW_FRAMES);
        let end = (index + ONSET_WINDOW_FRAMES + 1).min(frame_flux.len());
        let local = frame_flux[start..end].iter().sum::<f32>() / (end - start) as f32;
        if value <= ONSET_THRESHOLD_FACTOR * local + ONSET_FLOOR_DB {
            continue;
        }
        if previous_onset.is_some_and(|last| index - last < ONSET_MIN_GAP_FRAMES) {
            continue;
        }
        previous_onset = Some(index);
        count += 1;
    }
    count
}

/// Standard deviation over the mean — unitless, so a loud track and a quiet
/// one with the same shape land on the same number.
fn variation(frame_flux: &[f32], flux_mean: f32) -> f32 {
    if flux_mean <= FLUX_EPSILON_DB || frame_flux.is_empty() {
        return 0.0;
    }
    let variance = frame_flux
        .iter()
        .map(|value| (value - flux_mean).powi(2))
        .sum::<f32>()
        / frame_flux.len() as f32;
    variance.sqrt() / flux_mean
}

/// Linear power of the bass bands, frame by frame.
fn bass_frame_energy(spectrogram: &TrackSpectrogram) -> Vec<f32> {
    (0..spectrogram.frame_count())
        .filter_map(|index| spectrogram.frame(index))
        .map(|frame| {
            frame
                .iter()
                .take(BASS_BAND_COUNT)
                .copied()
                .map(cell_energy)
                .sum()
        })
        .collect()
}

/// The strongest periodicity of the bass onset envelope inside the musical
/// period window.
struct PulsePeak {
    lag_frames: usize,
    correlation: f32,
    /// Mean positive correlation of every other lag — what the peak has to
    /// stand out from before it may be called a beat.
    background: f32,
}

fn bass_pulse_peak(bass_energy: &[f32]) -> Option<PulsePeak> {
    if bass_energy.len() < minimum_pulse_frames() {
        return None;
    }
    let onsets: Vec<f32> = std::iter::once(0.0)
        .chain(
            bass_energy
                .windows(2)
                .map(|pair| (pair[1] - pair[0]).max(0.0)),
        )
        .collect();
    let onset_mean = mean(&onsets)?;
    let centered: Vec<f32> = onsets.iter().map(|value| value - onset_mean).collect();
    if centered.iter().map(|value| value * value).sum::<f32>() <= ENERGY_EPSILON {
        return None;
    }

    let min_lag = ((SPECTROGRAM_FRAME_RATE_HZ as f32 * 60.0) / MAX_TEMPO_BPM).ceil() as usize;
    let max_lag = ((SPECTROGRAM_FRAME_RATE_HZ as f32 * 60.0) / MIN_TEMPO_BPM).floor() as usize;
    let correlations: Vec<(usize, f32)> = (min_lag..=max_lag)
        .filter_map(|lag| normalized_autocorrelation(&centered, lag).map(|value| (lag, value)))
        .collect();
    let &(lag_frames, correlation) = correlations
        .iter()
        .max_by(|left, right| left.1.total_cmp(&right.1))?;
    let background = correlations
        .iter()
        .filter(|(lag, _)| *lag != lag_frames)
        .map(|(_, value)| value.max(0.0))
        .sum::<f32>()
        / correlations.len().saturating_sub(1).max(1) as f32;
    Some(PulsePeak {
        lag_frames,
        correlation,
        background,
    })
}

/// Two full periods of the slowest tempo — less than that and the
/// autocorrelation is reading its own edge.
fn minimum_pulse_frames() -> usize {
    ((SPECTROGRAM_FRAME_RATE_HZ as f32 * 60.0) / MIN_TEMPO_BPM) as usize * 2
}

fn normalized_autocorrelation(values: &[f32], lag: usize) -> Option<f32> {
    let pairs = values.len().checked_sub(lag)?;
    if pairs == 0 {
        return None;
    }
    let left = &values[..pairs];
    let right = &values[lag..];
    let numerator = left.iter().zip(right).map(|(a, b)| a * b).sum::<f32>();
    let left_energy = left.iter().map(|value| value * value).sum::<f32>();
    let right_energy = right.iter().map(|value| value * value).sum::<f32>();
    let denominator = (left_energy * right_energy).sqrt();
    (denominator > ENERGY_EPSILON).then_some(numerator / denominator)
}

fn mean(values: &[f32]) -> Option<f32> {
    (!values.is_empty()).then(|| values.iter().sum::<f32>() / values.len() as f32)
}
