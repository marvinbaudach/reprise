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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError, TryLockError};

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
        Self {
            level_dbfs: -140.0,
            baseline_dbfs: -140.0,
            impact: 0.0,
            aura: 0.0,
            kick: 0.0,
            pressure: 0.0,
        }
    }
}

struct LiveAudioState {
    sample_rate_hz: u32,
    processor: CavaBarProcessor,
    pressure_detector: BassPressureDetector,
    mono_samples: Vec<f32>,
    bands: [f32; SPECTRUM_BAND_COUNT],
}

impl LiveAudioState {
    fn new(sample_rate_hz: u32) -> Option<Self> {
        let processor =
            CavaBarProcessor::new(CavaConfig::new(sample_rate_hz, SPECTRUM_BAND_COUNT)).ok()?;
        Some(Self {
            sample_rate_hz,
            processor,
            pressure_detector: BassPressureDetector::new(sample_rate_hz),
            mono_samples: Vec::new(),
            bands: [0.0; SPECTRUM_BAND_COUNT],
        })
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
    live_audio: Option<LiveAudioState>,
    has_ingested: bool,
    has_analysis: bool,
    has_live_audio: bool,
    live_pressure: BassPressure,
    playing: bool,
}

/// One phone-local owner of the portable visual engine.
#[derive(uniffi::Object)]
pub struct AndroidVisualEngine {
    state: Mutex<VisualState>,
    stream_reset_pending: AtomicBool,
}

#[uniffi::export]
impl AndroidVisualEngine {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(VisualState {
                engine: VisualEngine::new(),
                live_audio: None,
                has_ingested: false,
                has_analysis: false,
                has_live_audio: false,
                live_pressure: silent_pressure(),
                playing: false,
            }),
            stream_reset_pending: AtomicBool::new(false),
        }
    }

    pub fn set_accent(&self, red: f32, green: f32, blue: f32) {
        self.lock()
            .engine
            .set_accent((finite_unit(red), finite_unit(green), finite_unit(blue)));
    }

    pub fn set_playing(&self, playing: bool) {
        let mut state = self.lock();
        state.playing = playing;
        let has_audio = state.has_analysis || state.has_live_audio;
        state.engine.set_playing(playing && has_audio);
    }

    /// Starts a clean visual history for the next track.
    pub fn note_track_changed(&self) {
        let mut state = self.lock();
        self.stream_reset_pending.store(false, Ordering::Release);
        state.engine.note_track_changed();
        state.engine.set_has_track(false);
        state.has_ingested = false;
        state.has_analysis = false;
        reset_live_audio(&mut state);
    }

    /// Installs one already-smoothed spectrogram frame.
    #[allow(clippy::needless_pass_by_value)] // UniFFI cannot export borrowed slices.
    pub fn ingest_bands(&self, bands: Vec<f32>) {
        let has_analysis = !bands.is_empty();
        let frame = spectrum_frame_from_bands(&bands);
        let mut state = self.lock();
        if self.stream_reset_pending.swap(false, Ordering::AcqRel) {
            reset_live_audio(&mut state);
        }
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

        // Scene encoding briefly owns the same state on the display thread.
        // A late spectrum is disposable, while blocking Media3's audio thread
        // is not, so contention is handled as latest-wins frame loss.
        let Some(mut state) = self.try_lock() else {
            return false;
        };
        if self.stream_reset_pending.swap(false, Ordering::AcqRel) {
            reset_live_audio(&mut state);
        }
        let needs_processor = state
            .live_audio
            .as_ref()
            .is_none_or(|live_audio| live_audio.sample_rate_hz != sample_rate_hz);
        if needs_processor {
            let Some(live_audio) = LiveAudioState::new(sample_rate_hz) else {
                return false;
            };
            state.live_audio = Some(live_audio);
        }

        let (bands, pressure) = {
            let live_audio = state
                .live_audio
                .as_mut()
                .expect("live processor was installed");
            let frame_count = byte_count / frame_bytes;
            live_audio.mono_samples.clear();
            live_audio.mono_samples.reserve(frame_count);
            for frame in bytes[..byte_count].chunks_exact(frame_bytes) {
                let sum = frame
                    .chunks_exact(size_of::<i16>())
                    .map(|sample| i16::from_le_bytes([sample[0], sample[1]]) as f32)
                    .sum::<f32>();
                live_audio
                    .mono_samples
                    .push(sum / channel_count as f32 / 32_768.0);
            }
            live_audio
                .processor
                .process_into(&live_audio.mono_samples, &mut live_audio.bands);
            let pressure = live_audio
                .pressure_detector
                .observe(&live_audio.mono_samples);
            (live_audio.bands, pressure)
        };
        let frame = SpectrumFrame::from_cava_bars(bands).with_bass_pressure(pressure);

        state.engine.set_has_track(true);
        let playing = state.playing;
        state.engine.set_playing(playing);
        state.engine.ingest(&frame);
        state.has_ingested = true;
        state.has_live_audio = true;
        state.live_pressure = pressure;
        true
    }

    /// Drops all CAVA and bass-detector history at a decoded-stream boundary.
    pub fn reset_audio_stream(&self) {
        self.stream_reset_pending.store(true, Ordering::Release);
        let Some(mut state) = self.try_lock() else {
            return;
        };
        if self.stream_reset_pending.swap(false, Ordering::AcqRel) {
            reset_live_audio(&mut state);
        }
    }

    pub fn has_live_audio(&self) -> bool {
        if self.stream_reset_pending.load(Ordering::Acquire) {
            return false;
        }
        let state = self.lock();
        !self.stream_reset_pending.load(Ordering::Acquire) && state.has_live_audio
    }

    pub fn bass_pressure(&self) -> AndroidBassPressure {
        if self.stream_reset_pending.load(Ordering::Acquire) {
            return AndroidBassPressure::silent();
        }
        let state = self.lock();
        if self.stream_reset_pending.load(Ordering::Acquire) {
            AndroidBassPressure::silent()
        } else if state.playing {
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

fn reset_live_audio(state: &mut VisualState) {
    if let Some(live_audio) = state.live_audio.as_mut() {
        live_audio.reset();
    }
    state.has_live_audio = false;
    state.live_pressure = silent_pressure();
    state
        .engine
        .set_playing(state.playing && state.has_analysis);
}

impl Default for AndroidVisualEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AndroidVisualEngine {
    fn lock(&self) -> MutexGuard<'_, VisualState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn try_lock(&self) -> Option<MutexGuard<'_, VisualState>> {
        match self.state.try_lock() {
            Ok(state) => Some(state),
            Err(TryLockError::Poisoned(error)) => Some(error.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn live_bands_for_testing(&self) -> [f32; SPECTRUM_BAND_COUNT] {
        self.lock()
            .live_audio
            .as_ref()
            .map_or([0.0; SPECTRUM_BAND_COUNT], |live_audio| live_audio.bands)
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
