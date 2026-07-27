use super::{CavaConfig, CavaError};

const BASE_FFT_SIZE: usize = 512;
const BASS_SPLIT_HZ: f32 = 100.0;

pub(super) struct BandPlan {
    cutoff_frequencies_hz: Vec<f32>,
    bands: Vec<Band>,
}

pub(super) struct Band {
    pub(super) lower_bin: usize,
    pub(super) upper_bin: usize,
    pub(super) use_bass_fft: bool,
    pub(super) equalizer: f32,
}

impl BandPlan {
    pub(super) fn new(config: CavaConfig) -> Result<Self, CavaError> {
        let fft_size = fft_size_for_rate(config.sample_rate_hz);
        if config.bar_count > fft_size / 2 + 1 {
            return Err(CavaError::InvalidBarCount);
        }
        let bass_fft_size = fft_size * 2;
        let boundary_count = config.bar_count + 1;
        let mut cutoffs = vec![0.0_f32; boundary_count];
        let mut relative_cutoffs = vec![0.0_f32; boundary_count];
        let mut lower_bins = vec![0usize; boundary_count];
        let mut upper_bins = vec![0usize; boundary_count];
        let frequency_constant = frequency_constant(config);
        let min_bandwidth_hz = (config.sample_rate_hz / bass_fft_size as u32) as f32;
        let relative_nyquist_hz = (config.sample_rate_hz / 2) as f32;
        let exact_nyquist_hz = config.sample_rate_hz as f32 / 2.0;
        let mut bass_boundary_count = 0usize;
        let mut first_bar = true;

        for boundary in 0..boundary_count {
            let distribution = -frequency_constant
                + f64::from((boundary as f32 + 1.0) / (boundary_count as f32)) * frequency_constant;
            cutoffs[boundary] =
                (f64::from(config.high_cutoff_hz) * 10.0_f64.powf(distribution)) as f32;
            if boundary > 0 && cutoffs[boundary - 1] >= cutoffs[boundary] {
                cutoffs[boundary] = cutoffs[boundary - 1] + min_bandwidth_hz;
            }

            relative_cutoffs[boundary] = cutoffs[boundary] / relative_nyquist_hz;
            if cutoffs[boundary] < BASS_SPLIT_HZ {
                lower_bins[boundary] =
                    (relative_cutoffs[boundary] * (bass_fft_size / 2) as f32) as usize;
                bass_boundary_count += 1;
                if bass_boundary_count > 1 {
                    first_bar = false;
                }
                lower_bins[boundary] = lower_bins[boundary].min(bass_fft_size / 2);
            } else {
                lower_bins[boundary] =
                    (relative_cutoffs[boundary] * (fft_size / 2) as f32).ceil() as usize;
                if boundary == bass_boundary_count {
                    first_bar = true;
                    if boundary > 0 {
                        upper_bins[boundary - 1] = (relative_cutoffs[boundary]
                            * (bass_fft_size / 2) as f32
                            - 1.0) as usize;
                    }
                } else {
                    first_bar = false;
                }
                lower_bins[boundary] = lower_bins[boundary].min(fft_size / 2);
            }

            if boundary > 0 {
                if first_bar {
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

        let bands = (0..config.bar_count)
            .map(|bar| {
                let use_bass_fft = bar < bass_boundary_count;
                let window_size = if use_bass_fft {
                    bass_fft_size
                } else {
                    fft_size
                };
                let bin_count = upper_bins[bar] - lower_bins[bar] + 1;
                let equalizer = 2.0_f32.powi(-28) * cutoffs[bar + 1].powf(0.85)
                    / (window_size as f32).log2()
                    / bin_count as f32;
                Band {
                    lower_bin: lower_bins[bar],
                    upper_bin: upper_bins[bar],
                    use_bass_fft,
                    equalizer,
                }
            })
            .collect();

        Ok(Self {
            cutoff_frequencies_hz: cutoffs,
            bands,
        })
    }

    pub(super) fn cutoff_frequencies_hz(&self) -> &[f32] {
        &self.cutoff_frequencies_hz
    }

    pub(super) fn bands(&self) -> &[Band] {
        &self.bands
    }
}

fn frequency_constant(config: CavaConfig) -> f64 {
    let cutoff_ratio = config.low_cutoff_hz as f32 / config.high_cutoff_hz as f32;
    let denominator = 1.0_f32 / (config.bar_count as f32 + 1.0) - 1.0;
    f64::from(cutoff_ratio).log10() / f64::from(denominator)
}

pub(super) fn fft_size_for_rate(sample_rate_hz: u32) -> usize {
    match sample_rate_hz {
        0..=8_125 => BASE_FFT_SIZE,
        8_126..=16_250 => BASE_FFT_SIZE * 2,
        16_251..=32_500 => BASE_FFT_SIZE * 4,
        32_501..=75_000 => BASE_FFT_SIZE * 8,
        75_001..=150_000 => BASE_FFT_SIZE * 16,
        150_001..=300_000 => BASE_FFT_SIZE * 32,
        _ => BASE_FFT_SIZE * 64,
    }
}
