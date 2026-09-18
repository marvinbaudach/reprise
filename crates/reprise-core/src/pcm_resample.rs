//! Streaming linear-interpolation resampler for the mobile analysis session.
//!
//! Decision 3 of `docs/plans/the-phone-analyses-its-own-music.md`: no
//! anti-alias filter, linear interpolation only. The consumer is a 24-band
//! spectrogram ending at `SPECTROGRAM_HIGH_HZ` (16 kHz); the only aliasing
//! candidates are the 16-22 kHz remnants of 44.1/48 kHz sources, judged not
//! worth a filter.

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
}

impl LinearResampler {
    #[must_use]
    pub fn new(from_hz: u32, to_hz: u32) -> Self {
        Self {
            from_hz,
            to_hz,
            position: 0.0,
            last_sample: None,
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
}
