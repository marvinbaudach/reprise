//! Streaming resampler for the analysis session: linear interpolation behind a
//! cheap anti-alias low-pass.
//!
//! Decision 3 of `docs/plans/the-phone-analyses-its-own-music.md` judged an
//! anti-alias filter not worth it. That held for the 16-22 kHz remnants of
//! 44.1/48 kHz sources, but not for high-rate sources (88.2/96/192 kHz), whose
//! ultrasonic content folds straight into the audible bands of the spectrogram.
//! When the source rate is above the target, the input therefore runs through an
//! eighth-order Butterworth low-pass first: four biquads, a constant 20 or so
//! multiply-adds per input sample, no allocation beyond the chunk copy.

use std::f64::consts::{PI, TAU};

/// Cut-off as a fraction of the *target* rate. Just under its Nyquist (0.5), so
/// the 16 kHz top of the spectrogram loses a few dB while the first aliasing
/// frequencies are already well down.
const CUTOFF_OF_TARGET_RATE: f64 = 0.47;
/// Butterworth order 8 as four second-order sections.
const SECTION_COUNT: usize = 4;

/// One direct-form-II-transposed biquad low-pass section.
struct Biquad {
    b0: f64,
    b1: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    /// RBJ low-pass; `b2 == b0` for a low-pass, so it is not stored.
    fn low_pass(cutoff_hz: f64, sample_rate_hz: f64, q: f64) -> Self {
        let w0 = TAU * cutoff_hz / sample_rate_hz;
        let alpha = w0.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        let b0 = (1.0 - w0.cos()) / 2.0;
        Self {
            b0: b0 / a0,
            b1: (1.0 - w0.cos()) / a0,
            a1: -2.0 * w0.cos() / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn process(&mut self, input: f64) -> f64 {
        let output = self.b0 * input + self.z1;
        self.z1 = self.b1 * input - self.a1 * output + self.z2;
        self.z2 = self.b0 * input - self.a2 * output;
        output
    }
}

/// Butterworth low-pass whose state survives chunk boundaries.
struct AntiAliasFilter {
    sections: [Biquad; SECTION_COUNT],
}

impl AntiAliasFilter {
    fn new(from_hz: u32, to_hz: u32) -> Self {
        let cutoff_hz = f64::from(to_hz) * CUTOFF_OF_TARGET_RATE;
        let order = (2 * SECTION_COUNT) as f64;
        let sections = std::array::from_fn(|index| {
            // Butterworth pole pairs: Q = 1 / (2 sin((2k - 1) pi / 2N)).
            let q = 1.0 / (2.0 * ((2 * index + 1) as f64 * PI / (2.0 * order)).sin());
            Biquad::low_pass(cutoff_hz, f64::from(from_hz), q)
        });
        Self { sections }
    }

    fn filter(&mut self, chunk: &[f32]) -> Vec<f32> {
        chunk
            .iter()
            .map(|sample| {
                let filtered = self
                    .sections
                    .iter_mut()
                    .fold(f64::from(*sample), |value, section| section.process(value));
                filtered as f32
            })
            .collect()
    }
}

/// Streaming resampler that keeps its fractional output phase across chunks.
///
/// Every chunk after the first needs the previous chunk's last sample to
/// interpolate its own first output; without it, every chunk boundary would
/// restart the phase and produce an audible discontinuity in the resampled
/// stream.
pub struct LinearResampler {
    from_hz: u32,
    to_hz: u32,
    /// Fractional read position into the *next* pushed chunk, in input-sample
    /// units. Zero at the very first sample of the stream; may be slightly
    /// negative once phase has to reach back into `last_sample`.
    position: f64,
    /// The previous chunk's final input sample, needed to interpolate any
    /// output position that falls before this chunk's first sample.
    last_sample: Option<f32>,
    /// Present only when decimating: the source rate is above the target.
    anti_alias: Option<AntiAliasFilter>,
}

impl LinearResampler {
    #[must_use]
    pub fn new(from_hz: u32, to_hz: u32) -> Self {
        Self {
            from_hz,
            to_hz,
            position: 0.0,
            last_sample: None,
            anti_alias: (from_hz > to_hz && to_hz > 0)
                .then(|| AntiAliasFilter::new(from_hz, to_hz)),
        }
    }

    /// Appends this chunk's resampled output to `out`. `mono` is one
    /// contiguous chunk of the input stream at `from_hz`; chunk boundaries
    /// may fall anywhere, including mid-cycle of the source waveform.
    pub fn push(&mut self, mono: &[f32], out: &mut Vec<f32>) {
        if mono.is_empty() {
            return;
        }
        if self.from_hz == self.to_hz {
            out.extend_from_slice(mono);
            self.last_sample = mono.last().copied();
            return;
        }
        let filtered;
        let mono = match self.anti_alias.as_mut() {
            Some(filter) => {
                filtered = filter.filter(mono);
                filtered.as_slice()
            }
            None => mono,
        };
        let ratio = f64::from(self.from_hz) / f64::from(self.to_hz);
        let last_index = (mono.len() - 1) as f64;
        while self.position <= last_index {
            out.push(self.sample_at(self.position, mono));
            self.position += ratio;
        }
        self.position -= mono.len() as f64;
        self.last_sample = mono.last().copied();
    }

    fn sample_at(&self, index: f64, mono: &[f32]) -> f32 {
        if index < 0.0 {
            let previous = self.last_sample.unwrap_or(mono[0]);
            let fraction = (index + 1.0) as f32;
            return previous + (mono[0] - previous) * fraction;
        }
        let base = index.floor();
        let fraction = (index - base) as f32;
        let base = base as usize;
        let start = mono[base];
        let next = mono.get(base + 1).copied().unwrap_or(start);
        start + (next - start) * fraction
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(sample_rate_hz: u32, frequency_hz: f64, sample_count: usize) -> Vec<f32> {
        (0..sample_count)
            .map(|index| {
                let phase =
                    std::f64::consts::TAU * frequency_hz * index as f64 / f64::from(sample_rate_hz);
                phase.sin() as f32
            })
            .collect()
    }

    #[test]
    fn resampler_is_identity_at_equal_rates() {
        let input = sine(32_000, 440.0, 500);
        let mut resampler = LinearResampler::new(32_000, 32_000);
        let mut out = Vec::new();

        resampler.push(&input[..200], &mut out);
        resampler.push(&input[200..], &mut out);

        assert_eq!(out, input);
    }

    #[test]
    fn resampler_output_length_matches_the_ratio() {
        let input = sine(48_000, 440.0, 48_000);
        let mut resampler = LinearResampler::new(48_000, 32_000);
        let mut out = Vec::new();

        resampler.push(&input, &mut out);

        assert!(
            (out.len() as i64 - 32_000).abs() <= 1,
            "expected ~32000 output samples, got {}",
            out.len()
        );
    }

    #[test]
    fn resampler_keeps_phase_across_chunk_boundaries() {
        let input = sine(48_000, 1_000.0, 4_800);

        let mut whole = LinearResampler::new(48_000, 32_000);
        let mut whole_out = Vec::new();
        whole.push(&input, &mut whole_out);

        let mut ragged = LinearResampler::new(48_000, 32_000);
        let mut ragged_out = Vec::new();
        for chunk in [&input[0..17], &input[17..1_003], &input[1_003..4_800]] {
            ragged.push(chunk, &mut ragged_out);
        }

        assert_eq!(
            whole_out.len(),
            ragged_out.len(),
            "chunking must not change the total output length"
        );
        for (index, (whole_sample, ragged_sample)) in
            whole_out.iter().zip(ragged_out.iter()).enumerate()
        {
            assert!(
                (whole_sample - ragged_sample).abs() < 1.0e-5,
                "sample {index} diverged: whole {whole_sample}, ragged {ragged_sample}"
            );
        }
    }

    fn rms(samples: &[f32]) -> f64 {
        let energy: f64 = samples.iter().map(|s| f64::from(*s).powi(2)).sum();
        (energy / samples.len() as f64).sqrt()
    }

    /// Output RMS of a sine, ignoring the filter's start-up transient.
    fn resampled_rms(from_hz: u32, frequency_hz: f64) -> f64 {
        let input = sine(from_hz, frequency_hz, from_hz as usize);
        let mut resampler = LinearResampler::new(from_hz, 32_000);
        let mut out = Vec::new();
        resampler.push(&input, &mut out);
        rms(&out[out.len() / 4..])
    }

    const SINE_RMS: f64 = std::f64::consts::FRAC_1_SQRT_2;

    #[test]
    fn content_above_the_target_nyquist_does_not_alias_into_the_band() {
        // 30 kHz at 96 kHz would fold to 2 kHz; 19 kHz at 44.1 kHz to 13 kHz.
        assert!(
            resampled_rms(96_000, 30_000.0) < SINE_RMS * 0.01,
            "30 kHz leaked: {}",
            resampled_rms(96_000, 30_000.0)
        );
        assert!(
            resampled_rms(44_100, 19_000.0) < SINE_RMS * 0.25,
            "19 kHz leaked: {}",
            resampled_rms(44_100, 19_000.0)
        );
    }

    #[test]
    fn music_band_content_passes_the_anti_alias_filter() {
        for (from_hz, frequency_hz) in [(48_000, 1_000.0), (44_100, 5_000.0), (96_000, 10_000.0)] {
            let level = resampled_rms(from_hz, frequency_hz);
            assert!(
                level > SINE_RMS * 0.94,
                "{frequency_hz} Hz at {from_hz} Hz lost too much: {level}"
            );
        }
    }
}
