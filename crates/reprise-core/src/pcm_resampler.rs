//! Streaming conversion of decoded interleaved PCM into the spectrogram format's mono rate.

use std::f64::consts::PI;

use crate::spectrogram::SPECTROGRAM_SAMPLE_RATE_HZ;

const HALF_KERNEL_WIDTH: i64 = 24;
const KERNEL_PHASE_COUNT: usize = 2_048;
const INGEST_BLOCK_FRAMES: usize = 1_024;

/// Invalid decoded PCM passed to [`PcmResampler`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PcmResamplerError {
    #[error("input sample rate must be greater than zero")]
    ZeroSampleRate,
    #[error("input channel count must be greater than zero")]
    ZeroChannels,
    #[error("PCM chunk has {sample_count} samples, not a whole number of {channel_count}-channel frames")]
    IncompleteFrame {
        sample_count: usize,
        channel_count: usize,
    },
    #[error("PCM chunk has {frame_count} frames, exceeding the one-second limit of {limit}")]
    ChunkTooLarge { frame_count: usize, limit: usize },
    #[error("PCM chunk contains a non-finite sample")]
    NonFiniteSample,
}

/// Stateful interleaved-PCM fold-down and band-limited conversion to 32 kHz mono.
///
/// A call accepts at most one second of source frames. Internally it ingests in
/// smaller blocks and retains only the short sinc tail needed by the next call;
/// it never accumulates a track-sized PCM buffer. [`finish`](Self::finish)
/// flushes the final filter tail and returns every output sample whose source
/// position falls inside the supplied stream.
pub struct PcmResampler {
    input_sample_rate_hz: u32,
    channel_count: usize,
    kernel: Vec<Vec<f64>>,
    input: Vec<f32>,
    input_start_frame: u64,
    received_frames: u64,
    next_source_numerator: u64,
}

impl PcmResampler {
    pub fn new(input_sample_rate_hz: u32, channel_count: usize) -> Result<Self, PcmResamplerError> {
        if input_sample_rate_hz == 0 {
            return Err(PcmResamplerError::ZeroSampleRate);
        }
        if channel_count == 0 {
            return Err(PcmResamplerError::ZeroChannels);
        }
        let cutoff =
            (f64::from(SPECTROGRAM_SAMPLE_RATE_HZ) / f64::from(input_sample_rate_hz)).min(1.0);
        Ok(Self {
            input_sample_rate_hz,
            channel_count,
            kernel: build_kernel(cutoff),
            input: Vec::new(),
            input_start_frame: 0,
            received_frames: 0,
            next_source_numerator: 0,
        })
    }

    /// Folds one bounded interleaved chunk to mono and returns every 32 kHz
    /// sample for which enough future source support is now available.
    pub fn push(&mut self, interleaved: &[f32]) -> Result<Vec<f32>, PcmResamplerError> {
        if !interleaved.len().is_multiple_of(self.channel_count) {
            return Err(PcmResamplerError::IncompleteFrame {
                sample_count: interleaved.len(),
                channel_count: self.channel_count,
            });
        }
        let frame_count = interleaved.len() / self.channel_count;
        let limit = self.input_sample_rate_hz as usize;
        if frame_count > limit {
            return Err(PcmResamplerError::ChunkTooLarge { frame_count, limit });
        }
        if interleaved.iter().any(|sample| !sample.is_finite()) {
            return Err(PcmResamplerError::NonFiniteSample);
        }

        if self.input_sample_rate_hz == SPECTROGRAM_SAMPLE_RATE_HZ {
            return Ok(fold_to_mono(interleaved, self.channel_count));
        }

        let mut output = Vec::with_capacity(
            frame_count.saturating_mul(SPECTROGRAM_SAMPLE_RATE_HZ as usize)
                / self.input_sample_rate_hz as usize
                + 1,
        );
        for block in interleaved.chunks(self.channel_count * INGEST_BLOCK_FRAMES) {
            let mono = fold_to_mono(block, self.channel_count);
            self.received_frames += mono.len() as u64;
            self.input.extend(mono);
            self.produce(false, &mut output);
        }
        Ok(output)
    }

    /// Flushes the short final filter tail. No PCM is retained afterwards.
    pub fn finish(mut self) -> Vec<f32> {
        if self.input_sample_rate_hz == SPECTROGRAM_SAMPLE_RATE_HZ {
            return Vec::new();
        }
        let remaining = self
            .output_sample_count()
            .saturating_sub(self.next_source_numerator / u64::from(self.input_sample_rate_hz));
        let mut output = Vec::with_capacity(remaining as usize);
        self.produce(true, &mut output);
        output
    }

    fn output_sample_count(&self) -> u64 {
        self.received_frames
            .saturating_mul(u64::from(SPECTROGRAM_SAMPLE_RATE_HZ))
            / u64::from(self.input_sample_rate_hz)
    }

    fn produce(&mut self, flush: bool, output: &mut Vec<f32>) {
        let output_rate = u64::from(SPECTROGRAM_SAMPLE_RATE_HZ);
        loop {
            let source_frame = self.next_source_numerator / output_rate;
            if flush {
                if self.next_source_numerator >= self.received_frames.saturating_mul(output_rate) {
                    break;
                }
            } else if source_frame.saturating_add(HALF_KERNEL_WIDTH as u64) >= self.received_frames
            {
                break;
            }

            let remainder = self.next_source_numerator % output_rate;
            output.push(self.interpolate(source_frame, remainder, output_rate));
            self.next_source_numerator = self
                .next_source_numerator
                .saturating_add(u64::from(self.input_sample_rate_hz));
        }
        self.discard_consumed_input();
    }

    fn interpolate(&self, source_frame: u64, remainder: u64, denominator: u64) -> f32 {
        let phase_position = remainder as f64 * KERNEL_PHASE_COUNT as f64 / denominator as f64;
        let lower_phase = phase_position.floor() as usize;
        let upper_phase = (lower_phase + 1).min(KERNEL_PHASE_COUNT);
        let phase_mix = phase_position - lower_phase as f64;
        let mut value = 0.0;
        let mut available_weight = 0.0;
        for (tap, offset) in (-HALF_KERNEL_WIDTH + 1..=HALF_KERNEL_WIDTH).enumerate() {
            let weight = self.kernel[lower_phase][tap]
                + (self.kernel[upper_phase][tap] - self.kernel[lower_phase][tap]) * phase_mix;
            let Some(index) = source_frame.checked_add_signed(offset) else {
                continue;
            };
            let Some(sample) = self.sample(index) else {
                continue;
            };
            value += f64::from(sample) * weight;
            available_weight += weight;
        }
        if available_weight.abs() <= f64::EPSILON {
            0.0
        } else {
            (value / available_weight).clamp(-1.0, 1.0) as f32
        }
    }

    fn sample(&self, absolute_frame: u64) -> Option<f32> {
        let index = absolute_frame.checked_sub(self.input_start_frame)? as usize;
        self.input.get(index).copied()
    }

    fn discard_consumed_input(&mut self) {
        let output_rate = u64::from(SPECTROGRAM_SAMPLE_RATE_HZ);
        let next_frame = self.next_source_numerator / output_rate;
        let keep_from = next_frame.saturating_sub((HALF_KERNEL_WIDTH - 1) as u64);
        let discard = keep_from
            .saturating_sub(self.input_start_frame)
            .min(self.input.len() as u64) as usize;
        self.input.drain(..discard);
        self.input_start_frame += discard as u64;
    }
}

fn fold_to_mono(interleaved: &[f32], channel_count: usize) -> Vec<f32> {
    interleaved
        .chunks_exact(channel_count)
        .map(|frame| {
            let sum = frame.iter().map(|sample| f64::from(*sample)).sum::<f64>();
            (sum / channel_count as f64).clamp(-1.0, 1.0) as f32
        })
        .collect()
}

fn build_kernel(cutoff: f64) -> Vec<Vec<f64>> {
    (0..=KERNEL_PHASE_COUNT)
        .map(|phase| {
            let fraction = phase as f64 / KERNEL_PHASE_COUNT as f64;
            let mut weights = (-HALF_KERNEL_WIDTH + 1..=HALF_KERNEL_WIDTH)
                .map(|offset| {
                    let distance = offset as f64 - fraction;
                    let window = 0.42
                        + 0.5 * (PI * distance / HALF_KERNEL_WIDTH as f64).cos()
                        + 0.08 * (2.0 * PI * distance / HALF_KERNEL_WIDTH as f64).cos();
                    cutoff * sinc(cutoff * distance) * window
                })
                .collect::<Vec<_>>();
            let sum = weights.iter().sum::<f64>();
            for weight in &mut weights {
                *weight /= sum;
            }
            weights
        })
        .collect()
}

fn sinc(value: f64) -> f64 {
    if value.abs() <= f64::EPSILON {
        1.0
    } else {
        (PI * value).sin() / (PI * value)
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::TAU;

    use crate::spectrogram::{SpectrogramAccumulator, SPECTROGRAM_SAMPLE_RATE_HZ};

    use super::{PcmResampler, PcmResamplerError, HALF_KERNEL_WIDTH, INGEST_BLOCK_FRAMES};

    fn signal(sample_rate_hz: u32, seconds: u32) -> Vec<f32> {
        let sample_count = sample_rate_hz as usize * seconds as usize;
        (0..sample_count)
            .map(|index| {
                let time = index as f64 / f64::from(sample_rate_hz);
                let sweep_phase = TAU
                    * (180.0 * time
                        + (12_000.0 - 180.0) * time * time / (2.0 * f64::from(seconds)));
                let tones = [55.0, 310.0, 1_200.0, 4_800.0, 9_500.0]
                    .into_iter()
                    .enumerate()
                    .map(|(tone, frequency)| {
                        let envelope =
                            0.55 + 0.45 * (TAU * (0.17 + tone as f64 * 0.031) * time).sin().abs();
                        envelope * (TAU * frequency * time).sin()
                    })
                    .sum::<f64>();
                ((0.18 * tones + 0.22 * sweep_phase.sin()).clamp(-1.0, 1.0)) as f32
            })
            .collect()
    }

    fn resample(input: &[f32], chunks: &[usize]) -> Vec<f32> {
        let mut resampler = PcmResampler::new(44_100, 1).unwrap();
        let mut output = Vec::new();
        let mut start = 0;
        for &chunk in chunks.iter().cycle() {
            if start == input.len() {
                break;
            }
            let end = (start + chunk).min(input.len());
            output.extend(resampler.push(&input[start..end]).unwrap());
            start = end;
        }
        output.extend(resampler.finish());
        output
    }

    #[test]
    fn one_chunk_and_arbitrary_chunks_produce_identical_samples() {
        let input = signal(44_100, 1);
        let one_chunk = resample(&input, &[input.len()]);
        let many_chunks = resample(&input, &[1, 17, 503, 2, 8_191, 97, 20_003]);

        assert_eq!(many_chunks, one_chunk);
        assert_eq!(one_chunk.len(), SPECTROGRAM_SAMPLE_RATE_HZ as usize);
    }

    #[test]
    fn stereo_frames_fold_to_the_arithmetic_mono_mean() {
        let mut resampler = PcmResampler::new(SPECTROGRAM_SAMPLE_RATE_HZ, 2).unwrap();
        let output = resampler
            .push(&[0.75, 0.25, 1.0, -1.0, -0.25, -0.75])
            .unwrap();

        assert_eq!(output, vec![0.5, 0.0, -0.5]);
        assert!(resampler.finish().is_empty());
    }

    #[test]
    fn one_second_is_the_largest_accepted_pcm_chunk() {
        let mut resampler = PcmResampler::new(48_000, 2).unwrap();
        assert!(resampler.push(&vec![0.0; 48_000 * 2]).is_ok());
        assert_eq!(
            resampler.push(&vec![0.0; 48_001 * 2]),
            Err(PcmResamplerError::ChunkTooLarge {
                frame_count: 48_001,
                limit: 48_000,
            })
        );
    }

    #[test]
    fn streaming_retains_only_the_filter_tail_between_chunks() {
        let mut resampler = PcmResampler::new(44_100, 1).unwrap();
        for _ in 0..3 {
            let _ = resampler.push(&vec![0.0; 44_100]).unwrap();
            assert!(
                resampler.input.len() <= INGEST_BLOCK_FRAMES + (HALF_KERNEL_WIDTH as usize * 2),
                "retained {} source frames",
                resampler.input.len()
            );
        }
    }

    #[test]
    fn resampled_spectral_cells_match_native_32_khz_cells_without_bias() {
        let seconds = 12;
        let native = signal(SPECTROGRAM_SAMPLE_RATE_HZ, seconds);
        let source = signal(44_100, seconds);
        let resampled = resample(&source, &[997, 5_003, 31, 44_100, 8_191]);

        let mut native_accumulator = SpectrogramAccumulator::new();
        native_accumulator.push(&native);
        let native_cells = native_accumulator.finish();
        let mut resampled_accumulator = SpectrogramAccumulator::new();
        resampled_accumulator.push(&resampled);
        let resampled_cells = resampled_accumulator.finish();

        assert_eq!(resampled_cells.cells().len(), native_cells.cells().len());
        let differences = resampled_cells
            .cells()
            .iter()
            .zip(native_cells.cells())
            .map(|(actual, expected)| i16::from(*actual) - i16::from(*expected))
            .collect::<Vec<_>>();
        let within_two = differences
            .iter()
            .filter(|difference| difference.abs() <= 2)
            .count() as f64
            / differences.len() as f64;
        let mean_absolute = differences
            .iter()
            .map(|difference| f64::from(difference.abs()))
            .sum::<f64>()
            / differences.len() as f64;
        let mean_signed = differences
            .iter()
            .map(|difference| f64::from(*difference))
            .sum::<f64>()
            / differences.len() as f64;

        assert!(
            within_two >= 0.99,
            "{within_two:.5} of cells were within two byte steps"
        );
        assert!(
            mean_absolute < 1.0,
            "mean absolute difference was {mean_absolute:.5} byte steps"
        );
        assert!(
            mean_signed.abs() < 0.1,
            "mean signed difference was {mean_signed:.5} byte steps"
        );
    }
}
