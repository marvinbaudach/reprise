//! Portable, raw spectrogram rendering data.

use crate::playback::spectral::{
    absolute_dbfs_to_byte, BandPlan, FftWorkspace, ABSOLUTE_DB_CEILING, ABSOLUTE_DB_FLOOR,
};

/// Number of logarithmic frequency bands stored for every frame.
pub const SPECTROGRAM_BAND_COUNT: usize = 24;
/// Version of the fixed frame-major byte format stored and transferred by Reprise.
pub const SPECTROGRAM_FORMAT_VERSION: i64 = 1;
/// PCM rate consumed by the offline rendering-data producer.
pub const SPECTROGRAM_SAMPLE_RATE_HZ: u32 = 32_000;
/// Stored time resolution.
pub const SPECTROGRAM_FRAME_RATE_HZ: u32 = 20;
/// Lower edge of the first logarithmic band.
pub const SPECTROGRAM_LOW_HZ: u32 = 20;
/// Upper edge of the final logarithmic band.
pub const SPECTROGRAM_HIGH_HZ: u32 = 16_000;
/// Absolute RMS dBFS floor; quieter energy is stored as black.
pub const SPECTROGRAM_FLOOR_DBFS: f32 = ABSOLUTE_DB_FLOOR;
/// Absolute RMS dBFS ceiling; louder energy saturates the cell.
pub const SPECTROGRAM_CEILING_DBFS: f32 = ABSOLUTE_DB_CEILING;

const SAMPLES_PER_FRAME: usize =
    SPECTROGRAM_SAMPLE_RATE_HZ as usize / SPECTROGRAM_FRAME_RATE_HZ as usize;
const MAIN_FFT_SIZE: usize = 4_096;
const BASS_FFT_SIZE: usize = 16_384;
const BASS_SPLIT_HZ: f32 = 100.0;

/// One complete frame-major spectrogram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackSpectrogram {
    cells: Vec<u8>,
}

impl TrackSpectrogram {
    pub fn empty() -> Self {
        Self { cells: Vec::new() }
    }

    pub fn from_cells(cells: Vec<u8>) -> Result<Self, SpectrogramFormatError> {
        if !cells.len().is_multiple_of(SPECTROGRAM_BAND_COUNT) {
            return Err(SpectrogramFormatError::IncompleteFrame {
                byte_count: cells.len(),
            });
        }
        Ok(Self { cells })
    }

    pub fn frame_count(&self) -> usize {
        self.cells.len() / SPECTROGRAM_BAND_COUNT
    }

    pub fn frame(&self, index: usize) -> Option<&[u8]> {
        let start = index.checked_mul(SPECTROGRAM_BAND_COUNT)?;
        self.cells
            .get(start..start.checked_add(SPECTROGRAM_BAND_COUNT)?)
    }

    pub fn cells(&self) -> &[u8] {
        &self.cells
    }

    /// Upper edge of the highest stored band with energy above the absolute
    /// spectrogram floor. This is display metadata derived from the existing
    /// render cache, not a second analysis pass.
    pub fn occupied_upper_hz(&self) -> Option<u32> {
        let highest = (0..SPECTROGRAM_BAND_COUNT).rev().find(|band| {
            self.cells
                .iter()
                .skip(*band)
                .step_by(SPECTROGRAM_BAND_COUNT)
                .any(|cell| *cell > 0)
        })?;
        let low = f64::from(SPECTROGRAM_LOW_HZ).ln();
        let high = f64::from(SPECTROGRAM_HIGH_HZ).ln();
        let edge = low + (high - low) * (highest + 1) as f64 / SPECTROGRAM_BAND_COUNT as f64;
        Some(edge.exp().round() as u32)
    }

    /// The seek bar's colour curve: one normalized spectral position per
    /// `bucket`, `0` at the track's own bass end and `255` at its treble end.
    ///
    /// Derived from the stored cells rather than measured again. The frequency
    /// content of a track is decided once, by the producer above; a second
    /// analysis beside it would be a second answer to the same question, and
    /// the two would drift.
    ///
    /// Normalized per track (5th to 95th percentile, widened to at least
    /// [`MIN_SPAN_OCTAVES`]), not against an absolute frequency range.
    /// Measured over a real library on 2026-08-05, an absolute axis put every
    /// sampled track inside one narrow band and the seek bar read as a single
    /// colour. What has to be visible is the travel within one track, so
    /// comparability between two tracks is given up — the same trade the
    /// height mapping already makes.
    pub fn centroid_curve(&self, buckets: usize) -> Vec<u8> {
        let frame_count = self.frame_count();
        if frame_count == 0 || buckets == 0 {
            return Vec::new();
        }
        let centres = band_centre_octaves();

        // Octave position per bucket, weighted by each frame's own loudness so
        // a near-silent frame cannot colour a bucket it barely occupies.
        let octaves: Vec<Option<f64>> = (0..buckets)
            .map(|bucket| {
                let start = bucket * frame_count / buckets;
                let end = (((bucket + 1) * frame_count / buckets).max(start + 1)).min(frame_count);
                let mut weighted = 0.0;
                let mut weight = 0.0;
                for frame in start..end {
                    let Some(cells) = self.frame(frame) else {
                        continue;
                    };
                    for (cell, centre) in cells.iter().zip(&centres) {
                        let amplitude = cell_amplitude(*cell);
                        weighted += centre * amplitude;
                        weight += amplitude;
                    }
                }
                (weight > CELL_ENERGY_EPSILON).then(|| weighted / weight)
            })
            .collect();

        let Some((low, high)) = percentile_window(&octaves) else {
            return vec![NEUTRAL_CENTROID; buckets];
        };
        let mut last_valid = None;
        octaves
            .iter()
            .map(|octave| match octave {
                Some(octave) => {
                    let value =
                        ((((octave - low) / (high - low)).clamp(0.0, 1.0) * 255.0).round()) as u8;
                    last_valid = Some(value);
                    value
                }
                // True silence carries the last colour rather than a jump to
                // one end: a pause is not a statement about frequency.
                None => last_valid.unwrap_or(NEUTRAL_CENTROID),
            })
            .collect()
    }
}

/// Colour position of a track with no usable spectral content at all.
const NEUTRAL_CENTROID: u8 = 128;
/// Below this summed cell amplitude a bucket counts as silent.
const CELL_ENERGY_EPSILON: f64 = 1.0e-6;
/// Narrowest colour axis a single track may span, in octaves.
///
/// Without it, a track that holds one spectral position would have its own
/// measurement jitter stretched across the whole axis and flicker between the
/// two ends.
const MIN_SPAN_OCTAVES: f64 = 0.5;

/// Log-frequency centre of every stored band, in octaves.
///
/// The bands tile [`SPECTROGRAM_LOW_HZ`]..[`SPECTROGRAM_HIGH_HZ`] on a
/// logarithmic scale, so a band's centre is the geometric mean of its edges —
/// which on a log axis is simply the midpoint.
fn band_centre_octaves() -> Vec<f64> {
    let low = f64::from(SPECTROGRAM_LOW_HZ).log2();
    let high = f64::from(SPECTROGRAM_HIGH_HZ).log2();
    let step = (high - low) / SPECTROGRAM_BAND_COUNT as f64;
    (0..SPECTROGRAM_BAND_COUNT)
        .map(|band| low + step * (band as f64 + 0.5))
        .collect()
}

/// A stored cell back to a linear amplitude weight.
fn cell_amplitude(cell: u8) -> f64 {
    if cell == 0 {
        return 0.0;
    }
    let unit = f64::from(cell) / 255.0;
    let dbfs = f64::from(SPECTROGRAM_FLOOR_DBFS)
        + unit * f64::from(SPECTROGRAM_CEILING_DBFS - SPECTROGRAM_FLOOR_DBFS);
    10.0_f64.powf(dbfs / 20.0)
}

/// The octave window a track's own axis spans.
fn percentile_window(octaves: &[Option<f64>]) -> Option<(f64, f64)> {
    let mut sorted: Vec<f64> = octaves.iter().flatten().copied().collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(f64::total_cmp);
    let at = |p: f64| sorted[(((sorted.len() - 1) as f64) * p).round() as usize];
    let (low, high) = (at(0.05), at(0.95));
    let deficit = MIN_SPAN_OCTAVES - (high - low);
    if deficit > 0.0 {
        Some((low - deficit / 2.0, high + deficit / 2.0))
    } else {
        Some((low, high))
    }
}

#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq)]
pub enum SpectrogramFormatError {
    #[error("spectrogram has {byte_count} bytes, not a whole number of 24-band frames")]
    IncompleteFrame { byte_count: usize },
}

/// Scanner-owned source identity captured before a rendering-data decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackSourceFingerprint {
    pub mtime_seconds: i64,
    pub size_bytes: i64,
    pub device: Option<i64>,
    pub inode: Option<i64>,
}

/// Streaming producer for [`TrackSpectrogram`].
pub struct SpectrogramAccumulator {
    band_plan: BandPlan,
    main_fft: FftWorkspace,
    bass_fft: FftWorkspace,
    history: Vec<f32>,
    main_frame: Vec<f32>,
    bass_frame: Vec<f32>,
    write_index: usize,
    samples_seen: usize,
    samples_since_frame: usize,
    cells: Vec<u8>,
}

impl SpectrogramAccumulator {
    pub fn new() -> Self {
        Self {
            band_plan: BandPlan::new(
                SPECTROGRAM_SAMPLE_RATE_HZ,
                SPECTROGRAM_BAND_COUNT,
                SPECTROGRAM_LOW_HZ,
                SPECTROGRAM_HIGH_HZ,
                MAIN_FFT_SIZE,
                BASS_FFT_SIZE,
                BASS_SPLIT_HZ,
            )
            .expect("the fixed spectrogram format has a valid band plan"),
            main_fft: FftWorkspace::new(MAIN_FFT_SIZE),
            bass_fft: FftWorkspace::new(BASS_FFT_SIZE),
            history: vec![0.0; BASS_FFT_SIZE],
            main_frame: vec![0.0; MAIN_FFT_SIZE],
            bass_frame: vec![0.0; BASS_FFT_SIZE],
            write_index: 0,
            samples_seen: 0,
            samples_since_frame: 0,
            cells: Vec::new(),
        }
    }

    pub fn push(&mut self, mono_samples: &[f32]) {
        for sample in mono_samples {
            self.history[self.write_index] = if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            self.write_index = (self.write_index + 1) % self.history.len();
            self.samples_seen = self.samples_seen.saturating_add(1);
            self.samples_since_frame += 1;
            if self.samples_since_frame == SAMPLES_PER_FRAME {
                self.close_frame();
                self.samples_since_frame = 0;
            }
        }
    }

    pub fn finish(mut self) -> TrackSpectrogram {
        if self.samples_since_frame > 0 {
            self.close_frame();
        }
        TrackSpectrogram { cells: self.cells }
    }

    fn close_frame(&mut self) {
        self.copy_ordered_history();
        self.main_frame
            .copy_from_slice(&self.bass_frame[BASS_FFT_SIZE - MAIN_FFT_SIZE..BASS_FFT_SIZE]);
        self.main_fft.process(&self.main_frame);
        self.bass_fft.process(&self.bass_frame);

        self.cells.extend(self.band_plan.bands().iter().map(|band| {
            let workspace = if band.use_bass_fft {
                &self.bass_fft
            } else {
                &self.main_fft
            };
            absolute_dbfs_to_byte(workspace.band_rms_dbfs(band.bins()))
        }));
    }

    fn copy_ordered_history(&mut self) {
        let available = self.samples_seen.min(BASS_FFT_SIZE);
        self.bass_frame.fill(0.0);
        let start = (self.write_index + self.history.len() - available) % self.history.len();
        for offset in 0..available {
            self.bass_frame[BASS_FFT_SIZE - available + offset] =
                self.history[(start + offset) % self.history.len()];
        }
    }
}

impl Default for SpectrogramAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "spectrogram_tests.rs"]
mod tests;
