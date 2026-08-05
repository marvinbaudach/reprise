//! Stateful, platform-neutral CAVA spectrum processing.
//!
//! The frequency layout and temporal processing are an idiomatic Rust port of
//! CAVA's MIT-licensed `cavacore` at commit
//! `4b12c2b043723f42567ddbfd5a516566bdf52316`. Reprise supplies its own
//! pure-Rust FFT and public safety boundary; no CAVA input, output, rendering,
//! threading, or FFTW integration is included.

mod bands;
mod smoothing;

use thiserror::Error;

use crate::playback::spectral::{
    absolute_dbfs_to_byte, absolute_dbfs_to_unit, BandPlan, FftWorkspace,
};
use bands::{band_plan, fft_size_for_rate};
use smoothing::Smoother;

/// Maximum supported display resolution.
pub const MAX_CAVA_BAR_COUNT: usize = 256;
const DEFAULT_LOW_CUTOFF_HZ: u32 = 50;
const DEFAULT_HIGH_CUTOFF_HZ: u32 = 10_000;
const DEFAULT_NOISE_REDUCTION: f32 = 0.77;
const DEFAULT_NOISE_FLOOR: f32 = 0.04;
const DEFAULT_AUTOSENSITIVITY: u32 = 1;

/// Configuration for [`CavaBarProcessor`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CavaConfig {
    pub sample_rate_hz: u32,
    pub bar_count: usize,
    pub low_cutoff_hz: u32,
    pub high_cutoff_hz: u32,
    pub noise_reduction: f32,
    pub noise_floor: f32,
    pub autosensitivity: u32,
}

impl CavaConfig {
    pub fn new(sample_rate_hz: u32, bar_count: usize) -> Self {
        Self {
            sample_rate_hz,
            bar_count,
            low_cutoff_hz: DEFAULT_LOW_CUTOFF_HZ,
            high_cutoff_hz: DEFAULT_HIGH_CUTOFF_HZ.min(sample_rate_hz / 2),
            noise_reduction: DEFAULT_NOISE_REDUCTION,
            noise_floor: DEFAULT_NOISE_FLOOR,
            autosensitivity: DEFAULT_AUTOSENSITIVITY,
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
    #[error("noise reduction must be finite and between 0 and 1")]
    InvalidNoiseReduction,
    #[error("noise floor must be finite and between 0 and 1")]
    InvalidNoiseFloor,
}

/// Converts successive mono PCM chunks into bounded visualizer bars.
pub struct CavaBarProcessor {
    config: CavaConfig,
    band_plan: BandPlan,
    input_buffer: Vec<f32>,
    main_fft: FftWorkspace,
    bass_fft: FftWorkspace,
    smoother: Smoother,
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
        if !config.noise_reduction.is_finite() || !(0.0..=1.0).contains(&config.noise_reduction) {
            return Err(CavaError::InvalidNoiseReduction);
        }
        if !config.noise_floor.is_finite() || !(0.0..=1.0).contains(&config.noise_floor) {
            return Err(CavaError::InvalidNoiseFloor);
        }
        let band_plan = band_plan(config)?;
        let fft_size = fft_size_for_rate(config.sample_rate_hz);
        Ok(Self {
            config,
            band_plan,
            input_buffer: vec![0.0; fft_size * 2],
            main_fft: FftWorkspace::new(fft_size),
            bass_fft: FftWorkspace::new(fft_size * 2),
            smoother: Smoother::new(
                config.bar_count,
                config.noise_reduction,
                config.noise_floor,
                config.autosensitivity,
            ),
        })
    }

    pub fn bar_count(&self) -> usize {
        self.config.bar_count
    }

    /// Actual FFT-quantized band boundaries, in Hz.
    pub fn cutoff_frequencies_hz(&self) -> &[f32] {
        self.band_plan.cutoff_frequencies_hz()
    }

    /// Adds normalized mono PCM samples and returns one CAVA band value per bar.
    pub fn process(&mut self, mono_samples: &[f32]) -> Vec<f32> {
        let signal_present = self.push_samples(mono_samples);
        self.main_fft
            .process(&self.input_buffer[..self.main_fft.len()]);
        self.bass_fft.process(&self.input_buffer);

        let mut bars: Vec<f32> = self
            .band_plan
            .bands()
            .iter()
            .map(|band| {
                let workspace = if band.use_bass_fft {
                    &self.bass_fft
                } else {
                    &self.main_fft
                };
                absolute_dbfs_to_unit(workspace.band_rms_dbfs(band.bins()))
            })
            .collect();
        let new_samples = mono_samples.len().min(self.input_buffer.len());
        self.smoother.apply(
            &mut bars,
            new_samples,
            self.config.sample_rate_hz,
            signal_present,
        );
        bars
    }

    /// Clears all buffered audio, smoothing history, and dynamic gain state.
    pub fn reset(&mut self) {
        self.input_buffer.fill(0.0);
        self.smoother.reset();
    }

    fn push_samples(&mut self, mono_samples: &[f32]) -> bool {
        let buffer_len = self.input_buffer.len();
        let kept = mono_samples.len().min(buffer_len);
        self.input_buffer.copy_within(..buffer_len - kept, kept);
        let mut sum_squares = 0.0_f64;
        for (target, sample) in self.input_buffer[..kept]
            .iter_mut()
            .rev()
            .zip(mono_samples[mono_samples.len() - kept..].iter())
        {
            let sample = if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            sum_squares += f64::from(sample) * f64::from(sample);
            *target = sample;
        }
        let rms = if kept == 0 {
            0.0
        } else {
            (sum_squares / kept as f64).sqrt() as f32
        };
        let level_dbfs = if rms > 0.0 {
            20.0 * rms.log10()
        } else {
            f32::NEG_INFINITY
        };
        absolute_dbfs_to_byte(level_dbfs) > 0
    }
}
