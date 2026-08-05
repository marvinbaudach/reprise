//! Pure sound-profile derivation from the stored spectrogram cache.

use crate::spectrogram::{
    TrackSpectrogram, SPECTROGRAM_BAND_COUNT, SPECTROGRAM_CEILING_DBFS, SPECTROGRAM_FLOOR_DBFS,
    SPECTROGRAM_FORMAT_VERSION, SPECTROGRAM_FRAME_RATE_HZ,
};

/// The layout revision of the stored [`SoundFeatures`] blob. Bump it in the
/// same change that alters `to_blob`/`from_blob`.
pub(crate) const SOUND_FEATURE_LAYOUT_VERSION: i64 = 1;

/// How many layout revisions fit under one spectrogram format.
const SOUND_FEATURE_LAYOUT_STRIDE: i64 = 100;

/// Folds both inputs of a stored profile into one stamp.
pub(crate) const fn sound_features_stamp(spectrogram_format: i64, layout: i64) -> i64 {
    spectrogram_format * SOUND_FEATURE_LAYOUT_STRIDE + layout
}

/// The stamp written into and filtered on in `track_sound_features.format_version`.
///
/// **Invariant:** it changes when *either* the spectrogram format or the derived
/// blob layout changes. Derived profiles are computed from spectrogram bytes
/// with this layout, so a spectrogram bump still has to invalidate them — and a
/// silent layout change must not leave old rows matching the SQL filter, where
/// `from_blob` would then reject every one of them while the backfill, gated on
/// the same stamp, sees nothing missing. Both inputs feed one monotone number so
/// the filter stays a single column comparison.
pub(crate) const SOUND_FEATURES_FORMAT_VERSION: i64 =
    sound_features_stamp(SPECTROGRAM_FORMAT_VERSION, SOUND_FEATURE_LAYOUT_VERSION);

const BASS_BAND_COUNT: usize = 8;
const MIN_TEMPO_BPM: f32 = 60.0;
const MAX_TEMPO_BPM: f32 = 200.0;
const MIN_TEMPO_CORRELATION: f32 = 0.2;
const TEMPO_BACKGROUND_RATIO: f32 = 1.4;
const ENERGY_EPSILON: f32 = 1.0e-20;

/// A compact, reproducible description derived from one stored spectrogram.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundFeatures {
    /// Mean linear energy per band, normalized to unit L2 length.
    pub band_mean: [f32; SPECTROGRAM_BAND_COUNT],
    /// Mean frame centroid on the stored band-index axis.
    pub centroid_mean: f32,
    /// Population variance of frame centroids on the stored band-index axis.
    pub centroid_var: f32,
    /// Loudest frame energy relative to mean frame energy, in dB.
    pub frame_crest_db: f32,
    /// Onset-autocorrelation estimate. Absence means no stable peak was found.
    pub tempo: Option<f32>,
}

impl SoundFeatures {
    const SCALAR_COUNT: usize = SPECTROGRAM_BAND_COUNT + 3;
    const BLOB_LEN: usize = Self::SCALAR_COUNT * size_of::<f32>() + 1 + size_of::<f32>();

    pub(crate) fn to_blob(&self) -> Vec<u8> {
        let mut blob = Vec::with_capacity(Self::BLOB_LEN);
        for value in self.band_mean.iter().copied().chain([
            self.centroid_mean,
            self.centroid_var,
            self.frame_crest_db,
        ]) {
            blob.extend_from_slice(&value.to_le_bytes());
        }
        blob.push(u8::from(self.tempo.is_some()));
        blob.extend_from_slice(&self.tempo.unwrap_or(0.0).to_le_bytes());
        blob
    }

    pub(crate) fn from_blob(blob: &[u8]) -> Result<Self, SoundFeaturesFormatError> {
        if blob.len() != Self::BLOB_LEN {
            return Err(SoundFeaturesFormatError::WrongLength(blob.len()));
        }
        let mut offset = 0;
        let mut next_f32 = || {
            let bytes: [u8; 4] = blob[offset..offset + 4].try_into().expect("checked length");
            offset += 4;
            f32::from_le_bytes(bytes)
        };
        let band_mean = std::array::from_fn(|_| next_f32());
        let centroid_mean = next_f32();
        let centroid_var = next_f32();
        let frame_crest_db = next_f32();
        let tempo_marker = blob[offset];
        offset += 1;
        let tempo_value =
            f32::from_le_bytes(blob[offset..offset + 4].try_into().expect("checked length"));
        let tempo = match tempo_marker {
            0 => None,
            1 => Some(tempo_value),
            marker => return Err(SoundFeaturesFormatError::InvalidTempoMarker(marker)),
        };
        let features = Self {
            band_mean,
            centroid_mean,
            centroid_var,
            frame_crest_db,
            tempo,
        };
        if features
            .band_mean
            .iter()
            .copied()
            .chain([
                features.centroid_mean,
                features.centroid_var,
                features.frame_crest_db,
            ])
            .chain(features.tempo)
            .any(|value| !value.is_finite())
        {
            return Err(SoundFeaturesFormatError::NonFinite);
        }
        Ok(features)
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SoundFeaturesFormatError {
    #[error("sound feature blob has {0} bytes, expected 113")]
    WrongLength(usize),
    #[error("sound feature blob has invalid tempo marker {0}")]
    InvalidTempoMarker(u8),
    #[error("sound feature blob contains a non-finite value")]
    NonFinite,
}

/// Derives a sound profile without file or database access.
pub fn derive_sound_features(spectrogram: &TrackSpectrogram) -> SoundFeatures {
    if spectrogram.frame_count() == 0 {
        return neutral_features();
    }

    let mut band_sums = [0.0_f32; SPECTROGRAM_BAND_COUNT];
    let mut centroids = Vec::with_capacity(spectrogram.frame_count());
    let mut frame_energy = Vec::with_capacity(spectrogram.frame_count());
    let mut bass_energy = Vec::with_capacity(spectrogram.frame_count());

    for frame_index in 0..spectrogram.frame_count() {
        let Some(frame) = spectrogram.frame(frame_index) else {
            continue;
        };
        let mut total = 0.0;
        let mut weighted_index = 0.0;
        let mut bass = 0.0;
        for (band, cell) in frame.iter().copied().enumerate() {
            let energy = cell_energy(cell);
            band_sums[band] += energy;
            total += energy;
            weighted_index += band as f32 * energy;
            if band < BASS_BAND_COUNT {
                bass += energy;
            }
        }
        frame_energy.push(total);
        bass_energy.push(bass);
        centroids.push((total > ENERGY_EPSILON).then_some(weighted_index / total));
    }

    let frame_count = spectrogram.frame_count() as f32;
    let mut band_mean = band_sums.map(|sum| sum / frame_count);
    let norm = band_mean
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if norm > ENERGY_EPSILON {
        for value in &mut band_mean {
            *value /= norm;
        }
    }

    let valid_centroids: Vec<f32> = centroids.into_iter().flatten().collect();
    let centroid_mean = mean(&valid_centroids).unwrap_or(0.0);
    let centroid_var = if valid_centroids.is_empty() {
        0.0
    } else {
        valid_centroids
            .iter()
            .map(|value| (value - centroid_mean).powi(2))
            .sum::<f32>()
            / valid_centroids.len() as f32
    };

    SoundFeatures {
        band_mean,
        centroid_mean,
        centroid_var,
        frame_crest_db: frame_crest_db(&frame_energy),
        tempo: estimate_tempo(&bass_energy),
    }
}

fn neutral_features() -> SoundFeatures {
    SoundFeatures {
        band_mean: [0.0; SPECTROGRAM_BAND_COUNT],
        centroid_mean: 0.0,
        centroid_var: 0.0,
        frame_crest_db: 0.0,
        tempo: None,
    }
}

fn cell_energy(cell: u8) -> f32 {
    if cell == 0 {
        return 0.0;
    }
    let unit = f32::from(cell) / 255.0;
    let dbfs = SPECTROGRAM_FLOOR_DBFS + unit * (SPECTROGRAM_CEILING_DBFS - SPECTROGRAM_FLOOR_DBFS);
    10.0_f32.powf(dbfs / 10.0)
}

fn mean(values: &[f32]) -> Option<f32> {
    (!values.is_empty()).then(|| values.iter().sum::<f32>() / values.len() as f32)
}

fn frame_crest_db(frame_energy: &[f32]) -> f32 {
    let Some(average) = mean(frame_energy) else {
        return 0.0;
    };
    let peak = frame_energy.iter().copied().fold(0.0_f32, f32::max);
    if average <= ENERGY_EPSILON || peak <= ENERGY_EPSILON {
        0.0
    } else {
        10.0 * (peak / average).log10()
    }
}

fn estimate_tempo(bass_energy: &[f32]) -> Option<f32> {
    if bass_energy.len() < minimum_tempo_frames() {
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
    let energy = centered.iter().map(|value| value * value).sum::<f32>();
    if energy <= ENERGY_EPSILON {
        return None;
    }

    let min_lag = ((SPECTROGRAM_FRAME_RATE_HZ as f32 * 60.0) / MAX_TEMPO_BPM).ceil() as usize;
    let max_lag = ((SPECTROGRAM_FRAME_RATE_HZ as f32 * 60.0) / MIN_TEMPO_BPM).floor() as usize;
    let correlations: Vec<(usize, f32)> = (min_lag..=max_lag)
        .filter_map(|lag| normalized_autocorrelation(&centered, lag).map(|value| (lag, value)))
        .collect();
    let &(best_lag, best) = correlations
        .iter()
        .max_by(|left, right| left.1.total_cmp(&right.1))?;
    let background = correlations
        .iter()
        .filter(|(lag, _)| *lag != best_lag)
        .map(|(_, value)| value.max(0.0))
        .sum::<f32>()
        / correlations.len().saturating_sub(1).max(1) as f32;
    if best < MIN_TEMPO_CORRELATION || best < background * TEMPO_BACKGROUND_RATIO {
        return None;
    }
    Some(SPECTROGRAM_FRAME_RATE_HZ as f32 * 60.0 / best_lag as f32)
}

fn minimum_tempo_frames() -> usize {
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
