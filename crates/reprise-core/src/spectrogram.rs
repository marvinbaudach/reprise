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
