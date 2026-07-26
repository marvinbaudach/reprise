//! Stateful, platform-neutral CAVA spectrum processing.
//!
//! The frequency layout and temporal processing are an idiomatic Rust port of
//! CAVA's MIT-licensed `cavacore` at commit
//! `4b12c2b043723f42567ddbfd5a516566bdf52316`. Reprise supplies its own
//! pure-Rust FFT and public safety boundary; no CAVA input, output, rendering,
//! threading, or FFTW integration is included.

mod bands;

use thiserror::Error;

use bands::BandPlan;

/// Maximum supported display resolution.
pub const MAX_CAVA_BAR_COUNT: usize = 256;
const DEFAULT_LOW_CUTOFF_HZ: u32 = 50;
const DEFAULT_HIGH_CUTOFF_HZ: u32 = 10_000;

/// Configuration for [`CavaBarProcessor`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CavaConfig {
    pub sample_rate_hz: u32,
    pub bar_count: usize,
    pub low_cutoff_hz: u32,
    pub high_cutoff_hz: u32,
}

impl CavaConfig {
    pub fn new(sample_rate_hz: u32, bar_count: usize) -> Self {
        Self {
            sample_rate_hz,
            bar_count,
            low_cutoff_hz: DEFAULT_LOW_CUTOFF_HZ,
            high_cutoff_hz: DEFAULT_HIGH_CUTOFF_HZ.min(sample_rate_hz / 2),
        }
    }
}

/// Invalid CAVA processor configuration.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum CavaError {
    #[error("sample rate must be between 1 and 384000 Hz")]
    InvalidSampleRate,
    #[error("bar count must be between 1 and 256")]
    InvalidBarCount,
    #[error("cutoff frequencies must be positive and low must be below high")]
    InvalidCutoffRange,
    #[error("high cutoff cannot exceed the Nyquist frequency")]
    HighCutoffAboveNyquist,
}

/// Converts successive mono PCM chunks into bounded visualizer bars.
pub struct CavaBarProcessor {
    config: CavaConfig,
    band_plan: BandPlan,
}

impl CavaBarProcessor {
    pub fn new(config: CavaConfig) -> Result<Self, CavaError> {
        if !(1..=384_000).contains(&config.sample_rate_hz) {
            return Err(CavaError::InvalidSampleRate);
        }
        if !(1..=MAX_CAVA_BAR_COUNT).contains(&config.bar_count) {
            return Err(CavaError::InvalidBarCount);
        }
        if config.low_cutoff_hz == 0
            || config.high_cutoff_hz == 0
            || config.low_cutoff_hz >= config.high_cutoff_hz
        {
            return Err(CavaError::InvalidCutoffRange);
        }
        if config.high_cutoff_hz > config.sample_rate_hz / 2 {
            return Err(CavaError::HighCutoffAboveNyquist);
        }
        let band_plan = BandPlan::new(config)?;
        Ok(Self { config, band_plan })
    }

    pub fn bar_count(&self) -> usize {
        self.config.bar_count
    }

    /// Actual FFT-quantized band boundaries, in Hz.
    pub fn cutoff_frequencies_hz(&self) -> &[f32] {
        self.band_plan.cutoff_frequencies_hz()
    }
}
