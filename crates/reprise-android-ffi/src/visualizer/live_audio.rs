//! Live-PCM bookkeeping for [`super::AndroidVisualEngine`]: the ring buffer
//! that smooths decoder jitter out of Media3's PCM callback and the CAVA/
//! bass-detector processor it feeds, both kept per decoded-stream generation.

use std::collections::VecDeque;
use std::time::Duration;

use reprise_core::playback::{
    BassPressure, BassPressureDetector, CavaBarProcessor, CavaConfig, SpectrumFrame,
    SPECTRUM_BAND_COUNT,
};

const LIVE_PCM_BUFFER_SECONDS: usize = 2;
// This is the single tuning knob for the stable visual delay behind decoded PCM.
pub(crate) const TARGET_PCM_BUFFER_DURATION: Duration = Duration::from_millis(250);
// A 30-tick proportional horizon corrects clock drift without visible speed changes.
const PCM_BUFFER_CONTROLLER_TAU_TICKS: f64 = 30.0;
const MIN_PCM_CONSUMPTION_RATE: f64 = 0.9;
const MAX_PCM_CONSUMPTION_RATE: f64 = 1.1;

pub(crate) struct LiveAudioState {
    pub(crate) stream_generation: u64,
    sample_rate_hz: u32,
    pub(crate) processor: CavaBarProcessor,
    pressure_detector: BassPressureDetector,
    mono_samples: Vec<f32>,
    pub(crate) pcm_buffer: PcmRingBuffer,
    pub(crate) bands: [f32; SPECTRUM_BAND_COUNT],
}

pub(crate) struct PcmRingBuffer {
    pub(crate) samples: VecDeque<f32>,
    pub(crate) capacity: usize,
}

impl PcmRingBuffer {
    fn new(sample_rate_hz: u32) -> Self {
        let capacity = sample_rate_hz as usize * LIVE_PCM_BUFFER_SECONDS;
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub(crate) fn append(&mut self, samples: &[f32]) {
        if samples.len() >= self.capacity {
            self.samples.clear();
            self.samples
                .extend(samples[samples.len() - self.capacity..].iter().copied());
            return;
        }

        let overflow = self
            .samples
            .len()
            .saturating_add(samples.len())
            .saturating_sub(self.capacity);
        self.samples.drain(..overflow);
        self.samples.extend(samples.iter().copied());
    }

    fn clear(&mut self) {
        self.samples.clear();
    }
}

impl LiveAudioState {
    pub(crate) fn new(stream_generation: u64, sample_rate_hz: u32) -> Option<Self> {
        let processor =
            CavaBarProcessor::new(CavaConfig::new(sample_rate_hz, SPECTRUM_BAND_COUNT)).ok()?;
        Some(Self {
            stream_generation,
            sample_rate_hz,
            processor,
            pressure_detector: BassPressureDetector::new(sample_rate_hz),
            mono_samples: Vec::new(),
            pcm_buffer: PcmRingBuffer::new(sample_rate_hz),
            bands: [0.0; SPECTRUM_BAND_COUNT],
        })
    }

    pub(crate) fn buffer_pcm_i16(
        &mut self,
        bytes: &[u8],
        frame_bytes: usize,
        channel_count: usize,
    ) {
        let frame_count = bytes.len() / frame_bytes;
        self.mono_samples.clear();
        self.mono_samples.reserve(frame_count);
        for frame in bytes.chunks_exact(frame_bytes) {
            let sum = frame
                .as_chunks::<{ size_of::<i16>() }>()
                .0
                .iter()
                .map(|sample| i16::from_le_bytes(*sample) as f32)
                .sum::<f32>();
            self.mono_samples
                .push(sum / channel_count as f32 / 32_768.0);
        }
        self.pcm_buffer.append(&self.mono_samples);
    }

    pub(crate) fn analyze_elapsed(
        &mut self,
        elapsed: Duration,
    ) -> Option<(SpectrumFrame, BassPressure)> {
        let target_samples = samples_for_duration(TARGET_PCM_BUFFER_DURATION, self.sample_rate_hz);
        let fill_samples = self.pcm_buffer.samples.len();
        if fill_samples > target_samples.saturating_mul(2) {
            self.pcm_buffer
                .samples
                .drain(..fill_samples - target_samples);
            return None;
        }

        let nominal_samples = elapsed.as_secs_f64() * f64::from(self.sample_rate_hz);
        if nominal_samples == 0.0 {
            return None;
        }
        let correction =
            (fill_samples as f64 - target_samples as f64) / PCM_BUFFER_CONTROLLER_TAU_TICKS;
        let requested_samples = (nominal_samples + correction)
            .clamp(
                nominal_samples * MIN_PCM_CONSUMPTION_RATE,
                nominal_samples * MAX_PCM_CONSUMPTION_RATE,
            )
            .round() as usize;
        let consumed_samples = requested_samples.min(self.pcm_buffer.samples.len());
        if consumed_samples == 0 {
            return None;
        }

        self.mono_samples.clear();
        self.mono_samples
            .extend(self.pcm_buffer.samples.drain(..consumed_samples));
        self.processor
            .process_into(&self.mono_samples, &mut self.bands);
        let pressure = self.pressure_detector.observe(&self.mono_samples);
        Some((
            SpectrumFrame::from_cava_bars(self.bands).with_bass_pressure(pressure),
            pressure,
        ))
    }

    pub(crate) fn reset(&mut self) {
        // A stream boundary must not mix a different track's samples into
        // the FFT window, but it deliberately keeps the smoother's bar shape
        // (see `CavaBarProcessor::reset_stream`): the next analyzed frame
        // then falls from that shape instead of dropping to zero for one
        // frame while the swipe's new panel has not shown anything yet.
        self.processor.reset_stream();
        self.pressure_detector.reset();
        self.mono_samples.clear();
        self.pcm_buffer.clear();
        self.bands.fill(0.0);
    }
}

fn samples_for_duration(duration: Duration, sample_rate_hz: u32) -> usize {
    (duration.as_secs_f64() * f64::from(sample_rate_hz)).round() as usize
}

pub(crate) fn reset_live_processor(
    live_audio: &mut Option<LiveAudioState>,
    stream_generation: u64,
) {
    if let Some(live_audio) = live_audio.as_mut() {
        live_audio.reset();
        live_audio.stream_generation = stream_generation;
    }
}

pub(crate) fn live_processor_for_stream<'a>(
    live_audio: &'a mut Option<LiveAudioState>,
    stream_generation: u64,
    sample_rate_hz: u32,
    pending_shape_seed: &mut Option<[f32; SPECTRUM_BAND_COUNT]>,
) -> Option<&'a mut LiveAudioState> {
    let replace = live_audio
        .as_ref()
        .is_none_or(|state| state.sample_rate_hz != sample_rate_hz);
    if replace {
        *live_audio = LiveAudioState::new(stream_generation, sample_rate_hz);
        if let Some(seed) = pending_shape_seed.take() {
            if let Some(live_audio) = live_audio.as_mut() {
                live_audio.processor.seed_shape(&seed);
            }
        }
    } else if live_audio
        .as_ref()
        .is_some_and(|state| state.stream_generation != stream_generation)
    {
        reset_live_processor(live_audio, stream_generation);
    }
    live_audio.as_mut()
}
