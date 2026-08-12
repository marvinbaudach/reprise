//! Allocation-light Android boundary for the shared song visualizer.
//!
//! A scene is flattened as one record per shape:
//! `[kind, r, g, b, a, width, glow, point_count, geometry...]`.
//! `kind` is `0` for a rectangle (`x, y, w, h`), `1` for a polyline
//! (`x1, y1, ...`), and `2` for a radial glow (`cx, cy, radius`). The rectangle
//! and radial-glow `point_count` fields are respectively `4` and `3`, matching
//! the number of geometry scalars rather than a literal number of points.
//!
//! The shared scene format also carries closed-path and dash metadata. Bars do
//! not use either, so this boundary deliberately omits them rather than adding
//! fields the phone would copy on every rendered frame.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError};
use std::time::{Duration, Instant};

use reprise_core::playback::{
    BassPressure, BassPressureDetector, CavaBarProcessor, CavaConfig, SpectrumFrame,
    SPECTRUM_BAND_COUNT,
};
use reprise_core::visuals::{spectrum_frame_from_bands, Fill, Geom, Scene, VisualEngine};

const RECT_KIND: f32 = 0.0;
const POLYLINE_KIND: f32 = 1.0;
const RADIAL_GLOW_KIND: f32 = 2.0;
const RECORD_PREFIX_LEN: usize = 8;
const MAX_PCM_CHANNEL_COUNT: usize = 32;
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
    bands: [f32; SPECTRUM_BAND_COUNT],
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
            bands: [0.0; SPECTRUM_BAND_COUNT],
        })
    }

    fn process_pcm_i16(
        &mut self,
        bytes: &[u8],
        frame_bytes: usize,
        channel_count: usize,
    ) -> (SpectrumFrame, BassPressure) {
        let frame_count = bytes.len() / frame_bytes;
        self.mono_samples.clear();
        self.mono_samples.reserve(frame_count);
        for frame in bytes.chunks_exact(frame_bytes) {
            let sum = frame
                .chunks_exact(size_of::<i16>())
                .map(|sample| i16::from_le_bytes([sample[0], sample[1]]) as f32)
                .sum::<f32>();
            self.mono_samples
                .push(sum / channel_count as f32 / 32_768.0);
        }
        self.processor
            .process_into(&self.mono_samples, &mut self.bands);
        let pressure = self.pressure_detector.observe(&self.mono_samples);
        (
            SpectrumFrame::from_cava_bars(self.bands).with_bass_pressure(pressure),
            pressure,
        )
    }

    fn reset(&mut self) {
        self.processor.reset();
        self.pressure_detector.reset();
        self.mono_samples.clear();
        self.bands.fill(0.0);
    }
}

struct VisualState {
    engine: VisualEngine,
    stream_generation: u64,
    has_ingested: bool,
    has_analysis: bool,
    has_live_audio: bool,
    last_live_audio_at: Option<Duration>,
    live_pressure: BassPressure,
    playing: bool,
}

/// One phone-local owner of the portable visual engine.
#[derive(uniffi::Object)]
pub struct AndroidVisualEngine {
    state: Mutex<VisualState>,
    live_audio: Mutex<Option<LiveAudioState>>,
    stream_generation: AtomicU64,
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

    pub fn set_playing(&self, playing: bool) {
        let mut state = self.lock();
        let stream_generation = self.current_stream_generation();
        reconcile_stream_generation(&mut state, stream_generation);
        state.playing = playing;
        expire_stale_live_audio(&mut state, self.clock.now());
        let has_audio = state.has_analysis || state.has_live_audio;
        state.engine.set_playing(playing && has_audio);
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
        state.has_ingested = false;
        state.has_analysis = false;
        reset_live_presentation(&mut state, stream_generation);
    }

    /// Installs one already-smoothed spectrogram frame.
    #[allow(clippy::needless_pass_by_value)] // UniFFI cannot export borrowed slices.
    pub fn ingest_bands(&self, bands: Vec<f32>) {
        let has_analysis = !bands.is_empty();
        let frame = spectrum_frame_from_bands(&bands);
        let mut state = self.lock();
        let stream_generation = self.current_stream_generation();
        reconcile_stream_generation(&mut state, stream_generation);
        expire_stale_live_audio(&mut state, self.clock.now());
        if state.has_live_audio {
            return;
        }
        state.engine.set_has_track(true);
        let playing = state.playing;
        state.engine.set_playing(playing && has_analysis);
        state.engine.ingest(&frame);
        state.has_ingested = true;
        state.has_analysis = has_analysis;
    }

    /// Downmixes interleaved little-endian PCM16 and feeds the shared CAVA path.
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
        // Downmix, FFT and bass detection have their own audio-thread state.
        // Contention drops a frame rather than blocking Media3.
        let Some(mut live_audio_slot) = self.try_lock_live_audio() else {
            return false;
        };
        let Some(live_audio) =
            live_processor_for_stream(&mut live_audio_slot, stream_generation, sample_rate_hz)
        else {
            return false;
        };
        let (frame, pressure) =
            live_audio.process_pcm_i16(&bytes[..byte_count], frame_bytes, channel_count);
        drop(live_audio_slot);

        if self.current_stream_generation() != stream_generation {
            return false;
        }
        // Only the finished frame crosses into display-thread state.
        let Some(mut state) = self.try_lock() else {
            return false;
        };
        if self.current_stream_generation() != stream_generation {
            return false;
        }
        reconcile_stream_generation(&mut state, stream_generation);

        state.engine.set_has_track(true);
        let playing = state.playing;
        state.engine.set_playing(playing);
        state.engine.ingest(&frame);
        state.has_ingested = true;
        state.has_live_audio = true;
        state.last_live_audio_at = Some(self.clock.now());
        state.live_pressure = pressure;
        true
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
            reset_live_presentation(&mut state, stream_generation);
        }
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

    /// Advances the portable presentation state; `true` means it is settled.
    pub fn tick(&self) -> bool {
        self.lock().engine.tick()
    }

    /// Returns the scene in the flat format documented by this module.
    pub fn scene(&self, width: f32, height: f32) -> Vec<f32> {
        let state = self.lock();
        if !state.has_ingested
            || !width.is_finite()
            || !height.is_finite()
            || width <= 0.0
            || height <= 0.0
        {
            return Vec::new();
        }
        encode_scene(&state.engine.scene(width, height))
    }
}

fn silent_pressure() -> BassPressure {
    BassPressureDetector::new(1).observe(&[])
}

fn reset_live_presentation(state: &mut VisualState, stream_generation: u64) {
    state.stream_generation = stream_generation;
    state.has_live_audio = false;
    state.last_live_audio_at = None;
    state.live_pressure = silent_pressure();
    state
        .engine
        .set_playing(state.playing && state.has_analysis);
}

fn reconcile_stream_generation(state: &mut VisualState, stream_generation: u64) {
    if state.stream_generation != stream_generation {
        reset_live_presentation(state, stream_generation);
    }
}

fn reset_live_processor(live_audio: &mut Option<LiveAudioState>, stream_generation: u64) {
    if let Some(live_audio) = live_audio.as_mut() {
        live_audio.reset();
        live_audio.stream_generation = stream_generation;
    }
}

fn live_processor_for_stream(
    live_audio: &mut Option<LiveAudioState>,
    stream_generation: u64,
    sample_rate_hz: u32,
) -> Option<&mut LiveAudioState> {
    let replace = live_audio
        .as_ref()
        .is_none_or(|state| state.sample_rate_hz != sample_rate_hz);
    if replace {
        *live_audio = LiveAudioState::new(stream_generation, sample_rate_hz);
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
        reset_live_presentation(state, state.stream_generation);
    }
}

fn live_audio_is_current(state: &VisualState, now: Duration) -> bool {
    state.has_live_audio
        && (!state.playing
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
        Self {
            state: Mutex::new(VisualState {
                engine: VisualEngine::new(),
                stream_generation: 0,
                has_ingested: false,
                has_analysis: false,
                has_live_audio: false,
                last_live_audio_at: None,
                live_pressure: silent_pressure(),
                playing: false,
            }),
            live_audio: Mutex::new(None),
            stream_generation: AtomicU64::new(0),
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

    fn current_stream_generation(&self) -> u64 {
        self.stream_generation.load(Ordering::Acquire)
    }

    fn advance_stream_generation(&self) -> u64 {
        self.stream_generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
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

pub(crate) fn encode_scene(scene: &Scene) -> Vec<f32> {
    let geometry_len = scene
        .shapes
        .iter()
        .map(|shape| match &shape.geom {
            Geom::Rect { .. } => 4,
            Geom::Polyline { points, .. } => points.len() * 2,
            Geom::RadialGlow { .. } => 3,
        })
        .sum::<usize>();
    let mut buffer = Vec::with_capacity(scene.shapes.len() * RECORD_PREFIX_LEN + geometry_len);

    for shape in &scene.shapes {
        let Fill::Solid(color) = &shape.fill;
        let (kind, point_count) = match &shape.geom {
            Geom::Rect { .. } => (RECT_KIND, 4),
            Geom::Polyline { points, .. } => (POLYLINE_KIND, points.len()),
            Geom::RadialGlow { .. } => (RADIAL_GLOW_KIND, 3),
        };
        buffer.extend_from_slice(&[
            kind,
            color.r,
            color.g,
            color.b,
            color.a,
            shape.width,
            shape.glow,
            point_count as f32,
        ]);
        match &shape.geom {
            Geom::Rect { x, y, w, h } => buffer.extend_from_slice(&[*x, *y, *w, *h]),
            Geom::Polyline { points, .. } => {
                for (x, y) in points {
                    buffer.extend_from_slice(&[*x, *y]);
                }
            }
            Geom::RadialGlow { cx, cy, r } => buffer.extend_from_slice(&[*cx, *cy, *r]),
        }
    }

    buffer
}
