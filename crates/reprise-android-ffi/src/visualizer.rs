//! Allocation-light Android boundary for the shared song visualizer.
//!
//! The flat byte layout a scene is encoded into lives in [`scene_encoding`],
//! whose module doc describes the record format the phone reads.

mod adopted_shape_hold;
mod live_audio;
mod scene_encoding;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError};
use std::time::{Duration, Instant};

use reprise_core::playback::{BassPressure, BassPressureDetector, SPECTRUM_BAND_COUNT};
use reprise_core::visuals::{spectrum_frame_from_bands, VisualEngine};

use adopted_shape_hold::AdoptedShapeHold;
pub(crate) use scene_encoding::encode_scene;

#[cfg(test)]
pub(crate) use live_audio::TARGET_PCM_BUFFER_DURATION;
use live_audio::{
    live_processor_for_stream, reset_live_history, reset_live_processor, LiveAudioState,
};

const MAX_PCM_CHANNEL_COUNT: usize = 32;
pub(crate) const LIVE_AUDIO_STALE_AFTER: Duration = Duration::from_millis(500);
/// How long the phone waits for the transport to answer a committed swipe;
/// mirrors `NOW_PLAYING_ANSWER_GRACE_MS` in `NowPlayingSheet.kt`.
const TRANSPORT_ANSWER_GRACE: Duration = Duration::from_millis(1_500);
/// How long the last live shape stays adoptable: the old stream goes stale
/// after [`LIVE_AUDIO_STALE_AFTER`], and the swipe's new panel may compose up
/// to [`TRANSPORT_ANSWER_GRACE`] after that. A shape older than this is one the
/// viewer saw fall away long ago, which must not pop back on screen.
pub(crate) const ADOPTABLE_SHAPE_MAX_AGE: Duration =
    LIVE_AUDIO_STALE_AFTER.saturating_add(TRANSPORT_ANSWER_GRACE);

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

struct VisualState {
    engine: VisualEngine,
    engine_playing: bool,
    stream_generation: u64,
    has_ingested: bool,
    has_analysis: bool,
    adopted_shape_hold: AdoptedShapeHold,
    has_live_audio: bool,
    // Set by `reset_live_presentation` when it is called for a genuine
    // decoded-stream boundary (`reset_audio_stream`, `note_track_changed`, or
    // any state catching up to one of those through
    // `reconcile_stream_generation`) — never for ordinary live-audio
    // staleness (`expire_stale_live_audio`), which shares that same reset
    // function but must keep its existing paused/idle fallback. Cleared the
    // moment the new stream actually speaks — a live PCM block analyzed in
    // `tick`, or a stored-analysis frame ingested via
    // `ingest_bands`/`adopt_shape`. While set it counts as `has_audio` below,
    // so the engine stays "playing" and its displayed bars simply hold
    // instead of decaying into the idle/paused projection for the gap
    // between the reset and the first new data — the alternative was a
    // visible decay-then-pop. Bounded by `set_playing(false)`, which clears
    // it: a real pause or stop must still fall back to the normal
    // paused/idle projection rather than holding forever. Deliberately not
    // bounded by a timer — a stall while playback is still intended keeps
    // the last picture on screen, which reads better than decaying it away
    // for a gap of unknown length.
    awaiting_stream_after_reset: bool,
    // The bars live PCM last drew while playback ran, stamped with the engine
    // clock time they were drawn at, kept for `adoptable_bands`. A swipe's new
    // panel reads it instead of the displayed bars, which a stop or a
    // transport blip has already decayed toward the resting shape by the time
    // that panel composes. Deliberately outlives `note_track_changed`, which
    // clears the display but not this. The next live tick replaces it, a
    // stored-analysis frame (`ingest_bands`) or a user pause
    // (`set_playback_intended(false)`) clears it, and it expires after
    // `ADOPTABLE_SHAPE_MAX_AGE`: Media3 keeps playback intended through the
    // end of the queue or a stall, so no pause ever clears it there.
    last_live_bands: Option<(Duration, [f32; SPECTRUM_BAND_COUNT])>,
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

    /// The last live shape, unless it is older than `ADOPTABLE_SHAPE_MAX_AGE`
    /// at `now`. The one place that bound is applied.
    fn fresh_live_bands(&self, now: Duration) -> Option<[f32; SPECTRUM_BAND_COUNT]> {
        self.last_live_bands
            .filter(|(drawn_at, _)| now.saturating_sub(*drawn_at) <= ADOPTABLE_SHAPE_MAX_AGE)
            .map(|(_, bands)| bands)
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
        if !playing {
            // A real pause or stop bounds the post-reset hold: nothing further
            // is coming, so the normal paused/idle projection must take over
            // instead of holding the pre-reset picture forever.
            state.awaiting_stream_after_reset = false;
        }
        expire_stale_live_audio(&mut state, now);
        let has_audio = state.has_analysis
            || state.adopted_shape_hold.is_active()
            || state.has_live_audio
            || state.awaiting_stream_after_reset;
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
        if !playback_intended {
            // A user pause, unlike a transport blip, is a real stop: the shape
            // drawn before it is no longer what the viewer sees, and handing
            // it to a swipe's new panel would pop it back at full height.
            state.last_live_bands = None;
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
        // Kotlin calls this before adopting the shape for the same live-slot change.
        *self.lock_pending_shape_seed() = None;
        let mut state = self.lock();
        let stream_generation = self.current_stream_generation();
        state.engine.note_track_changed();
        state.engine.set_has_track(false);
        let now = self.clock.now();
        state.last_visual_tick_at = now;
        state.has_ingested = false;
        state.has_analysis = false;
        reset_live_presentation(&mut state, stream_generation, now, true);
    }

    /// Installs one already-smoothed spectrogram frame.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI cannot export borrowed slices"
    )]
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
        // The new stream has spoken, even when all it said was "nothing": an
        // empty frame must not leave the post-reset hold pinning the old picture.
        state.awaiting_stream_after_reset = false;
        if has_analysis {
            state.last_live_bands = None;
        }
    }

    /// The engine's currently displayed bar values — what is actually on
    /// screen, decayed and idle-blended where applicable, not the raw
    /// last-ingested bands (see [`VisualEngine::current_bands`]).
    ///
    /// A panel taking over the live slot during a swipe adopts
    /// [`Self::adoptable_bands`] rather than this: once the old stream has
    /// stopped, what is displayed has already decayed toward the resting shape.
    pub fn current_bands(&self) -> Vec<f32> {
        self.lock().engine.current_bands().to_vec()
    }

    /// The bar shape a panel taking over the live slot should adopt: the last
    /// shape live PCM drew while playing, or [`Self::current_bands`] when no
    /// live audio has drawn since the last stored-analysis frame.
    ///
    /// The displayed bars are the wrong source once the old stream has
    /// stopped or `set_playing(false)` has blipped through the item change:
    /// they have already decayed toward the resting shape, and adopting that
    /// seeds the new song with a shape the viewer never saw at full height,
    /// so its first PCM block pops. This survives `note_track_changed` and a
    /// `set_playing(false)`; a user pause (`set_playback_intended(false)`)
    /// clears it, because the viewer then sees the resting display. It also
    /// expires once the shape is older than the live-audio staleness plus the
    /// transport's answer grace, so a song that ended or stalled minutes ago
    /// does not resurrect its last picture on a later swipe.
    pub fn adoptable_bands(&self) -> Vec<f32> {
        let state = self.lock();
        let now = self.clock.now();
        state
            .fresh_live_bands(now)
            .unwrap_or(*state.engine.current_bands())
            .to_vec()
    }

    /// Whether [`Self::adoptable_bands`] is the last live shape rather than
    /// the displayed-bars fallback. Exists so the adoption log can name the
    /// source instead of inferring it from equality. A separate call from
    /// `adoptable_bands`, so a live tick between the two can disagree; that
    /// is acceptable for a diagnostic line.
    pub fn adoptable_bands_are_live(&self) -> bool {
        let state = self.lock();
        state.fresh_live_bands(self.clock.now()).is_some()
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
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI cannot export borrowed slices"
    )]
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
        state.engine.adopt_shape(&frame);
        state.has_ingested = true;
        state.adopted_shape_hold.begin();
        state.awaiting_stream_after_reset = false;
    }

    /// Downmixes interleaved little-endian PCM16 into the live-audio ring buffer.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI cannot export borrowed byte slices"
    )]
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
        *self.lock_pending_shape_seed() = None;
        if let Some(mut state) = self.try_lock() {
            let stream_generation = self.current_stream_generation();
            reset_live_presentation(&mut state, stream_generation, self.clock.now(), true);
        }
    }

    /// Drops decoder history on resume without discarding the last live scene or
    /// the gain it was drawn at. Resume follows every `onIsPlayingChanged`
    /// false to true, a buffering stall included, so it must not read as a new
    /// song.
    pub fn reset_audio_history(&self) {
        // Keep the same lock order as live PCM ingestion and ticking: audio
        // before display. Taking both before the generation changes keeps a
        // later reconciliation from mistaking this reset for a stream boundary.
        let mut live_audio = self.lock_live_audio();
        *self.lock_pending_shape_seed() = None;
        let mut state = self.lock();
        let stream_generation = self.advance_stream_generation();
        // The same stream resumes: this is a gap, not a boundary, so the
        // processor keeps its gain and shape and nothing is measured again.
        reset_live_history(&mut live_audio, stream_generation);
        state.stream_generation = stream_generation;
        state.adopted_shape_hold.clear();
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
        let mut ingested_live_frame = false;
        let analyzed_live_frame = if let Some(live_frame) = live_frame {
            state.engine.set_retain_paused_live_shape(true);
            state.engine.set_has_track(true);
            let playing = state.playing;
            state.set_engine_playing(playing, now);
            let hold_adopted_shape = state.adopted_shape_hold.should_hold(
                live_frame.boundary_waiting,
                live_frame.signal_present,
                live_frame.analyzed_samples,
                live_frame.boundary_window_samples,
            );
            if !hold_adopted_shape {
                state.engine.ingest(&live_frame.frame);
                state.has_ingested = true;
                ingested_live_frame = true;
            }
            state.awaiting_stream_after_reset = false;
            state.has_live_audio = true;
            state.last_live_audio_at = Some(now);
            state.live_pressure = live_frame.pressure;
            true
        } else {
            false
        };

        let advanced = state.engine.advance_by(elapsed);
        if ingested_live_frame {
            state.last_live_bands = Some((now, *state.engine.current_bands()));
        }
        advanced || analyzed_live_frame
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
    pub fn scene_tinted(
        &self,
        width: f32,
        height: f32,
        red: f32,
        green: f32,
        blue: f32,
    ) -> Vec<u8> {
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

/// Resets CAVA/bass-detector bookkeeping shared by two different callers: a
/// genuine decoded-stream boundary (`reset_audio_stream`, `note_track_changed`,
/// or any state catching up to one of those through
/// [`reconcile_stream_generation`] — `reset_audio_history` deliberately does
/// not go through here at all, so a later reconciliation cannot mistake its
/// own generation bump for one) and ordinary live-audio staleness
/// (`expire_stale_live_audio`), which bumps nothing and calls this with the
/// same generation. Only the former holds the display
/// (see `awaiting_stream_after_reset`'s doc): staleness during otherwise
/// uninterrupted live playback keeps its existing paused/idle fallback
/// instead, exactly as before this fix — a stall that never speaks again
/// must not freeze the screen forever just because live audio happened to go
/// quiet for one measurement.
fn reset_live_presentation(
    state: &mut VisualState,
    stream_generation: u64,
    now: Duration,
    holds_display: bool,
) {
    state.stream_generation = stream_generation;
    state.adopted_shape_hold.clear();
    state.has_live_audio = false;
    state.last_live_audio_at = None;
    state.live_pressure = silent_pressure();
    state.engine.set_retain_paused_live_shape(false);
    // Only a stream that is playing has a picture worth holding: a boundary
    // that lands while paused or stopped must let the resting projection show,
    // or a later resume would snap the old picture back on screen.
    state.awaiting_stream_after_reset = holds_display && state.playing;
    let has_audio = state.has_analysis
        || state.adopted_shape_hold.is_active()
        || state.awaiting_stream_after_reset;
    state.set_engine_playing(state.playing && has_audio, now);
}

fn reconcile_stream_generation(state: &mut VisualState, stream_generation: u64, now: Duration) {
    if state.stream_generation != stream_generation {
        reset_live_presentation(state, stream_generation, now, true);
    }
}

fn expire_stale_live_audio(state: &mut VisualState, now: Duration) {
    if state.has_live_audio && !live_audio_is_current(state, now) {
        reset_live_presentation(state, state.stream_generation, now, false);
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
                adopted_shape_hold: AdoptedShapeHold::default(),
                has_live_audio: false,
                awaiting_stream_after_reset: false,
                last_live_bands: None,
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
    pub(crate) fn live_boundary_for_testing(&self) -> Option<(bool, f32)> {
        self.live_audio
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|live_audio| {
                (
                    live_audio.processor.is_waiting_for_boundary(),
                    live_audio.processor.sensitivity(),
                )
            })
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

// The yardstick lives in `reprise_core::playback::boundary_fixture`, which
// exists in debug builds only.
#[cfg(all(test, debug_assertions))]
mod boundary_tests {
    include!("visualizer_boundary_tests.rs");
}
