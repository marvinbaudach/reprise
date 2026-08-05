//! Shared frequency-band measurement primitives.
//!
//! The live CAVA renderer and the stored spectrogram deliberately meet here:
//! they use one logarithmic band mapper and one calibrated dBFS conversion.
//! Temporal smoothing remains a renderer concern and does not live in this
//! module.

use std::ops::RangeInclusive;
use std::sync::Arc;

use realfft::{num_complex::Complex32, RealFftPlanner, RealToComplex};

pub(crate) const ABSOLUTE_DB_FLOOR: f32 = -70.0;
pub(crate) const ABSOLUTE_DB_CEILING: f32 = -6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BandPlanError {
    InvalidConfiguration,
    TooManyBands,
}

pub(crate) struct BandPlan {
    cutoff_frequencies_hz: Vec<f32>,
    bands: Vec<Band>,
}

pub(crate) struct Band {
    pub(crate) lower_bin: usize,
    pub(crate) upper_bin: usize,
    pub(crate) use_bass_fft: bool,
}

impl Band {
    pub(crate) fn bins(&self) -> RangeInclusive<usize> {
        self.lower_bin..=self.upper_bin
    }
}

impl BandPlan {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        sample_rate_hz: u32,
        band_count: usize,
        low_cutoff_hz: u32,
        high_cutoff_hz: u32,
        fft_size: usize,
        bass_fft_size: usize,
        bass_split_hz: f32,
    ) -> Result<Self, BandPlanError> {
        if sample_rate_hz == 0
            || band_count == 0
            || low_cutoff_hz == 0
            || low_cutoff_hz >= high_cutoff_hz
            || high_cutoff_hz > sample_rate_hz / 2
            || fft_size < 2
            || bass_fft_size < fft_size
        {
            return Err(BandPlanError::InvalidConfiguration);
        }
        if band_count > fft_size / 2 + 1 {
            return Err(BandPlanError::TooManyBands);
        }

        let boundary_count = band_count + 1;
        let mut cutoffs = vec![0.0_f32; boundary_count];
        let mut relative_cutoffs = vec![0.0_f32; boundary_count];
        let mut lower_bins = vec![0usize; boundary_count];
        let mut upper_bins = vec![0usize; boundary_count];
        let frequency_constant = frequency_constant(band_count, low_cutoff_hz, high_cutoff_hz);
        let min_bandwidth_hz = sample_rate_hz as f32 / bass_fft_size as f32;
        let relative_nyquist_hz = (sample_rate_hz / 2) as f32;
        let exact_nyquist_hz = sample_rate_hz as f32 / 2.0;
        let mut bass_boundary_count = 0usize;
        let mut first_band = true;

        for boundary in 0..boundary_count {
            let distribution = -frequency_constant
                + f64::from((boundary as f32 + 1.0) / boundary_count as f32) * frequency_constant;
            cutoffs[boundary] = (f64::from(high_cutoff_hz) * 10.0_f64.powf(distribution)) as f32;
            if boundary > 0 && cutoffs[boundary - 1] >= cutoffs[boundary] {
                cutoffs[boundary] = cutoffs[boundary - 1] + min_bandwidth_hz;
            }

            relative_cutoffs[boundary] = cutoffs[boundary] / relative_nyquist_hz;
            if cutoffs[boundary] < bass_split_hz {
                lower_bins[boundary] =
                    (relative_cutoffs[boundary] * (bass_fft_size / 2) as f32) as usize;
                bass_boundary_count += 1;
                if bass_boundary_count > 1 {
                    first_band = false;
                }
                lower_bins[boundary] = lower_bins[boundary].min(bass_fft_size / 2);
            } else {
                lower_bins[boundary] =
                    (relative_cutoffs[boundary] * (fft_size / 2) as f32).ceil() as usize;
                if boundary == bass_boundary_count {
                    first_band = true;
                    if boundary > 0 {
                        upper_bins[boundary - 1] = (relative_cutoffs[boundary]
                            * (bass_fft_size / 2) as f32
                            - 1.0) as usize;
                    }
                } else {
                    first_band = false;
                }
                lower_bins[boundary] = lower_bins[boundary].min(fft_size / 2);
            }

            if boundary > 0 {
                if first_band {
                    if upper_bins[boundary - 1] < lower_bins[boundary - 1] {
                        upper_bins[boundary - 1] = lower_bins[boundary - 1] + 1;
                    }
                } else {
                    upper_bins[boundary - 1] = lower_bins[boundary].saturating_sub(1);
                    if lower_bins[boundary] <= lower_bins[boundary - 1] {
                        let half_size = if boundary < bass_boundary_count {
                            bass_fft_size / 2
                        } else {
                            fft_size / 2
                        };
                        if lower_bins[boundary - 1] + 1 < half_size + 1 {
                            lower_bins[boundary] = lower_bins[boundary - 1] + 1;
                            upper_bins[boundary - 1] = lower_bins[boundary] - 1;
                        }
                    }
                }
            }

            let source_fft_size = if boundary < bass_boundary_count {
                bass_fft_size
            } else {
                fft_size
            };
            relative_cutoffs[boundary] = lower_bins[boundary] as f32 / (source_fft_size / 2) as f32;
            cutoffs[boundary] = relative_cutoffs[boundary] * exact_nyquist_hz;
        }

        let bands = (0..band_count)
            .map(|index| {
                let use_bass_fft = index < bass_boundary_count;
                Band {
                    lower_bin: lower_bins[index],
                    upper_bin: upper_bins[index],
                    use_bass_fft,
                }
            })
            .collect();

        Ok(Self {
            cutoff_frequencies_hz: cutoffs,
            bands,
        })
    }

    pub(crate) fn cutoff_frequencies_hz(&self) -> &[f32] {
        &self.cutoff_frequencies_hz
    }

    pub(crate) fn bands(&self) -> &[Band] {
        &self.bands
    }
}

fn frequency_constant(band_count: usize, low_cutoff_hz: u32, high_cutoff_hz: u32) -> f64 {
    let cutoff_ratio = low_cutoff_hz as f32 / high_cutoff_hz as f32;
    let denominator = 1.0_f32 / (band_count as f32 + 1.0) - 1.0;
    f64::from(cutoff_ratio).log10() / f64::from(denominator)
}

pub(crate) struct FftWorkspace {
    plan: Arc<dyn RealToComplex<f32>>,
    input: Vec<f32>,
    spectrum: Vec<Complex32>,
    scratch: Vec<Complex32>,
    hann: Vec<f32>,
    hann_square_sum: f32,
}

impl FftWorkspace {
    pub(crate) fn new(len: usize) -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let plan = planner.plan_fft_forward(len);
        let input = plan.make_input_vec();
        let spectrum = plan.make_output_vec();
        let scratch = plan.make_scratch_vec();
        let hann = (0..len)
            .map(|index| {
                0.5 * (1.0 - (std::f32::consts::TAU * index as f32 / (len - 1) as f32).cos())
            })
            .collect::<Vec<_>>();
        let hann_square_sum = hann.iter().map(|value| value * value).sum();
        Self {
            plan,
            input,
            spectrum,
            scratch,
            hann,
            hann_square_sum,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.input.len()
    }

    pub(crate) fn process(&mut self, samples: &[f32]) {
        debug_assert_eq!(samples.len(), self.input.len());
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

    pub(crate) fn band_rms_dbfs(&self, bins: RangeInclusive<usize>) -> f32 {
        let nyquist = self.spectrum.len().saturating_sub(1);
        let power = bins
            .map(|bin| {
                let one_sided_weight = if bin == 0 || bin == nyquist { 1.0 } else { 2.0 };
                self.spectrum[bin].norm_sqr() * one_sided_weight
            })
            .sum::<f32>();
        let mean_square = power / (self.len() as f32 * self.hann_square_sum);
        if mean_square > 0.0 {
            10.0 * mean_square.log10()
        } else {
            f32::NEG_INFINITY
        }
    }
}

pub(crate) fn absolute_dbfs_to_byte(level_dbfs: f32) -> u8 {
    (absolute_dbfs_to_unit(level_dbfs) * 255.0).round() as u8
}

pub(crate) fn absolute_dbfs_to_unit(level_dbfs: f32) -> f32 {
    ((level_dbfs - ABSOLUTE_DB_FLOOR) / (ABSOLUTE_DB_CEILING - ABSOLUTE_DB_FLOOR)).clamp(0.0, 1.0)
}
