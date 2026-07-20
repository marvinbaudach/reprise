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

pub const SPECTRUM_BAND_COUNT: usize = 16;
const SPECTRUM_FLOOR_DB: f32 = -80.0;

/// One bounded, path-free snapshot of the currently playing audio spectrum.
/// Values are normalized to `0..=1` so frontends never depend on a platform
/// analyzer's decibel floor or message representation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpectrumFrame {
    bands: [f32; SPECTRUM_BAND_COUNT],
}

impl SpectrumFrame {
    pub fn from_decibels(decibels: [f32; SPECTRUM_BAND_COUNT]) -> Self {
        Self {
            bands: decibels.map(|value| {
                if !value.is_finite() {
                    return 0.0;
                }
                ((value - SPECTRUM_FLOOR_DB) / -SPECTRUM_FLOOR_DB).clamp(0.0, 1.0)
            }),
        }
    }

    pub fn bands(&self) -> &[f32; SPECTRUM_BAND_COUNT] {
        &self.bands
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
    /// A local-only, normalized audio spectrum for optional visual rendering.
    Spectrum(SpectrumFrame),
    Error(String),
}

#[cfg(test)]
mod song_visual_tests {
    use super::*;

    #[test]
    fn ac_10_spectrum_frame_normalizes_decibels_and_rejects_non_finite_input() {
        let frame = SpectrumFrame::from_decibels([
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

        assert_eq!(frame.bands()[0], 0.0);
        assert_eq!(frame.bands()[5], 0.5);
        assert_eq!(frame.bands()[10], 1.0);
        assert_eq!(frame.bands()[11], 0.0);
        assert_eq!(frame.bands()[12], 1.0);
        assert_eq!(&frame.bands()[13..], &[0.0, 0.0, 0.0]);
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
