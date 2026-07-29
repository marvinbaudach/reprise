//! Platform-neutral audio-playback contract (core-destined). Defines the
//! coarse `PlaybackState`, the asynchronous `PlayerEvent` stream, the
//! `PlaybackError` every backend reports, and the `PlaybackBackend` trait the
//! frontend drives playback through. The concrete implementations live in the
//! per-OS platform crates (Linux: GStreamer `playbin3` in `player.rs`).

mod bass_pressure;
mod cava;
mod fault_policy;

pub use bass_pressure::{BassPressure, BassPressureDetector, STEADY_GLOW};
pub use cava::{CavaBarProcessor, CavaConfig, CavaError};
pub use fault_policy::{playback_fault_policy, PlaybackFaultNotice, PlaybackFaultPolicy};

/// Coarse playback state, mirrored from the underlying GStreamer pipeline state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped,
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

/// One-to-one CAVA bars carried by [`SpectrumFrame`].
pub const SPECTRUM_BAND_COUNT: usize = 64;

/// One bounded, path-free snapshot of CAVA's already-smoothed output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpectrumFrame {
    bands: [f32; SPECTRUM_BAND_COUNT],
    pressure: BassPressure,
}

impl SpectrumFrame {
    /// Wraps one already-smoothed CAVA frame without applying another gain,
    /// logarithmic fold, or temporal filter.
    pub fn from_cava_bars(bars: [f32; SPECTRUM_BAND_COUNT]) -> Self {
        Self {
            bands: bars.map(|bar| {
                if bar.is_finite() {
                    bar.clamp(0.0, 1.0)
                } else {
                    0.0
                }
            }),
            pressure: BassPressure::silent(),
        }
    }

    /// Attaches the absolute bass measurement taken from the same PCM. Kept
    /// separate from the bars because CAVA's auto-sensitivity makes those
    /// relative, and the glow layer needs an honest level (AC-23).
    #[must_use]
    pub fn with_bass_pressure(self, pressure: BassPressure) -> Self {
        Self {
            pressure: pressure.sanitized(),
            ..self
        }
    }

    /// Per-band normalized magnitudes (`0..=1`).
    pub fn bands(&self) -> &[f32; SPECTRUM_BAND_COUNT] {
        &self.bands
    }

    /// How hard the bass is pushing, measured without CAVA's gain in the path.
    pub fn bass_pressure(&self) -> BassPressure {
        self.pressure
    }

    /// Under render load CAVA frames are strictly latest-wins.
    pub fn coalesce_latest(self, latest: Self) -> Self {
        latest
    }
}

/// Events the player reports asynchronously, from the GStreamer bus watch and
/// the position ticker. The UI layer subscribes to these via the callback
/// passed to `Player::new`.
// `Spectrum` carries a fixed 64-band snapshot (~276 B) emitted ~60×/s; boxing
// it would add a per-frame heap allocation on the audio hot path.
#[allow(clippy::large_enum_variant)]
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
    /// Metadata carried by a remote stream. Radio uses the title as its live
    /// now-playing value and the organization as the station label.
    StreamTags {
        title: Option<String>,
        organization: Option<String>,
    },
    /// A local-only, normalized audio spectrum for optional visual rendering.
    Spectrum(SpectrumFrame),
    Error(String),
}

/// Identifies which stream — i.e. which call to [`PlaybackBackend::play`] /
/// [`PlaybackBackend::play_uri`], or which gapless/crossfade hand-off to a
/// pre-fed track — produced a given [`PlayerEvent`]. See the "Stream
/// generations" section of [`PlaybackBackend`]'s doc comment for the full
/// contract; in short, a backend bumps this counter every time it starts
/// something new, and a consumer that remembers the generation it itself
/// last started can use it to tell a late-arriving event for an abandoned
/// stream apart from one for the stream currently in play.
///
/// Ordering is the whole API surface on purpose: a consumer's discard rule is
/// "strictly older than the highest generation I have seen so far ⇒ stale,
/// discard". A generation the consumer has never seen that is *newer* than
/// its own bookmark is never stale — it can only mean a stream already
/// started by some means the consumer has not caught up with yet (its own
/// `play()` call included, since the backend bumps before that call returns),
/// so the right move is to adopt it and advance the bookmark, not discard it.
/// Equivalently: compare with `<`, never `==`/`!=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StreamGeneration(u64);

impl StreamGeneration {
    /// The generation in effect before any stream has ever started. Also
    /// what [`PlaybackBackend::current_generation`]'s default implementation
    /// returns forever, for backends that do not override it — see that
    /// method's doc comment for why that is a safe, honest default rather
    /// than a lie.
    pub const INITIAL: Self = Self(0);
}

impl From<u64> for StreamGeneration {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// A [`PlayerEvent`] paired with the [`StreamGeneration`] that was current
/// the instant the backend produced it.
///
/// Purely additive, opt-in delivery. A backend's plain event-delivery
/// callback (`Box<dyn Fn(PlayerEvent) + Send + Sync>`, unchanged — see
/// [`PlaybackBackend`]'s doc comment) keeps delivering bare `PlayerEvent`s
/// exactly as it always has, so a consumer that does not care about stream
/// generations is never forced to see this type, and existing consumers are
/// unaffected byte-for-byte. A backend that also offers a *tagged*
/// construction path (Linux: `Player::new_with_generation`) delivers this
/// type instead, to consumers that need to discard stale async events
/// crossing a stream boundary — see [`StreamGeneration`]'s doc comment for
/// the discard rule.
#[derive(Debug, Clone)]
pub struct StreamEvent {
    pub generation: StreamGeneration,
    pub event: PlayerEvent,
}

#[cfg(test)]
#[path = "playback/song_visual_tests.rs"]
mod song_visual_tests;

#[cfg(test)]
#[path = "playback/cava_tests.rs"]
mod cava_tests;

#[cfg(test)]
#[path = "playback/bass_pressure_tests.rs"]
mod bass_pressure_tests;

/// The audio-playback contract every platform implements (Linux: GStreamer
/// playbin3 in `player.rs`; future macOS/Windows: AVFoundation / WASAPI —
/// see "Repository & frontend strategy"). Surface = exactly what the
/// GNOME frontend consumes today, nothing speculative. Event delivery is a
/// construction-time concern, not a trait method: each concrete backend
/// takes a `Box<dyn Fn(PlayerEvent) + Send + Sync>` callback in its own
/// constructor and may invoke it from any thread; frontends marshal.
///
/// ## Stream generations
///
/// Event delivery is asynchronous relative to the calls that start playback:
/// a `TrackFinished`/`Error`/`Position` for a track the caller has already
/// abandoned (the user pressed Next) can still be in flight when the next
/// track starts, and arrive after it — applied naively, that re-advances a
/// queue that already advanced, or overwrites the new track's position with
/// the old one's. A backend implementing this trait MUST maintain a
/// [`StreamGeneration`] counter and bump it every time it starts something
/// new: `play`, `play_uri`, and a gapless/crossfade hand-off to a pre-fed
/// track all count as "new" — the hand-off hands the listener a genuinely
/// different stream even though no `play`/`play_uri` call drove it.
/// [`current_generation`](Self::current_generation) exposes the live value
/// for any consumer to read. A backend that also offers a *tagged*
/// construction path (Linux: `Player::new_with_generation`) stamps every
/// emitted event, at the instant it is produced, with whatever generation
/// was current then (see [`StreamEvent`]) — that production-time stamp, not
/// a value the consumer reads later on delivery, is what makes discarding a
/// stale event safe under the race above; delivery order and read timing are
/// not trustworthy substitutes for it. This is purely additive: the plain
/// `Fn(PlayerEvent)` construction path is unaffected, and a consumer that
/// never asks for tagging never observes either new type.
pub trait PlaybackBackend {
    fn play(&self, path: &str) -> Result<(), PlaybackError>;
    /// Starts a non-local media URI. Implementations must accept `http`,
    /// `https`, and `file`; local-path callers continue to use [`Self::play`].
    fn play_uri(&self, uri: &str) -> Result<(), PlaybackError>;
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

    /// The generation of whichever stream this backend most recently started
    /// (see the "Stream generations" section above). Backends that do not
    /// track streams individually may keep this default, which never
    /// changes: every event then compares as "not older", so staleness
    /// detection is simply unavailable for that backend rather than
    /// incorrect — a safe, honest default for mocks/fixtures, never a lie.
    fn current_generation(&self) -> StreamGeneration {
        StreamGeneration::INITIAL
    }
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
