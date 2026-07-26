//! Stateful, platform-neutral CAVA spectrum processing.
//!
//! The frequency layout and temporal processing are an idiomatic Rust port of
//! CAVA's MIT-licensed `cavacore` at commit
//! `4b12c2b043723f42567ddbfd5a516566bdf52316`. Reprise supplies its own
//! pure-Rust FFT and public safety boundary; no CAVA input, output, rendering,
//! threading, or FFTW integration is included.

mod bands;
mod smoothing;

use std::sync::Arc;

use realfft::{num_complex::Complex32, RealFftPlanner, RealToComplex};
use thiserror::Error;

use bands::{fft_size_for_rate, BandPlan};
use smoothing::Smoother;

/// Maximum supported display resolution.
pub const MAX_CAVA_BAR_COUNT: usize = 256;
const DEFAULT_LOW_CUTOFF_HZ: u32 = 50;
const DEFAULT_HIGH_CUTOFF_HZ: u32 = 10_000;
const DEFAULT_NOISE_REDUCTION: f32 = 0.77;

/// Configuration for [`CavaBarProcessor`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CavaConfig {
    pub sample_rate_hz: u32,
    pub bar_count: usize,
    pub low_cutoff_hz: u32,
    pub high_cutoff_hz: u32,
    pub noise_reduction: f32,
}

impl CavaConfig {
    pub fn new(sample_rate_hz: u32, bar_count: usize) -> Self {
        Self {
            sample_rate_hz,
            bar_count,
            low_cutoff_hz: DEFAULT_LOW_CUTOFF_HZ,
            high_cutoff_hz: DEFAULT_HIGH_CUTOFF_HZ.min(sample_rate_hz / 2),
            noise_reduction: DEFAULT_NOISE_REDUCTION,
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
        let band_plan = BandPlan::new(config)?;
        let fft_size = fft_size_for_rate(config.sample_rate_hz);
        Ok(Self {
            config,
            band_plan,
            input_buffer: vec![0.0; fft_size * 2],
            main_fft: FftWorkspace::new(fft_size),
            bass_fft: FftWorkspace::new(fft_size * 2),
            smoother: Smoother::new(config.bar_count, config.noise_reduction),
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
        self.push_samples(mono_samples);
        self.main_fft
            .process(&self.input_buffer[..self.main_fft.len()]);
        self.bass_fft.process(&self.input_buffer);

        let mut bars: Vec<f32> = self
            .band_plan
            .bands()
            .iter()
            .map(|band| {
                let spectrum = if band.use_bass_fft {
                    self.bass_fft.spectrum()
                } else {
                    self.main_fft.spectrum()
                };
                spectrum[band.lower_bin..=band.upper_bin]
                    .iter()
                    .map(|value| value.norm())
                    .sum::<f32>()
                    * band.equalizer
                    * 65_535.0
            })
            .collect();
        let new_samples = mono_samples.len().min(self.input_buffer.len());
        self.smoother
            .apply(&mut bars, new_samples, self.config.sample_rate_hz);
        bars
    }

    fn push_samples(&mut self, mono_samples: &[f32]) {
        let buffer_len = self.input_buffer.len();
        let kept = mono_samples.len().min(buffer_len);
        self.input_buffer.copy_within(..buffer_len - kept, kept);
        for (target, sample) in self.input_buffer[..kept]
            .iter_mut()
            .rev()
            .zip(mono_samples[mono_samples.len() - kept..].iter())
        {
            *target = if sample.is_finite() { *sample } else { 0.0 };
        }
    }
}

struct FftWorkspace {
    plan: Arc<dyn RealToComplex<f32>>,
    input: Vec<f32>,
    spectrum: Vec<Complex32>,
    scratch: Vec<Complex32>,
    hann: Vec<f32>,
}

impl FftWorkspace {
    fn new(len: usize) -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let plan = planner.plan_fft_forward(len);
        let input = plan.make_input_vec();
        let spectrum = plan.make_output_vec();
        let scratch = plan.make_scratch_vec();
        let hann = (0..len)
            .map(|index| {
                0.5 * (1.0 - (std::f32::consts::TAU * index as f32 / (len - 1) as f32).cos())
            })
            .collect();
        Self {
            plan,
            input,
            spectrum,
            scratch,
            hann,
        }
    }

    fn len(&self) -> usize {
        self.input.len()
    }

    fn process(&mut self, samples: &[f32]) {
        for ((target, sample), multiplier) in self
            .input
            .iter_mut()
            .zip(samples.iter())
            .zip(self.hann.iter())
        {
            *target = sample * multiplier;
        }
        self.plan
            .process_with_scratch(&mut self.input, &mut self.spectrum, &mut self.scratch)
            .expect("real FFT buffers are allocated by their plan");
    }

    fn spectrum(&self) -> &[Complex32] {
        &self.spectrum
    }
}
