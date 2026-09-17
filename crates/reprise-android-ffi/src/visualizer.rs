//! Allocation-light Android boundary for the shared song visualizer.
//!
//! The flat byte layout a scene is encoded into lives in [`scene_encoding`],
//! whose module doc describes the record format the phone reads.

mod scene_encoding;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError};
use std::time::{Duration, Instant};

use reprise_core::playback::{
    BassPressure, BassPressureDetector, CavaBarProcessor, CavaConfig, SpectrumFrame,
    SPECTRUM_BAND_COUNT,
};
use reprise_core::visuals::{spectrum_frame_from_bands, VisualEngine};

pub(crate) use scene_encoding::encode_scene;

const MAX_PCM_CHANNEL_COUNT: usize = 32;
const LIVE_PCM_BUFFER_SECONDS: usize = 2;
// This is the single tuning knob for the stable visual delay behind decoded PCM.
const TARGET_PCM_BUFFER_DURATION: Duration = Duration::from_millis(250);
// A 30-tick proportional horizon corrects clock drift without visible speed changes.
const PCM_BUFFER_CONTROLLER_TAU_TICKS: f64 = 30.0;
const MIN_PCM_CONSUMPTION_RATE: f64 = 0.9;
const MAX_PCM_CONSUMPTION_RATE: f64 = 1.1;
pub(crate) const LIVE_AUDIO_STALE_AFTER: Duration = Duration::from_millis(500);

pub(crate) trait MonotonicClock: Send + Sync {
    fn now(&self) -> Duration;
}

struct SystemMonotonicClock {
    started_at: Instant,
}

impl SystemMonotonicClock {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}

impl MonotonicClock for SystemMonotonicClock {
    fn now(&self) -> Duration {
        self.started_at.elapsed()
    }
}

#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct AndroidBassPressure {
    pub level_dbfs: f32,
    pub baseline_dbfs: f32,
    pub impact: f32,
    pub aura: f32,
    pub kick: f32,
    pub pressure: f32,
}

impl From<BassPressure> for AndroidBassPressure {
    fn from(value: BassPressure) -> Self {
        Self {
            level_dbfs: value.level_dbfs,
            baseline_dbfs: value.baseline_dbfs,
            impact: value.impact,
            aura: value.aura,
            kick: value.kick,
            pressure: value.pressure,
        }
    }
}

impl AndroidBassPressure {
    fn silent() -> Self {
        silent_pressure().into()
    }
}

struct LiveAudioState {
    stream_generation: u64,
    sample_rate_hz: u32,
    processor: CavaBarProcessor,
    pressure_detector: BassPressureDetector,
    mono_samples: Vec<f32>,
    pcm_buffer: PcmRingBuffer,
    bands: [f32; SPECTRUM_BAND_COUNT],
}

struct PcmRingBuffer {
    samples: VecDeque<f32>,
    capacity: usize,
}

impl PcmRingBuffer {
    fn new(sample_rate_hz: u32) -> Self {
        let capacity = sample_rate_hz as usize * LIVE_PCM_BUFFER_SECONDS;
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn append(&mut self, samples: &[f32]) {
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
    fn new(stream_generation: u64, sample_rate_hz: u32) -> Option<Self> {
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

    fn buffer_pcm_i16(&mut self, bytes: &[u8], frame_bytes: usize, channel_count: usize) {
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

    fn analyze_elapsed(&mut self, elapsed: Duration) -> Option<(SpectrumFrame, BassPressure)> {
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

    fn reset(&mut self) {
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

struct VisualState {
    engine: VisualEngine,
    engine_playing: bool,
    stream_generation: u64,
    has_ingested: bool,
    has_analysis: bool,
    has_adopted_shape: bool,
    has_live_audio: bool,
    last_live_audio_at: Option<Duration>,
    live_pressure: BassPressure,
    playing: bool,
    playback_intent: PlaybackIntent,
    last_visual_tick_at: Duration,
}

impl VisualState {
    fn set_engine_playing(&mut self, playing: bool, now: Duration) {
        if self.engine_playing != playing {
            self.engine_playing = playing;
            self.last_visual_tick_at = now;
        }
        self.engine.set_playing(playing);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PlaybackIntent {
    Unknown,
    Playing,
    Paused,
}

/// One phone-local owner of the portable visual engine.
#[derive(uniffi::Object)]
pub struct AndroidVisualEngine {
    state: Mutex<VisualState>,
    live_audio: Mutex<Option<LiveAudioState>>,
    // A shape [`AndroidVisualEngine::adopt_shape`] could not seed into a live
    // processor yet because none exists (the engine is fresh and no PCM has
    // arrived). Applied the moment `live_processor_for_stream` creates one.
    // Lock order: `live_audio`, then this, then `state` — the same "audio
    // before display" order the rest of this module already keeps.
    pending_shape_seed: Mutex<Option<[f32; SPECTRUM_BAND_COUNT]>>,
    stream_generation: AtomicU64,
    dropped_audio_frames: AtomicU64,
    clock: Arc<dyn MonotonicClock>,
}

#[uniffi::export]
impl AndroidVisualEngine {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self::with_monotonic_clock(Arc::new(SystemMonotonicClock::new()))
    }

    pub fn set_accent(&self, red: f32, green: f32, blue: f32) {
        self.lock()
            .engine
            .set_accent((finite_unit(red), finite_unit(green), finite_unit(blue)));
    }

    /// Sets whether the visual scene should keep evolving instead of releasing.
    ///
    /// Kotlin must pass its snapshot-derived `visualizerActive` signal here:
    /// true for both Playing and Buffering, false for Paused and Stopped. Raw
    /// Media3 `isPlaying` is deliberately wrong because it drops while a
    /// play-intended stream buffers.
    pub fn set_playing(&self, playing: bool) {
        let mut state = self.lock();
        let stream_generation = self.current_stream_generation();
        let now = self.clock.now();
        reconcile_stream_generation(&mut state, stream_generation, now);
        if state.playing != playing {
            state.last_visual_tick_at = now;
        }
        state.playing = playing;
        expire_stale_live_audio(&mut state, now);
        let has_audio = state.has_analysis || state.has_adopted_shape || state.has_live_audio;
        state.set_engine_playing(playing && has_audio, now);
    }

    /// Records Media3's raw `playWhenReady` intent for live-PCM freshness.
    ///
    /// Unlike [`Self::set_playing`], this input comes directly from the audio
    /// sink's Player listener rather than the Android playback snapshot. It
    /// decides whether staleness time continues while Media3 is not producing
    /// PCM; it does not drive the visual scene's play/release state.
    pub fn set_playback_intended(&self, playback_intended: bool) {
        let mut state = self.lock();
        let stream_generation = self.current_stream_generation();
        let now = self.clock.now();
        reconcile_stream_generation(&mut state, stream_generation, now);
        let resumed = playback_intended && state.playback_intent != PlaybackIntent::Playing;
        state.playback_intent = if playback_intended {
            PlaybackIntent::Playing
        } else {
            PlaybackIntent::Paused
        };
        if resumed && state.has_live_audio {
            state.last_live_audio_at = Some(now);
        }
        expire_stale_live_audio(&mut state, now);
    }

    /// Starts a clean visual history for the next track.
    pub fn note_track_changed(&self) {
        self.advance_stream_generation();
        if let Some(mut live_audio) = self.try_lock_live_audio() {
            let stream_generation = self.current_stream_generation();
            reset_live_processor(&mut live_audio, stream_generation);
        }
        let mut state = self.lock();
        let stream_generation = self.current_stream_generation();
        state.engine.note_track_changed();
        state.engine.set_has_track(false);
        let now = self.clock.now();
        state.last_visual_tick_at = now;
        state.has_ingested = false;
        state.has_analysis = false;
        reset_live_presentation(&mut state, stream_generation, now);
    }

    /// Installs one already-smoothed spectrogram frame.
    #[allow(clippy::needless_pass_by_value)] // UniFFI cannot export borrowed slices.
    pub fn ingest_bands(&self, bands: Vec<f32>) {
        let has_analysis = !bands.is_empty();
        let frame = spectrum_frame_from_bands(&bands);
        let mut state = self.lock();
        let stream_generation = self.current_stream_generation();
        let now = self.clock.now();
        reconcile_stream_generation(&mut state, stream_generation, now);
        expire_stale_live_audio(&mut state, now);
        if state.has_live_audio {
            return;
        }
        state.engine.set_retain_paused_live_shape(false);
        state.engine.set_has_track(true);
        let playing = state.playing;
        state.set_engine_playing(playing && has_analysis, now);
        state.engine.ingest(&frame);
        state.has_ingested = true;
        state.has_analysis = has_analysis;
    }

    /// The engine's currently displayed bar values: the live CAVA bands while
    /// live audio drives it, the ingested spectrogram bands otherwise.
    ///
    /// A panel taking over the live slot during a swipe reads this off the
    /// engine it replaces and hands it to [`Self::adopt_shape`] on its own,
    /// freshly created engine, so the new engine's first frames continue from
    /// the outgoing engine's shape instead of climbing from zero.
    pub fn current_bands(&self) -> Vec<f32> {
        self.lock().engine.current_bands().to_vec()
    }

    /// Seeds a freshly created engine with another engine's bar shape.
    ///
    /// Installs the shape into the portable engine immediately — as an
    /// ingested, track-loaded frame, playing according to this engine's own
    /// `playing` flag, but deliberately not marked as live audio — so
    /// [`Self::scene`] draws it at once instead of the empty pre-ingest
    /// scene. It also seeds the live CAVA processor's bar-shape memory, so
    /// the first frames analyzed from real PCM fall from this shape rather
    /// than climbing from zero; the seed is applied immediately if a live
    /// processor already exists, or held until the first PCM block creates
    /// one otherwise (this engine has no live audio yet, so there is
    /// normally nothing to seed immediately). Empty input is a no-op.
    #[allow(clippy::needless_pass_by_value)] // UniFFI cannot export borrowed slices.
    pub fn adopt_shape(&self, bands: Vec<f32>) {
        if bands.is_empty() {
            return;
        }
        let frame = spectrum_frame_from_bands(&bands);
        let seed = *frame.bands();

        {
            let mut live_audio = self.lock_live_audio();
            if let Some(live_audio) = live_audio.as_mut() {
                live_audio.processor.seed_shape(&seed);
            } else {
                *self.lock_pending_shape_seed() = Some(seed);
            }
        }

        let mut state = self.lock();
        let now = self.clock.now();
        state.engine.set_has_track(true);
        let playing = state.playing;
        state.set_engine_playing(playing, now);
        state.engine.ingest(&frame);
        state.has_ingested = true;
        state.has_adopted_shape = true;
    }

    /// Downmixes interleaved little-endian PCM16 into the live-audio ring buffer.
    #[allow(clippy::needless_pass_by_value)] // UniFFI cannot export borrowed byte slices.
    pub fn ingest_pcm_i16(
        &self,
        bytes: Vec<u8>,
        byte_count: u32,
        sample_rate_hz: u32,
        channel_count: u32,
    ) -> bool {
        let byte_count = byte_count as usize;
        let channel_count = channel_count as usize;
        let frame_bytes = channel_count.saturating_mul(size_of::<i16>());
        if byte_count == 0
            || byte_count > bytes.len()
            || channel_count == 0
            || channel_count > MAX_PCM_CHANNEL_COUNT
            || frame_bytes == 0
            || !byte_count.is_multiple_of(frame_bytes)
        {
            return false;
        }

        let stream_generation = self.current_stream_generation();
        // Downmixing and buffering have their own audio-thread state. Contention
        // drops a block rather than blocking Media3.
        let Some(mut live_audio_slot) = self.try_lock_live_audio() else {
            self.count_dropped_audio_frame();
            return false;
        };
        let Some(live_audio) = ({
            let mut pending_shape_seed = self.lock_pending_shape_seed();
            live_processor_for_stream(
                &mut live_audio_slot,
                stream_generation,
                sample_rate_hz,
                &mut pending_shape_seed,
            )
        }) else {
            return false;
        };
        live_audio.buffer_pcm_i16(&bytes[..byte_count], frame_bytes, channel_count);

        if self.current_stream_generation() != stream_generation {
            let current_generation = self.current_stream_generation();
            reset_live_processor(&mut live_audio_slot, current_generation);
            return false;
        }
        true
    }

    /// Returns the cumulative count of live-audio frames dropped on lock contention.
    pub fn dropped_audio_frames(&self) -> u64 {
        self.dropped_audio_frames.load(Ordering::Relaxed)
    }

    /// Drops all CAVA and bass-detector history at a decoded-stream boundary.
    pub fn reset_audio_stream(&self) {
        self.advance_stream_generation();
        if let Some(mut live_audio) = self.try_lock_live_audio() {
            let stream_generation = self.current_stream_generation();
            reset_live_processor(&mut live_audio, stream_generation);
        }
        if let Some(mut state) = self.try_lock() {
            let stream_generation = self.current_stream_generation();
            reset_live_presentation(&mut state, stream_generation, self.clock.now());
        }
    }

    /// Drops decoder history on resume without discarding the last live scene.
    pub fn reset_audio_history(&self) {
        // Keep the same lock order as live PCM ingestion and ticking: audio
        // before display. Taking both before the generation changes keeps a
        // later reconciliation from mistaking this reset for a stream boundary.
        let mut live_audio = self.lock_live_audio();
        let mut state = self.lock();
        let stream_generation = self.advance_stream_generation();
        reset_live_processor(&mut live_audio, stream_generation);
        state.stream_generation = stream_generation;
        state.has_adopted_shape = false;
        state.last_live_audio_at = state.has_live_audio.then(|| self.clock.now());
        state.live_pressure = silent_pressure();
    }

    pub fn has_live_audio(&self) -> bool {
        let stream_generation = self.current_stream_generation();
        let mut state = self.lock();
        expire_stale_live_audio(&mut state, self.clock.now());
        self.current_stream_generation() == stream_generation
            && state.stream_generation == stream_generation
            && state.has_live_audio
    }

    pub fn bass_pressure(&self) -> AndroidBassPressure {
        let stream_generation = self.current_stream_generation();
        let mut state = self.lock();
        expire_stale_live_audio(&mut state, self.clock.now());
        if self.current_stream_generation() != stream_generation
            || state.stream_generation != stream_generation
        {
            AndroidBassPressure::silent()
        } else if state.playing && state.has_live_audio {
            state.live_pressure.into()
        } else {
            AndroidBassPressure::silent()
        }
    }

    /// Advances the portable presentation state by monotonic elapsed time.
    ///
    /// A newly analyzed live-audio frame always reports `true`, even when the
    /// portable engine would otherwise report a settled presentation.
    pub fn tick(&self) -> bool {
        // Keep the same lock order as live PCM ingestion: audio before display.
        let mut live_audio_slot = self.lock_live_audio();
        let mut state = self.lock();
        let stream_generation = self.current_stream_generation();
        let now = self.clock.now();
        let elapsed = now.saturating_sub(state.last_visual_tick_at);
        state.last_visual_tick_at = now;
        reconcile_stream_generation(&mut state, stream_generation, now);

        let live_frame = state.playing.then(|| {
            live_audio_slot.as_mut().and_then(|live_audio| {
                if live_audio.stream_generation != stream_generation {
                    live_audio.reset();
                    live_audio.stream_generation = stream_generation;
                    return None;
                }
                live_audio.analyze_elapsed(elapsed)
            })
        });
        let live_frame = live_frame.flatten();
        let ingested_live_frame = if let Some((frame, pressure)) = live_frame {
            state.engine.set_retain_paused_live_shape(true);
            state.engine.set_has_track(true);
            let playing = state.playing;
            state.set_engine_playing(playing, now);
            state.engine.ingest(&frame);
            state.has_ingested = true;
            state.has_adopted_shape = false;
            state.has_live_audio = true;
            state.last_live_audio_at = Some(now);
            state.live_pressure = pressure;
            true
        } else {
            false
        };

        state.engine.advance_by(elapsed) || ingested_live_frame
    }

    /// Returns the scene in the flat format documented by this module.
    pub fn scene(&self, width: f32, height: f32) -> Vec<u8> {
        let state = self.lock();
        if !scene_is_drawable(&state, width, height) {
            return Vec::new();
        }
        encode_scene(&state.engine.scene(width, height))
    }

    /// The same scene as [`Self::scene`], painted in the given accent instead
    /// of the engine's own. Empty under exactly the same conditions.
    pub fn scene_tinted(&self, width: f32, height: f32, red: f32, green: f32, blue: f32) -> Vec<u8> {
        let state = self.lock();
        if !scene_is_drawable(&state, width, height) {
            return Vec::new();
        }
        let accent = (finite_unit(red), finite_unit(green), finite_unit(blue));
        encode_scene(&state.engine.scene_with_accent(width, height, accent))
    }
}

fn scene_is_drawable(state: &VisualState, width: f32, height: f32) -> bool {
    state.has_ingested && width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0
}

fn silent_pressure() -> BassPressure {
    BassPressureDetector::new(1).observe(&[])
}

fn reset_live_presentation(state: &mut VisualState, stream_generation: u64, now: Duration) {
    state.stream_generation = stream_generation;
    state.has_adopted_shape = false;
    state.has_live_audio = false;
    state.last_live_audio_at = None;
    state.live_pressure = silent_pressure();
    state.engine.set_retain_paused_live_shape(false);
    let has_audio = state.has_analysis || state.has_adopted_shape;
    state.set_engine_playing(state.playing && has_audio, now);
}

fn reconcile_stream_generation(state: &mut VisualState, stream_generation: u64, now: Duration) {
    if state.stream_generation != stream_generation {
        reset_live_presentation(state, stream_generation, now);
    }
}

fn reset_live_processor(live_audio: &mut Option<LiveAudioState>, stream_generation: u64) {
    if let Some(live_audio) = live_audio.as_mut() {
        live_audio.reset();
        live_audio.stream_generation = stream_generation;
    }
}

fn live_processor_for_stream<'a>(
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

fn expire_stale_live_audio(state: &mut VisualState, now: Duration) {
    if state.has_live_audio && !live_audio_is_current(state, now) {
        reset_live_presentation(state, state.stream_generation, now);
    }
}

fn live_audio_is_current(state: &VisualState, now: Duration) -> bool {
    state.has_live_audio
        && (state.playback_intent == PlaybackIntent::Paused
            || state
                .last_live_audio_at
                .is_some_and(|last| now.saturating_sub(last) < LIVE_AUDIO_STALE_AFTER))
}

impl Default for AndroidVisualEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AndroidVisualEngine {
    fn with_monotonic_clock(clock: Arc<dyn MonotonicClock>) -> Self {
        let now = clock.now();
        Self {
            state: Mutex::new(VisualState {
                engine: VisualEngine::new(),
                engine_playing: false,
                stream_generation: 0,
                has_ingested: false,
                has_analysis: false,
                has_adopted_shape: false,
                has_live_audio: false,
                last_live_audio_at: None,
                live_pressure: silent_pressure(),
                playing: false,
                playback_intent: PlaybackIntent::Unknown,
                last_visual_tick_at: now,
            }),
            live_audio: Mutex::new(None),
            pending_shape_seed: Mutex::new(None),
            stream_generation: AtomicU64::new(0),
            dropped_audio_frames: AtomicU64::new(0),
            clock,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_clock(clock: Arc<dyn MonotonicClock>) -> Self {
        Self::with_monotonic_clock(clock)
    }

    fn lock(&self) -> MutexGuard<'_, VisualState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn try_lock(&self) -> Option<MutexGuard<'_, VisualState>> {
        try_lock_recovering(&self.state)
    }

    fn try_lock_live_audio(&self) -> Option<MutexGuard<'_, Option<LiveAudioState>>> {
        try_lock_recovering(&self.live_audio)
    }

    fn lock_live_audio(&self) -> MutexGuard<'_, Option<LiveAudioState>> {
        self.live_audio
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_pending_shape_seed(&self) -> MutexGuard<'_, Option<[f32; SPECTRUM_BAND_COUNT]>> {
        self.pending_shape_seed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn current_stream_generation(&self) -> u64 {
        self.stream_generation.load(Ordering::Acquire)
    }

    fn advance_stream_generation(&self) -> u64 {
        self.stream_generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
    }

    fn count_dropped_audio_frame(&self) {
        self.dropped_audio_frames.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(test)]
    pub(crate) fn live_bands_for_testing(&self) -> [f32; SPECTRUM_BAND_COUNT] {
        self.live_audio
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map_or([0.0; SPECTRUM_BAND_COUNT], |live_audio| live_audio.bands)
    }

    #[cfg(test)]
    pub(crate) fn with_live_processor_locked_for_testing<T>(&self, test: impl FnOnce() -> T) -> T {
        let _live_audio = self
            .live_audio
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        test()
    }

    #[cfg(test)]
    pub(crate) fn with_state_locked_for_testing<T>(&self, test: impl FnOnce() -> T) -> T {
        let _state = self.lock();
        test()
    }
}

fn try_lock_recovering<T>(mutex: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    match mutex.try_lock() {
        Ok(state) => Some(state),
        Err(TryLockError::Poisoned(error)) => Some(error.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

fn finite_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod pcm_tests {
    include!("visualizer_pcm_tests.rs");
}
