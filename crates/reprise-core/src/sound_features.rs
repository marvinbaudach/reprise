//! Pure sound-profile derivation from the stored spectrogram cache.

use crate::sound_rhythm::{derive_rhythm_features, estimate_tempo, RhythmFeatures};
use crate::spectrogram::{
    cell_energy, TrackSpectrogram, SPECTROGRAM_BAND_COUNT, SPECTROGRAM_FORMAT_VERSION,
};

/// The revision of the stored [`SoundFeatures`] blob. Bump it in the same
/// change that alters `to_blob`/`from_blob` — and equally when a derivation
/// starts producing a *different number in the same field*. A stored profile
/// is a cache of what the derivation says today; a value whose meaning has
/// changed is as stale as one whose offset has moved, and only this number
/// tells the backfill to compute it again.
pub(crate) const SOUND_FEATURE_LAYOUT_VERSION: i64 = 3;

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

const ENERGY_EPSILON: f32 = 1.0e-20;

/// A compact, reproducible description derived from one stored spectrogram.
///
/// Two halves that answer different questions: the fields below describe how
/// the track is equalized and mastered, [`RhythmFeatures`] describes how it
/// moves. The first finds the same production, the second is what carries
/// genre — see the module comment of `sound_rhythm`.
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
    /// Temporal structure: per-band flux, onset rate, flux mean and variation,
    /// pulse strength.
    pub rhythm: RhythmFeatures,
    /// Onset-autocorrelation estimate. Absence means no stable peak was found.
    pub tempo: Option<f32>,
}

impl SoundFeatures {
    /// `band_mean` and `band_flux` plus centroid mean, centroid variance,
    /// frame crest, onset rate, flux mean, flux variation, pulse strength.
    const SCALAR_COUNT: usize = 2 * SPECTROGRAM_BAND_COUNT + 7;
    const BLOB_LEN: usize = Self::SCALAR_COUNT * size_of::<f32>() + 1 + size_of::<f32>();

    /// Every stored scalar as a mutable slot, in blob order — the one place
    /// that order is written down. Writing copies the slots out, reading fills
    /// them in, and the finiteness check walks the same list, so a reorder here
    /// moves all three together and they cannot drift apart.
    fn scalar_slots(&mut self) -> impl Iterator<Item = &mut f32> + '_ {
        self.band_mean
            .iter_mut()
            .chain([
                &mut self.centroid_mean,
                &mut self.centroid_var,
                &mut self.frame_crest_db,
            ])
            .chain(self.rhythm.band_flux.iter_mut())
            .chain([
                &mut self.rhythm.onset_rate,
                &mut self.rhythm.flux_mean,
                &mut self.rhythm.flux_variation,
                &mut self.rhythm.pulse_strength,
            ])
    }

    /// Whether any stored value is one no reader may accept. The same question
    /// on both sides of the blob, asked over the same slot list.
    fn has_non_finite(&mut self) -> bool {
        // Read out before the slot walk borrows the whole profile.
        let tempo_is_non_finite = self.tempo.is_some_and(|tempo| !tempo.is_finite());
        tempo_is_non_finite || self.scalar_slots().any(|value| !value.is_finite())
    }

    /// The stored bytes, or [`SoundFeaturesFormatError::NonFinite`] for a
    /// profile that must not reach the database.
    ///
    /// Refusing here is what keeps the check symmetric: `from_blob` rejects a
    /// non-finite row on every read, but the pending-work gate only asks
    /// whether a row *exists*, so a row written with such a value would be
    /// skipped with a warning forever and never derived again.
    pub(crate) fn to_blob(&self) -> Result<Vec<u8>, SoundFeaturesFormatError> {
        // The blob order lives on mutable slots, so the write side walks its
        // own copy rather than keeping a second copy of the order.
        let mut slots = self.clone();
        if slots.has_non_finite() {
            return Err(SoundFeaturesFormatError::NonFinite);
        }
        let mut blob = Vec::with_capacity(Self::BLOB_LEN);
        for value in slots.scalar_slots() {
            blob.extend_from_slice(&value.to_le_bytes());
        }
        blob.push(u8::from(self.tempo.is_some()));
        blob.extend_from_slice(&self.tempo.unwrap_or(0.0).to_le_bytes());
        Ok(blob)
    }

    pub(crate) fn from_blob(blob: &[u8]) -> Result<Self, SoundFeaturesFormatError> {
        if blob.len() != Self::BLOB_LEN {
            return Err(SoundFeaturesFormatError::WrongLength { actual: blob.len() });
        }
        let mut offset = 0;
        let mut features = neutral_features();
        for slot in features.scalar_slots() {
            let bytes: [u8; 4] = blob[offset..offset + 4].try_into().expect("checked length");
            *slot = f32::from_le_bytes(bytes);
            offset += 4;
        }
        let tempo_marker = blob[offset];
        offset += 1;
        let tempo_value =
            f32::from_le_bytes(blob[offset..offset + 4].try_into().expect("checked length"));
        features.tempo = match tempo_marker {
            0 => None,
            1 => Some(tempo_value),
            marker => return Err(SoundFeaturesFormatError::InvalidTempoMarker(marker)),
        };
        if features.has_non_finite() {
            return Err(SoundFeaturesFormatError::NonFinite);
        }
        Ok(features)
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SoundFeaturesFormatError {
    #[error(
        "sound feature blob has {actual} bytes, expected {}",
        SoundFeatures::BLOB_LEN
    )]
    WrongLength { actual: usize },
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

    for frame_index in 0..spectrogram.frame_count() {
        let Some(frame) = spectrogram.frame(frame_index) else {
            continue;
        };
        let mut total = 0.0;
        let mut weighted_index = 0.0;
        for (band, cell) in frame.iter().copied().enumerate() {
            let energy = cell_energy(cell);
            band_sums[band] += energy;
            total += energy;
            weighted_index += band as f32 * energy;
        }
        frame_energy.push(total);
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
        rhythm: derive_rhythm_features(spectrogram),
        tempo: estimate_tempo(spectrogram),
    }
}

/// The profile of a track with nothing to describe — and the blank slate
/// `from_blob` fills slot by slot.
fn neutral_features() -> SoundFeatures {
    SoundFeatures {
        band_mean: [0.0; SPECTROGRAM_BAND_COUNT],
        centroid_mean: 0.0,
        centroid_var: 0.0,
        frame_crest_db: 0.0,
        rhythm: RhythmFeatures::still(),
        tempo: None,
    }
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
