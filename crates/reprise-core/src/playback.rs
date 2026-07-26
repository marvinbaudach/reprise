//! Platform-neutral audio-playback contract (core-destined). Defines the
//! coarse `PlaybackState`, the asynchronous `PlayerEvent` stream, the
//! `PlaybackError` every backend reports, and the `PlaybackBackend` trait the
//! frontend drives playback through. The concrete implementations live in the
//! per-OS platform crates (Linux: GStreamer `playbin3` in `player.rs`).

mod cava;
mod fault_policy;

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
        }
    }

    /// Per-band normalized magnitudes (`0..=1`).
    pub fn bands(&self) -> &[f32; SPECTRUM_BAND_COUNT] {
        &self.bands
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

#[cfg(test)]
#[path = "playback/song_visual_tests.rs"]
mod song_visual_tests;

#[cfg(test)]
#[path = "playback/cava_tests.rs"]
mod cava_tests;

/// The audio-playback contract every platform implements (Linux: GStreamer
/// playbin3 in `player.rs`; future macOS/Windows: AVFoundation / WASAPI —
/// see "Repository & frontend strategy"). Surface = exactly what the
/// GNOME frontend consumes today, nothing speculative. Event delivery is a
/// construction-time concern, not a trait method: each concrete backend
/// takes a `Box<dyn Fn(PlayerEvent) + Send + Sync>` callback in its own
/// constructor and may invoke it from any thread; frontends marshal.
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
