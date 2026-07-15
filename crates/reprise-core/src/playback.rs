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
    Error(String),
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
