use crate::sound_profile::{
    AudioEvidence, ProfileDimension, SoundProfile, SoundProfileError, TempoEstimate,
};

const WINDOW_SIZE: usize = 256;
const HOP_SIZE: usize = 128;
const RMS_HISTOGRAM_BINS: usize = 64;
const MIN_TEMPO_BPM: usize = 40;
const MAX_TEMPO_BPM: usize = 200;
const ONSET_RMS_FLOOR: f64 = 0.01;
const ONSET_RISE_FACTOR: f64 = 1.5;
const ROLLOFF_FRACTION: f64 = 0.85;
// The platform decoder feeds calibrated 8 kHz mono PCM, so 4 kHz is the
// stable projection ceiling across extractor and projection-only upgrades.
const PROFILE_NYQUIST_HZ: f64 = 4_000.0;
const INTENSITY_RMS_SCALE: f64 = 2.0;
const CENTROID_BRIGHTNESS_WEIGHT: f64 = 0.65;
const ROLLOFF_BRIGHTNESS_WEIGHT: f64 = 0.35;
const DYNAMIC_RANGE_SCALE: f64 = 2.0;
const ONSET_RATE_REFERENCE: f64 = 4.0;
const ONSET_RHYTHMICITY_WEIGHT: f64 = 0.7;
const TEMPO_RHYTHMICITY_WEIGHT: f64 = 0.3;

#[derive(Clone, Debug, PartialEq)]
pub struct AnalysisOutput {
    pub evidence: AudioEvidence,
    pub profile: SoundProfile,
    pub waveform_peaks: Vec<u8>,
}

pub struct AudioEvidenceAccumulator {
    sample_rate: u32,
    expected_samples: u64,
    samples_seen: u64,
    sum_squares: f64,
    frame_start: u64,
    frame_buffer: Vec<f64>,
    frame_rms_histogram: [u64; RMS_HISTOGRAM_BINS],
    previous_frame_rms: f64,
    previous_spectrum: Vec<f64>,
    spectral_centroid_sum: f64,
    spectral_rolloff_sum: f64,
    spectral_flux_sum: f64,
    spectral_frames: u64,
    onset_count: u64,
    last_onset_sample: Option<u64>,
    tempo_histogram: [u64; MAX_TEMPO_BPM + 1],
    tempo_intervals: u64,
    waveform_sum_squares: Vec<f64>,
    waveform_counts: Vec<u64>,
}

impl AudioEvidenceAccumulator {
    pub fn new(
        sample_rate: u32,
        expected_samples: u64,
        waveform_buckets: usize,
    ) -> Result<Self, AudioExtractionError> {
        if sample_rate == 0 || expected_samples == 0 || waveform_buckets == 0 {
            return Err(AudioExtractionError::InvalidConfiguration);
        }
        Ok(Self {
            sample_rate,
            expected_samples,
            samples_seen: 0,
            sum_squares: 0.0,
            frame_start: 0,
            frame_buffer: Vec::with_capacity(WINDOW_SIZE),
            frame_rms_histogram: [0; RMS_HISTOGRAM_BINS],
            previous_frame_rms: 0.0,
            previous_spectrum: vec![0.0; WINDOW_SIZE / 2 + 1],
            spectral_centroid_sum: 0.0,
            spectral_rolloff_sum: 0.0,
            spectral_flux_sum: 0.0,
            spectral_frames: 0,
            onset_count: 0,
            last_onset_sample: None,
            tempo_histogram: [0; MAX_TEMPO_BPM + 1],
            tempo_intervals: 0,
            waveform_sum_squares: vec![0.0; waveform_buckets],
            waveform_counts: vec![0; waveform_buckets],
        })
    }

    pub fn push(&mut self, samples: &[f32]) -> Result<(), AudioExtractionError> {
        let new_total = self
            .samples_seen
            .checked_add(samples.len() as u64)
            .ok_or(AudioExtractionError::TooManySamples)?;
        if self.expected_samples > 0 && new_total > self.expected_samples {
            return Err(AudioExtractionError::TooManySamples);
        }
        for &sample in samples {
            if !sample.is_finite() {
                return Err(AudioExtractionError::NonFiniteSample);
            }
            let sample = f64::from(sample.clamp(-1.0, 1.0));
            self.sum_squares += sample * sample;
            self.add_waveform_sample(sample);
            self.frame_buffer.push(sample);
            self.samples_seen += 1;
            if self.frame_buffer.len() == WINDOW_SIZE {
                self.analyze_frame();
                self.frame_buffer.drain(..HOP_SIZE);
                self.frame_start += HOP_SIZE as u64;
            }
        }
        Ok(())
    }

    pub fn buffered_sample_count(&self) -> usize {
        self.frame_buffer.len()
            + self.previous_spectrum.len()
            + self.waveform_sum_squares.len()
            + self.waveform_counts.len()
            + self.tempo_histogram.len()
            + self.frame_rms_histogram.len()
    }

    pub fn finish(mut self) -> Result<AnalysisOutput, AudioExtractionError> {
        if self.samples_seen == 0 {
            return Err(AudioExtractionError::EmptyAudio);
        }
        if !self.frame_buffer.is_empty() {
            self.frame_buffer.resize(WINDOW_SIZE, 0.0);
            self.analyze_frame();
        }
        let duration_seconds = self.samples_seen as f64 / f64::from(self.sample_rate);
        let loudness_rms = (self.sum_squares / self.samples_seen as f64).sqrt();
        let dynamic_range = histogram_percentile(&self.frame_rms_histogram, 0.9)
            - histogram_percentile(&self.frame_rms_histogram, 0.1);
        let divisor = self.spectral_frames.max(1) as f64;
        let tempo = self.tempo_estimate()?;
        let evidence = AudioEvidence::new(
            loudness_rms,
            dynamic_range.max(0.0),
            self.spectral_centroid_sum / divisor,
            self.spectral_rolloff_sum / divisor,
            self.spectral_flux_sum / divisor,
            self.onset_count as f64 / duration_seconds,
            tempo,
        )?;
        let profile = project_profile(&evidence)?;
        Ok(AnalysisOutput {
            evidence,
            profile,
            waveform_peaks: finish_waveform(&self.waveform_sum_squares, &self.waveform_counts),
        })
    }

    fn add_waveform_sample(&mut self, sample: f64) {
        let buckets = self.waveform_sum_squares.len() as u64;
        let denominator = self.expected_samples;
        let bucket =
            ((self.samples_seen * buckets) / denominator).min(buckets.saturating_sub(1)) as usize;
        self.waveform_sum_squares[bucket] += sample * sample;
        self.waveform_counts[bucket] += 1;
    }

    fn analyze_frame(&mut self) {
        let rms = (self
            .frame_buffer
            .iter()
            .map(|sample| sample * sample)
            .sum::<f64>()
            / WINDOW_SIZE as f64)
            .sqrt();
        let rms_bin = ((rms.clamp(0.0, 1.0) * RMS_HISTOGRAM_BINS as f64) as usize)
            .min(RMS_HISTOGRAM_BINS - 1);
        self.frame_rms_histogram[rms_bin] += 1;
        if rms >= ONSET_RMS_FLOOR
            && rms > self.previous_frame_rms * ONSET_RISE_FACTOR
            && self
                .last_onset_sample
                .is_none_or(|last| self.frame_start.saturating_sub(last) >= HOP_SIZE as u64)
        {
            self.record_onset();
        }
        self.previous_frame_rms = rms;

        let magnitudes = spectrum(&self.frame_buffer);
        let magnitude_sum = magnitudes.iter().sum::<f64>();
        if magnitude_sum > f64::EPSILON {
            let bin_hz = f64::from(self.sample_rate) / WINDOW_SIZE as f64;
            let centroid = magnitudes
                .iter()
                .enumerate()
                .map(|(bin, magnitude)| bin as f64 * bin_hz * magnitude)
                .sum::<f64>()
                / magnitude_sum;
            let rolloff_target = magnitude_sum * ROLLOFF_FRACTION;
            let mut cumulative = 0.0;
            let mut rolloff = 0.0;
            for (bin, magnitude) in magnitudes.iter().enumerate() {
                cumulative += magnitude;
                if cumulative >= rolloff_target {
                    rolloff = bin as f64 * bin_hz;
                    break;
                }
            }
            let normalized = magnitudes
                .iter()
                .map(|magnitude| magnitude / magnitude_sum)
                .collect::<Vec<_>>();
            let flux = normalized
                .iter()
                .zip(&self.previous_spectrum)
                .map(|(current, previous)| (current - previous).max(0.0))
                .sum::<f64>();
            self.spectral_centroid_sum += centroid;
            self.spectral_rolloff_sum += rolloff;
            self.spectral_flux_sum += flux;
            self.previous_spectrum = normalized;
            self.spectral_frames += 1;
        }
    }

    fn record_onset(&mut self) {
        self.onset_count += 1;
        if let Some(previous) = self.last_onset_sample {
            let interval = self.frame_start.saturating_sub(previous);
            if interval > 0 {
                let bpm = (60.0 * f64::from(self.sample_rate) / interval as f64).round() as usize;
                if (MIN_TEMPO_BPM..=MAX_TEMPO_BPM).contains(&bpm) {
                    self.tempo_histogram[bpm] += 1;
                    self.tempo_intervals += 1;
                }
            }
        }
        self.last_onset_sample = Some(self.frame_start);
    }

    fn tempo_estimate(&self) -> Result<Option<TempoEstimate>, AudioExtractionError> {
        let Some((bpm, count)) = self.tempo_histogram[MIN_TEMPO_BPM..=MAX_TEMPO_BPM]
            .iter()
            .enumerate()
            .max_by_key(|(_, count)| *count)
            .map(|(offset, count)| (offset + MIN_TEMPO_BPM, *count))
            .filter(|(_, count)| *count > 0)
        else {
            return Ok(None);
        };
        let confidence = count as f64 / self.tempo_intervals.max(1) as f64;
        Ok(Some(TempoEstimate::new(bpm as f64, confidence)?))
    }
}

pub fn project_profile(evidence: &AudioEvidence) -> Result<SoundProfile, AudioExtractionError> {
    let intensity = (evidence.loudness_rms() * INTENSITY_RMS_SCALE).clamp(0.0, 1.0);
    let brightness = (CENTROID_BRIGHTNESS_WEIGHT * evidence.spectral_centroid_hz()
        / PROFILE_NYQUIST_HZ
        + ROLLOFF_BRIGHTNESS_WEIGHT * evidence.spectral_rolloff_hz() / PROFILE_NYQUIST_HZ)
        .clamp(0.0, 1.0);
    let dynamicity = (evidence.dynamic_range() * DYNAMIC_RANGE_SCALE).clamp(0.0, 1.0);
    let tempo_confidence = evidence
        .tempo()
        .map_or(0.0, |tempo| tempo.confidence().get());
    let rhythmicity = (evidence.onset_rate() / ONSET_RATE_REFERENCE * ONSET_RHYTHMICITY_WEIGHT
        + tempo_confidence * TEMPO_RHYTHMICITY_WEIGHT)
        .clamp(0.0, 1.0);
    Ok(SoundProfile::new(
        ProfileDimension::new(intensity, 1.0)?,
        ProfileDimension::new(
            brightness,
            if evidence.spectral_centroid_hz() > 0.0 {
                0.9
            } else {
                0.2
            },
        )?,
        ProfileDimension::new(dynamicity, 0.8)?,
        ProfileDimension::new(
            rhythmicity,
            if tempo_confidence > 0.0 {
                tempo_confidence
            } else {
                0.4
            },
        )?,
    ))
}

fn histogram_percentile(histogram: &[u64; RMS_HISTOGRAM_BINS], percentile: f64) -> f64 {
    let total = histogram.iter().sum::<u64>();
    if total == 0 {
        return 0.0;
    }
    let target = (total as f64 * percentile).ceil() as u64;
    let mut cumulative = 0;
    for (index, count) in histogram.iter().enumerate() {
        cumulative += count;
        if cumulative >= target {
            return index as f64 / (RMS_HISTOGRAM_BINS - 1) as f64;
        }
    }
    1.0
}

fn finish_waveform(sum_squares: &[f64], counts: &[u64]) -> Vec<u8> {
    let rms = sum_squares
        .iter()
        .zip(counts)
        .map(|(sum, count)| {
            if *count == 0 {
                0.0
            } else {
                (sum / *count as f64).sqrt()
            }
        })
        .collect::<Vec<_>>();
    let maximum = rms.iter().copied().fold(0.0_f64, f64::max);
    if maximum <= f64::EPSILON {
        return vec![0; rms.len()];
    }
    rms.into_iter()
        .map(|value| (value / maximum * 255.0).round() as u8)
        .collect()
}

fn spectrum(samples: &[f64]) -> Vec<f64> {
    let mut values = samples
        .iter()
        .enumerate()
        .map(|(index, sample)| {
            let window =
                0.5 - 0.5 * (std::f64::consts::TAU * index as f64 / (WINDOW_SIZE - 1) as f64).cos();
            (*sample * window, 0.0)
        })
        .collect::<Vec<_>>();
    fft_in_place(&mut values);
    values[..=WINDOW_SIZE / 2]
        .iter()
        .map(|(real, imaginary)| real.hypot(*imaginary))
        .collect()
}

fn fft_in_place(values: &mut [(f64, f64)]) {
    let length = values.len();
    let mut target = 0;
    for source in 1..length {
        let mut bit = length >> 1;
        while target & bit != 0 {
            target ^= bit;
            bit >>= 1;
        }
        target ^= bit;
        if source < target {
            values.swap(source, target);
        }
    }
    let mut span = 2;
    while span <= length {
        let angle = -std::f64::consts::TAU / span as f64;
        let step = (angle.cos(), angle.sin());
        for start in (0..length).step_by(span) {
            let mut twiddle = (1.0, 0.0);
            for offset in 0..span / 2 {
                let even = values[start + offset];
                let odd = values[start + offset + span / 2];
                let rotated = (
                    odd.0 * twiddle.0 - odd.1 * twiddle.1,
                    odd.0 * twiddle.1 + odd.1 * twiddle.0,
                );
                values[start + offset] = (even.0 + rotated.0, even.1 + rotated.1);
                values[start + offset + span / 2] = (even.0 - rotated.0, even.1 - rotated.1);
                twiddle = (
                    twiddle.0 * step.0 - twiddle.1 * step.1,
                    twiddle.0 * step.1 + twiddle.1 * step.0,
                );
            }
        }
        span *= 2;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AudioExtractionError {
    #[error("sample rate and waveform bucket count must be greater than zero")]
    InvalidConfiguration,
    #[error("audio stream contains more samples than declared")]
    TooManySamples,
    #[error("audio stream contains a non-finite sample")]
    NonFiniteSample,
    #[error("audio stream is empty")]
    EmptyAudio,
    #[error(transparent)]
    InvalidEvidence(#[from] SoundProfileError),
}
