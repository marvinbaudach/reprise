//! Platform-neutral audio-playback contract (core-destined). Defines the
//! coarse `PlaybackState`, the asynchronous `PlayerEvent` stream, the
//! `PlaybackError` every backend reports, and the `PlaybackBackend` trait the
//! frontend drives playback through. The concrete implementations live in the
//! per-OS platform crates (Linux: GStreamer `playbin3` in `player.rs`).

/// Coarse playback state, mirrored from the underlying GStreamer pipeline state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped,
}

/// The one user-facing notice a playback fault is allowed to produce.
/// Frontends translate these semantic variants at their presentation edge;
/// keeping the cardinality in [`PlaybackFaultPolicy`] makes FB-6's "one
/// toast" rule a core policy rather than an accident of one GTK branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackFaultNotice {
    /// The file vanished while it was playing: mark it missing, skip, and
    /// explain that availability—not decoding—caused the skip.
    TrackUnavailableSkipped,
    /// The file still exists but the backend could not play it.
    CouldNotPlaySkipped,
}

/// Complete effect policy for one fault of the currently playing track.
/// Background watcher events never construct this value and therefore stay
/// silent; only the player fault path consumes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaybackFaultPolicy {
    pub mark_missing: bool,
    pub skip: bool,
    /// Exactly one notice by construction. An array is deliberate: a future
    /// edit cannot silently add a second toast without changing this API and
    /// its FB-6 acceptance test.
    pub notices: [PlaybackFaultNotice; 1],
}

/// Resolves a player backend fault from the strongest evidence available at
/// that moment: whether the track's path still names a file.
pub fn playback_fault_policy(file_exists: bool) -> PlaybackFaultPolicy {
    if file_exists {
        PlaybackFaultPolicy {
            mark_missing: false,
            skip: true,
            notices: [PlaybackFaultNotice::CouldNotPlaySkipped],
        }
    } else {
        PlaybackFaultPolicy {
            mark_missing: true,
            skip: true,
            notices: [PlaybackFaultNotice::TrackUnavailableSkipped],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioEffects {
    pub equalizer_enabled: bool,
    pub equalizer_bands: [f64; 10],
    pub replay_gain: crate::library::settings::ReplayGainMode,
}

impl Default for AudioEffects {
    fn default() -> Self {
        Self {
            equalizer_enabled: false,
            equalizer_bands: [0.0; 10],
            replay_gain: crate::library::settings::ReplayGainMode::Off,
        }
    }
}

pub const SPECTRUM_BAND_COUNT: usize = 32;
/// Target interval between spectrum messages. ~23 ms (≈43 Hz) trades a little
/// CPU for crisp, legible beat response — 50 ms (20 Hz) smears fast kicks. The
/// platform analyzer builds its element from this; the visual envelope time
/// constants derive from it so the feel is independent of the exact rate.
pub const SPECTRUM_INTERVAL_MS: u64 = 23;
const SPECTRUM_FLOOR_DB: f32 = -80.0;

/// A detected onset for the current frame. `fired` is the event edge; `strength`
/// (`0..=1`) is how far the spectral flux overshot the adaptive threshold, so
/// the frontend can scale impact visuals to how hard the hit landed.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Beat {
    pub fired: bool,
    pub strength: f32,
}

/// One bounded, path-free snapshot of the currently playing audio, enriched with
/// derived, ready-to-render reactivity signals. Values are normalized so
/// frontends never depend on a platform analyzer's decibel floor or message
/// representation, and never have to re-derive envelopes or onsets themselves.
///
/// `from_decibels` yields a bands-only frame (all scalars neutral); the live,
/// reactive scalars come from [`SpectrumAnalyzer::ingest`], which owns the
/// cross-frame state the derivations need.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpectrumFrame {
    bands: [f32; SPECTRUM_BAND_COUNT],
    level: f32,
    bass: f32,
    beat: Beat,
    dynamics: f32,
}

fn normalize_bands(decibels: [f32; SPECTRUM_BAND_COUNT]) -> [f32; SPECTRUM_BAND_COUNT] {
    decibels.map(|value| {
        if !value.is_finite() {
            return 0.0;
        }
        ((value - SPECTRUM_FLOOR_DB) / -SPECTRUM_FLOOR_DB).clamp(0.0, 1.0)
    })
}

impl SpectrumFrame {
    /// Bands-only frame with neutral scalars. Used where no cross-frame state is
    /// available (tests, and any stateless consumer).
    pub fn from_decibels(decibels: [f32; SPECTRUM_BAND_COUNT]) -> Self {
        Self {
            bands: normalize_bands(decibels),
            level: 0.0,
            bass: 0.0,
            beat: Beat::default(),
            dynamics: 0.0,
        }
    }

    /// Per-band normalized magnitudes (`0..=1`).
    pub fn bands(&self) -> &[f32; SPECTRUM_BAND_COUNT] {
        &self.bands
    }

    /// Overall loudness envelope with fast attack / slow release (`0..=1`).
    pub fn level(&self) -> f32 {
        self.level
    }

    /// Low-band (kick/sub) envelope with fast attack / slow release (`0..=1`).
    pub fn bass(&self) -> f32 {
        self.bass
    }

    /// Onset detected this frame.
    pub fn beat(&self) -> Beat {
        self.beat
    }

    /// Short-term loudness relative to a slow baseline (`-1..=1`): strongly
    /// positive on a drop/slam after a lull, negative on a sudden quiet.
    pub fn dynamics(&self) -> f32 {
        self.dynamics
    }
}

// --- Reactivity derivation (see `Beat` / `SpectrumFrame`) --------------------

/// Number of low bands folded into the `bass` envelope (kick/sub range).
const BASS_BAND_COUNT: usize = 4;
/// Low bands weighted up in the spectral-flux sum so kicks dominate onset
/// detection over hi-hat/cymbal shimmer.
const FLUX_LOW_BANDS: usize = 8;
const FLUX_LOW_WEIGHT: f32 = 1.6;
const FLUX_HIGH_WEIGHT: f32 = 1.0;
/// Envelope release time constants (ms). Attack is near-instant.
const LEVEL_RELEASE_MS: f32 = 180.0;
const BASS_RELEASE_MS: f32 = 140.0;
/// Smoothing window for the adaptive flux mean/variance (ms).
const FLUX_STATS_MS: f32 = 350.0;
/// Short/long loudness baselines feeding `dynamics` (ms).
const DYN_SHORT_MS: f32 = 120.0;
const DYN_LONG_MS: f32 = 2200.0;
const DYN_SCALE: f32 = 1.5;
/// Beat fires when flux exceeds `mean + K*std + floor`, is above a minimum, and
/// the refractory window has elapsed.
const BEAT_K: f32 = 1.8;
const BEAT_FLOOR: f32 = 0.02;
const BEAT_MIN_FLUX: f32 = 0.05;
const BEAT_REFRACTORY_MS: f32 = 90.0;
/// Flux overshoot above threshold that maps to full `strength`.
const BEAT_STRENGTH_OVERSHOOT: f32 = 0.35;

fn ema_coeff(interval_ms: f32, tau_ms: f32) -> f32 {
    (1.0 - (-interval_ms / tau_ms).exp()).clamp(0.0, 1.0)
}

/// Stateful, pure-Rust reactivity extractor. Feed it successive raw-decibel
/// spectrum frames (spaced ~[`SPECTRUM_INTERVAL_MS`] apart) and it returns a
/// [`SpectrumFrame`] carrying the normalized bands plus derived `level`, `bass`,
/// `beat`, and `dynamics`. All time constants derive from the frame interval, so
/// the feel is independent of the exact message rate. Portable across frontends;
/// the GTK visualizer only renders what this produces.
pub struct SpectrumAnalyzer {
    prev_bands: [f32; SPECTRUM_BAND_COUNT],
    level_env: f32,
    bass_env: f32,
    flux_mean: f32,
    flux_sq_mean: f32,
    short_loud: f32,
    long_loud: f32,
    frames_since_beat: u32,
    level_release: f32,
    bass_release: f32,
    flux_coeff: f32,
    short_coeff: f32,
    long_coeff: f32,
    refractory_frames: u32,
}

impl Default for SpectrumAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl SpectrumAnalyzer {
    pub fn new() -> Self {
        let dt = SPECTRUM_INTERVAL_MS as f32;
        let refractory_frames = (BEAT_REFRACTORY_MS / dt).ceil() as u32;
        Self {
            prev_bands: [0.0; SPECTRUM_BAND_COUNT],
            level_env: 0.0,
            bass_env: 0.0,
            flux_mean: 0.0,
            flux_sq_mean: 0.0,
            short_loud: 0.0,
            long_loud: 0.0,
            // Allow the first real frame to fire.
            frames_since_beat: refractory_frames,
            level_release: ema_coeff(dt, LEVEL_RELEASE_MS),
            bass_release: ema_coeff(dt, BASS_RELEASE_MS),
            flux_coeff: ema_coeff(dt, FLUX_STATS_MS),
            short_coeff: ema_coeff(dt, DYN_SHORT_MS),
            long_coeff: ema_coeff(dt, DYN_LONG_MS),
            refractory_frames,
        }
    }

    /// Consume one raw-decibel frame, advance the internal state, and emit the
    /// enriched, bounded [`SpectrumFrame`] for this instant.
    pub fn ingest(&mut self, decibels: [f32; SPECTRUM_BAND_COUNT]) -> SpectrumFrame {
        let bands = normalize_bands(decibels);
        let overall = mean(&bands, 0..SPECTRUM_BAND_COUNT);
        let bass_input = mean(&bands, 0..BASS_BAND_COUNT);

        let level = envelope(self.level_env, overall, self.level_release);
        self.level_env = level;
        let bass = envelope(self.bass_env, bass_input, self.bass_release);
        self.bass_env = bass;

        let beat = self.detect_beat(&bands);
        self.prev_bands = bands;

        let dynamics = self.detect_dynamics(overall);

        SpectrumFrame {
            bands,
            level,
            bass,
            beat,
            dynamics,
        }
    }

    fn detect_beat(&mut self, bands: &[f32; SPECTRUM_BAND_COUNT]) -> Beat {
        let mut flux = 0.0_f32;
        let mut total_weight = 0.0_f32;
        for index in 0..SPECTRUM_BAND_COUNT {
            let weight = if index < FLUX_LOW_BANDS {
                FLUX_LOW_WEIGHT
            } else {
                FLUX_HIGH_WEIGHT
            };
            flux += (bands[index] - self.prev_bands[index]).max(0.0) * weight;
            total_weight += weight;
        }
        flux /= total_weight.max(1.0);

        let variance = (self.flux_sq_mean - self.flux_mean * self.flux_mean).max(0.0);
        let threshold = self.flux_mean + BEAT_K * variance.sqrt() + BEAT_FLOOR;
        let fired = flux > threshold
            && flux > BEAT_MIN_FLUX
            && self.frames_since_beat >= self.refractory_frames;
        let strength = if fired {
            ((flux - threshold) / BEAT_STRENGTH_OVERSHOOT).clamp(0.0, 1.0)
        } else {
            0.0
        };

        self.flux_mean += (flux - self.flux_mean) * self.flux_coeff;
        self.flux_sq_mean += (flux * flux - self.flux_sq_mean) * self.flux_coeff;
        self.frames_since_beat = if fired {
            0
        } else {
            self.frames_since_beat.saturating_add(1)
        };

        Beat { fired, strength }
    }

    fn detect_dynamics(&mut self, overall: f32) -> f32 {
        self.short_loud += (overall - self.short_loud) * self.short_coeff;
        self.long_loud += (overall - self.long_loud) * self.long_coeff;
        ((self.short_loud - self.long_loud) * DYN_SCALE).clamp(-1.0, 1.0)
    }
}

fn envelope(current: f32, input: f32, release: f32) -> f32 {
    if input > current {
        input
    } else {
        (current + (input - current) * release).clamp(0.0, 1.0)
    }
}

fn mean(bands: &[f32; SPECTRUM_BAND_COUNT], range: std::ops::Range<usize>) -> f32 {
    let count = range.len().max(1) as f32;
    range.map(|index| bands[index]).sum::<f32>() / count
}

/// Events the player reports asynchronously, from the GStreamer bus watch and
/// the position ticker. The UI layer subscribes to these via the callback
/// passed to `Player::new`.
#[derive(Debug, Clone)]
pub enum PlayerEvent {
    StateChanged(PlaybackState),
    Position {
        position_ms: i64,
        duration_ms: i64,
    },
    TrackFinished,
    /// The gaplessly pre-fed next track (see `PlaybackBackend::set_next`) has
    /// taken over without a pipeline restart: `about-to-finish` consumed the
    /// queued URI and playback continued seamlessly into it. The frontend
    /// advances its own queue model by exactly one step in response WITHOUT
    /// issuing a new `play()` — the audio is already rolling. Contrast
    /// `TrackFinished`, which fires on a real end-of-stream (no next track was
    /// pre-fed) and is the frontend's cue to *start* the next track.
    AdvancedToNext,
    /// A local-only, normalized audio spectrum for optional visual rendering.
    Spectrum(SpectrumFrame),
    Error(String),
}

#[cfg(test)]
mod song_visual_tests {
    use super::*;

    #[test]
    fn ac_10_spectrum_frame_normalizes_decibels_and_rejects_non_finite_input() {
        let mut decibels = [-80.0_f32; SPECTRUM_BAND_COUNT];
        decibels[..16].copy_from_slice(&[
            -80.0,
            -72.0,
            -64.0,
            -56.0,
            -48.0,
            -40.0,
            -32.0,
            -24.0,
            -16.0,
            -8.0,
            0.0,
            -120.0,
            12.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ]);
        let frame = SpectrumFrame::from_decibels(decibels);

        assert_eq!(frame.bands()[0], 0.0);
        assert_eq!(frame.bands()[5], 0.5);
        assert_eq!(frame.bands()[10], 1.0);
        assert_eq!(frame.bands()[11], 0.0);
        assert_eq!(frame.bands()[12], 1.0);
        assert_eq!(&frame.bands()[13..16], &[0.0, 0.0, 0.0]);
        // Bands beyond the explicit prefix sit at the floor.
        assert!(frame.bands()[16..].iter().all(|&value| value == 0.0));
    }
}

#[cfg(test)]
mod spectrum_analyzer_tests {
    use super::*;

    const SILENCE: [f32; SPECTRUM_BAND_COUNT] = [SPECTRUM_FLOOR_DB; SPECTRUM_BAND_COUNT];
    const FULL: [f32; SPECTRUM_BAND_COUNT] = [0.0; SPECTRUM_BAND_COUNT];

    fn ingest_n(
        analyzer: &mut SpectrumAnalyzer,
        db: [f32; SPECTRUM_BAND_COUNT],
        n: usize,
    ) -> SpectrumFrame {
        let mut frame = analyzer.ingest(db);
        for _ in 1..n {
            frame = analyzer.ingest(db);
        }
        frame
    }

    #[test]
    fn constant_input_settles_without_beats_and_tracks_level() {
        let mut analyzer = SpectrumAnalyzer::new();
        let moderate = [-20.0_f32; SPECTRUM_BAND_COUNT]; // normalized 0.75
        let frame = ingest_n(&mut analyzer, moderate, 60);
        assert!(!frame.beat().fired, "steady tone must not keep firing beats");
        assert!(
            (frame.level() - 0.75).abs() < 0.05,
            "level should track the steady input, got {}",
            frame.level()
        );
    }

    #[test]
    fn impulse_after_silence_fires_beat_with_instant_attack() {
        let mut analyzer = SpectrumAnalyzer::new();
        ingest_n(&mut analyzer, SILENCE, 20);
        let hit = analyzer.ingest(FULL);
        assert!(hit.beat().fired, "a slam after silence must register a beat");
        assert!(hit.beat().strength > 0.0);
        assert!(
            hit.level() > 0.9,
            "attack is near-instant, level should jump, got {}",
            hit.level()
        );
    }

    #[test]
    fn level_releases_gradually_after_impulse() {
        let mut analyzer = SpectrumAnalyzer::new();
        ingest_n(&mut analyzer, SILENCE, 20);
        let hit = analyzer.ingest(FULL);
        let after = analyzer.ingest(SILENCE);
        assert!(after.level() < hit.level(), "level must decay after the hit");
        assert!(
            after.level() > 0.1,
            "release is gradual, not an instant drop to zero, got {}",
            after.level()
        );
    }

    #[test]
    fn silence_then_sustained_loud_spikes_dynamics() {
        let mut analyzer = SpectrumAnalyzer::new();
        ingest_n(&mut analyzer, SILENCE, 40);
        let frame = ingest_n(&mut analyzer, FULL, 4);
        assert!(
            frame.dynamics() > 0.3,
            "a loud drop after a lull should read as a positive dynamics spike, got {}",
            frame.dynamics()
        );
    }

    #[test]
    fn slow_ramp_does_not_produce_a_beat_storm() {
        let mut analyzer = SpectrumAnalyzer::new();
        let mut beats = 0;
        for step in 0..64 {
            let db = [-80.0_f32 + step as f32; SPECTRUM_BAND_COUNT];
            if analyzer.ingest(db).beat().fired {
                beats += 1;
            }
        }
        assert!(
            beats <= 3,
            "a smooth build-up is not a beat every frame, fired {beats} times"
        );
    }

    #[test]
    fn all_outputs_stay_finite_and_bounded() {
        let mut analyzer = SpectrumAnalyzer::new();
        for step in 0..200 {
            let db = [-80.0_f32 + (step % 80) as f32; SPECTRUM_BAND_COUNT];
            let frame = analyzer.ingest(db);
            assert!(frame.level().is_finite() && (0.0..=1.0).contains(&frame.level()));
            assert!(frame.bass().is_finite() && (0.0..=1.0).contains(&frame.bass()));
            assert!(frame.beat().strength.is_finite() && (0.0..=1.0).contains(&frame.beat().strength));
            assert!(frame.dynamics().is_finite() && (-1.0..=1.0).contains(&frame.dynamics()));
        }
    }
}

/// The audio-playback contract every platform implements (Linux: GStreamer
/// playbin3 in `player.rs`; future macOS/Windows: AVFoundation / WASAPI —
/// see "Repository & frontend strategy"). Surface = exactly what the
/// GNOME frontend consumes today, nothing speculative. Event delivery is a
/// construction-time concern, not a trait method: each concrete backend
/// takes a `Box<dyn Fn(PlayerEvent) + Send + Sync>` callback in its own
/// constructor and may invoke it from any thread; frontends marshal.
pub trait PlaybackBackend {
    fn play(&self, path: &str) -> Result<(), PlaybackError>;
    fn toggle_pause(&self) -> Result<PlaybackState, PlaybackError>;
    fn seek_to(&self, position_ms: i64) -> Result<(), PlaybackError>;
    fn set_volume(&self, volume: f64);
    fn set_audio_effects(&self, effects: AudioEffects) -> Result<(), PlaybackError>;
    /// Enables or disables the optional spectrum analyzer at runtime. Backends
    /// without an analyzer may keep the default no-op implementation.
    fn set_spectrum_enabled(&self, _enabled: bool) -> Result<(), PlaybackError> {
        Ok(())
    }
    fn stop(&self) -> Result<(), PlaybackError>;

    /// Pre-feeds the path of the track that should play next, so the backend
    /// can hand off to it *gaplessly* (no pipeline restart) when the current
    /// track is about to finish. `None` clears any queued track — call it when
    /// gapless is disabled, at the end of the queue, or whenever the upcoming
    /// track changes (queue edit, repeat/shuffle toggle, manual navigation).
    /// The frontend re-feeds on every such change; the backend keeps only the
    /// latest value ("last write wins"). A backend that does not support
    /// gapless handoff may treat this as a no-op — playback then falls back to
    /// the ordinary `TrackFinished`-driven advance.
    fn set_next(&self, path: Option<&str>);

    /// Selects how the backend transitions into the pre-fed next track (see
    /// `set_next`): `Off`/`Gapless` hand off at the end (the frontend only
    /// pre-feeds a next URI when this is `Gapless`), `Crossfade` overlaps the
    /// current track's tail with the next track's head over `crossfade_seconds`
    /// (only meaningful for `Crossfade`). The frontend calls this at startup
    /// and whenever the setting changes, so it takes effect without a restart.
    /// A backend that does not support crossfade may treat that mode as
    /// `Gapless` (documented degradation, never a failure).
    fn set_transition(
        &self,
        mode: crate::library::settings::TrackTransition,
        crossfade_seconds: u8,
    );
}

/// Platform-neutral playback error. `Backend`'s message is produced by the
/// platform impl and shown to users as-is (toasts/logs) — the Linux impl
/// formats "GStreamer: {source}" into it so user-visible strings stay
/// byte-identical to the pre-seam `PlayerError::Gst` Display output.
#[derive(Debug, thiserror::Error)]
pub enum PlaybackError {
    #[error("{0}")]
    Backend(String),
    #[error("invalid path: {0}")]
    BadPath(String),
}
