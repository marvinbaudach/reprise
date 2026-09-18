//! Mobile-side streaming render-data producer: 16-bit PCM in, waveform peaks
//! and a spectrogram out. Decision 2 of
//! `docs/plans/the-phone-analyses-its-own-music.md`: all maths stays in
//! Rust; the platform decoder only pumps PCM. Decision 4: waveform peaks are
//! re-bucketed from per-frame RMS at the end rather than mapped through an
//! `expected_samples` upper bound, because that bound is not known until the
//! stream ends on this path.

use crate::pcm_resample::LinearResampler;
use crate::spectrogram::{
    SpectrogramAccumulator, SPECTROGRAM_FRAME_RATE_HZ, SPECTROGRAM_SAMPLE_RATE_HZ,
};
use crate::waveform::{TrackRenderData, STORED_PEAK_COUNT};

const SAMPLES_PER_FRAME: usize =
    SPECTROGRAM_SAMPLE_RATE_HZ as usize / SPECTROGRAM_FRAME_RATE_HZ as usize;

#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq)]
pub enum RenderDataSessionError {
    #[error("the decoder changed sample rate or channel count mid-stream")]
    RateOrChannelChanged,
    #[error("the audio stream had no samples")]
    EmptyStream,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct StreamConfig {
    sample_rate_hz: u32,
    channel_count: u32,
}

/// Streaming producer that turns interleaved 16-bit PCM into the same
/// [`TrackRenderData`] shape the desktop's GStreamer pipeline produces.
///
/// Downmixes to mono, resamples to [`SPECTROGRAM_SAMPLE_RATE_HZ`] with
/// [`LinearResampler`], and feeds [`SpectrogramAccumulator`] directly. The
/// per-frame RMS needed for the waveform peaks is kept as one running sum of
/// squares and one count per 1600-sample frame — never a sample buffer — and
/// re-bucketed into [`STORED_PEAK_COUNT`] buckets in [`finish`](Self::finish).
pub struct RenderDataSession {
    config: Option<StreamConfig>,
    resampler: Option<LinearResampler>,
    spectrogram: SpectrogramAccumulator,
    frame_sum_squares: f64,
    frame_samples_seen: u64,
    /// One `(sum_squares, count)` pair per completed 1600-sample frame, in
    /// stream order; the count is `SAMPLES_PER_FRAME` for every frame but the
    /// last, which may be shorter.
    frames: Vec<(f64, u64)>,
}

impl RenderDataSession {
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: None,
            resampler: None,
            spectrogram: SpectrogramAccumulator::new(),
            frame_sum_squares: 0.0,
            frame_samples_seen: 0,
            frames: Vec::new(),
        }
    }

    /// Downmixes `samples` (interleaved 16-bit PCM at `sample_rate_hz` with
    /// `channel_count` channels) to mono, resamples it to
    /// [`SPECTROGRAM_SAMPLE_RATE_HZ`], and folds it into the accumulators.
    ///
    /// The rate and channel count of the first call fix the stream's config;
    /// a later call with a different rate or channel count is rejected
    /// rather than silently mixing two configurations into one analysis.
    pub fn push_pcm_i16(
        &mut self,
        samples: &[i16],
        sample_rate_hz: u32,
        channel_count: u32,
    ) -> Result<(), RenderDataSessionError> {
        let config = StreamConfig {
            sample_rate_hz,
            channel_count,
        };
        match self.config {
            Some(existing) if existing != config => {
                return Err(RenderDataSessionError::RateOrChannelChanged);
            }
            Some(_) => {}
            None => {
                self.resampler = Some(LinearResampler::new(
                    sample_rate_hz,
                    SPECTROGRAM_SAMPLE_RATE_HZ,
                ));
                self.config = Some(config);
            }
        }

        let channels = channel_count.max(1) as usize;
        let mono: Vec<f32> = samples
            .chunks_exact(channels)
            .map(|frame| {
                let sum: f32 = frame.iter().map(|sample| f32::from(*sample)).sum();
                sum / channels as f32 / 32_768.0
            })
            .collect();

        let mut resampled = Vec::with_capacity(mono.len());
        self.resampler
            .as_mut()
            .expect("resampler is set together with config")
            .push(&mono, &mut resampled);

        self.spectrogram.push(&resampled);
        self.accumulate_waveform_frames(&resampled);
        Ok(())
    }

    fn accumulate_waveform_frames(&mut self, resampled: &[f32]) {
        for &sample in resampled {
            let sample = f64::from(sample.clamp(-1.0, 1.0));
            self.frame_sum_squares += sample * sample;
            self.frame_samples_seen += 1;
            if self.frame_samples_seen == SAMPLES_PER_FRAME as u64 {
                self.frames
                    .push((self.frame_sum_squares, self.frame_samples_seen));
                self.frame_sum_squares = 0.0;
                self.frame_samples_seen = 0;
            }
        }
    }

    /// Ends the stream: the spectrogram comes straight from the accumulator;
    /// the waveform peaks are the per-frame sums re-bucketed into
    /// [`STORED_PEAK_COUNT`] buckets and normalized exactly like
    /// `finish_waveform` in `waveform.rs`. An empty stream is an error: there
    /// is nothing to show and no track fingerprint to store it under.
    pub fn finish(mut self) -> Result<TrackRenderData, RenderDataSessionError> {
        if self.frame_samples_seen > 0 {
            self.frames
                .push((self.frame_sum_squares, self.frame_samples_seen));
        }
        if self.frames.is_empty() {
            return Err(RenderDataSessionError::EmptyStream);
        }
        let spectrogram = self.spectrogram.finish();
        let waveform_peaks = rebucket_peaks(&self.frames, STORED_PEAK_COUNT);
        Ok(TrackRenderData {
            waveform_peaks,
            spectrogram,
        })
    }
}

impl Default for RenderDataSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Distributes per-frame `(sum_squares, count)` pairs over `buckets` the same
/// way [`crate::waveform::WaveformAccumulator`] distributes samples over
/// buckets, then applies its exact sqrt-normalization so a computed analysis
/// and a desktop-decoded one read the same way.
fn rebucket_peaks(frames: &[(f64, u64)], buckets: usize) -> Vec<u8> {
    let frame_count = frames.len() as u64;
    let mut sum_squares = vec![0.0_f64; buckets];
    let mut counts = vec![0_u64; buckets];
    for (index, &(sum, count)) in frames.iter().enumerate() {
        let bucket = ((index as u64 * buckets as u64) / frame_count).min(buckets as u64 - 1);
        let bucket = bucket as usize;
        sum_squares[bucket] += sum;
        counts[bucket] += count;
    }
    finish_waveform_peaks(&sum_squares, &counts)
}

/// Mirrors `waveform.rs`'s private `finish_waveform`: per-bucket RMS,
/// max-normalized, sqrt-compressed into a `0..=255` byte. Duplicated rather
/// than exposed from `waveform.rs`, which this strand does not own.
fn finish_waveform_peaks(sum_squares: &[f64], counts: &[u64]) -> Vec<u8> {
    let rms: Vec<f64> = sum_squares
        .iter()
        .zip(counts)
        .map(|(sum, count)| {
            if *count == 0 {
                0.0
            } else {
                (sum / *count as f64).sqrt()
            }
        })
        .collect();
    let maximum = rms.iter().copied().fold(0.0_f64, f64::max);
    if maximum <= f64::EPSILON {
        return vec![0; rms.len()];
    }
    rms.into_iter()
        .map(|value| ((value / maximum).sqrt() * 255.0).round() as u8)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spectrogram::{SPECTROGRAM_BAND_COUNT, SPECTROGRAM_HIGH_HZ, SPECTROGRAM_LOW_HZ};
    use crate::waveform::WaveformAccumulator;

    /// One channel of 16-bit PCM for a sine at `frequency_hz`, `sample_count`
    /// samples long, at full scale times `amplitude`.
    fn sine_i16(
        sample_rate_hz: u32,
        frequency_hz: f64,
        sample_count: usize,
        amplitude: f64,
    ) -> Vec<i16> {
        (0..sample_count)
            .map(|index| {
                let phase =
                    std::f64::consts::TAU * frequency_hz * index as f64 / f64::from(sample_rate_hz);
                (phase.sin() * amplitude * f64::from(i16::MAX)) as i16
            })
            .collect()
    }

    fn interleave_stereo(mono: &[i16]) -> Vec<i16> {
        mono.iter().flat_map(|sample| [*sample, *sample]).collect()
    }

    /// The band index [`crate::spectrogram::TrackSpectrogram`] would put
    /// `frequency_hz` in, computed the same way the private
    /// `band_centre_octaves`/band-plan machinery does: log-spaced bands
    /// tiling `SPECTROGRAM_LOW_HZ..SPECTROGRAM_HIGH_HZ`.
    fn expected_band(frequency_hz: f64) -> usize {
        let low = f64::from(SPECTROGRAM_LOW_HZ).log2();
        let high = f64::from(SPECTROGRAM_HIGH_HZ).log2();
        let step = (high - low) / SPECTROGRAM_BAND_COUNT as f64;
        (((frequency_hz.log2() - low) / step).floor() as usize).min(SPECTROGRAM_BAND_COUNT - 1)
    }

    #[test]
    fn session_frame_count_follows_the_duration() {
        let sample_rate_hz = 44_100;
        let seconds = 10;
        let mono = sine_i16(
            sample_rate_hz,
            440.0,
            sample_rate_hz as usize * seconds,
            0.5,
        );
        let stereo = interleave_stereo(&mono);
        let mut session = RenderDataSession::new();

        session
            .push_pcm_i16(&stereo, sample_rate_hz, 2)
            .expect("push must succeed");
        let data = session.finish().expect("finish must succeed");

        assert_eq!(data.spectrogram.frame_count(), 200);
        assert_eq!(data.waveform_peaks.len(), STORED_PEAK_COUNT);
    }

    #[test]
    fn session_puts_a_sine_in_the_right_band() {
        let sample_rate_hz = SPECTROGRAM_SAMPLE_RATE_HZ;
        let mono = sine_i16(sample_rate_hz, 440.0, sample_rate_hz as usize * 2, 0.5);
        let mut session = RenderDataSession::new();

        session
            .push_pcm_i16(&mono, sample_rate_hz, 1)
            .expect("push must succeed");
        let data = session.finish().expect("finish must succeed");

        let target = expected_band(440.0);
        let frame = data
            .spectrogram
            .frame(data.spectrogram.frame_count() - 1)
            .expect("a finished stream has at least one frame");
        let peak_band = frame
            .iter()
            .enumerate()
            .max_by_key(|(_, level)| **level)
            .map(|(index, _)| index)
            .expect("a non-empty frame has a peak band");

        assert_eq!(peak_band, target, "frame was {frame:?}");
        assert!(
            frame[target] > frame[(target + 2).min(SPECTROGRAM_BAND_COUNT - 1)].saturating_add(20),
            "band {target} was not resolved from band {}: {frame:?}",
            target + 2
        );
    }

    #[test]
    fn session_peaks_match_the_desktop_accumulator() {
        // Exactly 1000 frames of 1600 samples each, matching STORED_PEAK_COUNT
        // one-to-one, so every bucket covers exactly one frame and the
        // comparison to `WaveformAccumulator` needs no tolerance for
        // frame/bucket boundary drift.
        let sample_rate_hz = SPECTROGRAM_SAMPLE_RATE_HZ;
        let total_samples = STORED_PEAK_COUNT * SAMPLES_PER_FRAME;
        let mono: Vec<i16> = (0..total_samples)
            .map(|index| {
                // A stepped envelope: ten equal segments at rising amplitude,
                // so no two segments are the same loudness.
                let segment = index / (total_samples / 10);
                let amplitude = 0.05 + 0.09 * segment as f64;
                let phase =
                    std::f64::consts::TAU * 440.0 * index as f64 / f64::from(sample_rate_hz);
                (phase.sin() * amplitude * f64::from(i16::MAX)) as i16
            })
            .collect();

        let mut session = RenderDataSession::new();
        session
            .push_pcm_i16(&mono, sample_rate_hz, 1)
            .expect("push must succeed");
        let session_peaks = session
            .finish()
            .expect("finish must succeed")
            .waveform_peaks;

        let float_samples: Vec<f32> = mono
            .iter()
            .map(|sample| f32::from(*sample) / 32_768.0)
            .collect();
        let mut accumulator =
            WaveformAccumulator::new(float_samples.len() as u64, STORED_PEAK_COUNT).unwrap();
        accumulator.push(&float_samples).unwrap();
        let desktop_peaks = accumulator.finish().unwrap();

        assert_eq!(session_peaks.len(), desktop_peaks.len());
        for (index, (session_peak, desktop_peak)) in
            session_peaks.iter().zip(desktop_peaks.iter()).enumerate()
        {
            assert!(
                (i32::from(*session_peak) - i32::from(*desktop_peak)).abs() <= 2,
                "bucket {index}: session {session_peak}, desktop {desktop_peak}"
            );
        }
    }

    #[test]
    fn session_rejects_a_rate_change_mid_stream() {
        let mut session = RenderDataSession::new();
        session.push_pcm_i16(&[0; 100], 44_100, 2).unwrap();

        let result = session.push_pcm_i16(&[0; 100], 48_000, 2);

        assert_eq!(result, Err(RenderDataSessionError::RateOrChannelChanged));
    }

    #[test]
    fn session_rejects_a_channel_change_mid_stream() {
        let mut session = RenderDataSession::new();
        session.push_pcm_i16(&[0; 100], 44_100, 2).unwrap();

        let result = session.push_pcm_i16(&[0; 100], 44_100, 1);

        assert_eq!(result, Err(RenderDataSessionError::RateOrChannelChanged));
    }

    #[test]
    fn session_rejects_an_empty_stream() {
        let session = RenderDataSession::new();

        let result = session.finish();

        assert_eq!(result, Err(RenderDataSessionError::EmptyStream));
    }
}
