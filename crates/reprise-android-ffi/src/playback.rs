use std::sync::Arc;

use reprise_core::library::settings::TrackTransition;
use reprise_core::playback::{
    AudioEffects, PlaybackBackend, PlaybackError, PlaybackState, PlayerEvent, StreamEvent,
    StreamGeneration,
};

use crate::{AndroidEqualizerPoint, AndroidEqualizerSnapshot};

#[cfg(test)]
#[path = "playback_test_support.rs"]
mod test_support;

#[cfg(test)]
#[path = "playback_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "playback_history_tests.rs"]
mod history_tests;

#[cfg(test)]
#[path = "playback_terminal_event_tests.rs"]
mod terminal_event_tests;

#[cfg(test)]
#[path = "queue_boundary_tests.rs"]
mod queue_boundary_tests;

#[cfg(test)]
#[path = "play_track_ids_tests.rs"]
mod play_track_ids_tests;

#[cfg(test)]
#[path = "trash_boundary_tests.rs"]
mod trash_boundary_tests;

#[cfg(test)]
#[path = "listen_export_playback_tests.rs"]
mod listen_export_playback_tests;

type EventHandler = dyn Fn(StreamEvent) + Send + Sync + 'static;

/// The playback states Media3 must report back to Core.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AndroidPlaybackState {
    Playing,
    /// Media3 still has play intent but is temporarily not producing audio.
    Buffering,
    Paused,
    Stopped,
}

/// The transition modes the Media3 backend can actually provide.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AndroidTransitionMode {
    Off,
    Gapless,
}

/// A Media3 command failure returned across the Kotlin callback boundary.
#[derive(Clone, Debug, thiserror::Error, uniffi::Error)]
#[uniffi(with_try_read)]
pub enum AndroidPlaybackError {
    #[error("Media3 failure: {detail}")]
    Backend { detail: String },
    #[error("invalid playback request: {detail}")]
    InvalidRequest { detail: String },
    #[error("unsupported Android playback capability: {detail}")]
    Unsupported { detail: String },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for AndroidPlaybackError {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Backend {
            detail: error.to_string(),
        }
    }
}

impl From<AndroidPlaybackError> for PlaybackError {
    fn from(error: AndroidPlaybackError) -> Self {
        Self::Backend(error.to_string())
    }
}

/// The synchronous command surface Kotlin implements around Media3.
///
/// Every implementation method must enter Media3's application Looper before
/// touching the player. Keeping that hand-off behind every callback makes the
/// Core trait safe to drive from any runtime thread without weakening Media3's
/// single-Looper contract.
#[uniffi::export(callback_interface)]
pub trait AndroidPlaybackPort: Send + Sync {
    fn set_event_bridge(
        &self,
        bridge: Arc<PlaybackEventBridge>,
    ) -> Result<(), AndroidPlaybackError>;
    fn play_path(&self, path: String) -> Result<(), AndroidPlaybackError>;
    fn play_uri(&self, uri: String) -> Result<(), AndroidPlaybackError>;
    fn toggle_pause(&self) -> Result<AndroidPlaybackState, AndroidPlaybackError>;
    fn seek_to(&self, position_ms: i64) -> Result<(), AndroidPlaybackError>;
    fn set_volume(&self, volume: f64) -> Result<(), AndroidPlaybackError>;
    fn set_equalizer(
        &self,
        enabled: bool,
        curve: Vec<AndroidEqualizerPoint>,
    ) -> Result<(), AndroidPlaybackError>;
    fn equalizer_snapshot(&self) -> Result<Option<AndroidEqualizerSnapshot>, AndroidPlaybackError>;
    fn set_audio_effects(&self) -> Result<(), AndroidPlaybackError>;
    fn set_spectrum_enabled(&self, enabled: bool) -> Result<(), AndroidPlaybackError>;
    fn stop(&self) -> Result<(), AndroidPlaybackError>;
    fn set_next(&self, uri: Option<String>) -> Result<(), AndroidPlaybackError>;
    fn set_transition(&self, mode: AndroidTransitionMode) -> Result<(), AndroidPlaybackError>;
    fn current_generation(&self) -> Result<u64, AndroidPlaybackError>;
}

/// Adapts the foreign Media3 command port to Core's playback contract.
pub struct AndroidPlaybackBackend {
    port: Box<dyn AndroidPlaybackPort>,
}

impl AndroidPlaybackBackend {
    pub fn new(
        port: Box<dyn AndroidPlaybackPort>,
        on_event: Box<EventHandler>,
    ) -> Result<Self, PlaybackError> {
        let bridge = PlaybackEventBridge::new(on_event);
        port.set_event_bridge(bridge).map_err(PlaybackError::from)?;
        Ok(Self { port })
    }

    pub fn set_equalizer(
        &self,
        enabled: bool,
        curve: Vec<AndroidEqualizerPoint>,
    ) -> Result<(), AndroidPlaybackError> {
        self.port.set_equalizer(enabled, curve)
    }

    pub fn equalizer_snapshot(
        &self,
    ) -> Result<Option<AndroidEqualizerSnapshot>, AndroidPlaybackError> {
        self.port.equalizer_snapshot()
    }
}

impl PlaybackBackend for AndroidPlaybackBackend {
    fn play(&self, path: &str) -> Result<(), PlaybackError> {
        self.port
            .play_path(path.to_owned())
            .map_err(PlaybackError::from)
    }

    fn play_uri(&self, uri: &str) -> Result<(), PlaybackError> {
        self.port
            .play_uri(uri.to_owned())
            .map_err(PlaybackError::from)
    }

    fn toggle_pause(&self) -> Result<PlaybackState, PlaybackError> {
        self.port
            .toggle_pause()
            .map(PlaybackState::from)
            .map_err(PlaybackError::from)
    }

    fn seek_to(&self, position_ms: i64) -> Result<(), PlaybackError> {
        self.port.seek_to(position_ms).map_err(PlaybackError::from)
    }

    fn set_volume(&self, volume: f64) {
        let _ = self.port.set_volume(volume);
    }

    fn set_audio_effects(&self, _effects: AudioEffects) -> Result<(), PlaybackError> {
        self.port.set_audio_effects().map_err(PlaybackError::from)
    }

    fn set_spectrum_enabled(&self, enabled: bool) -> Result<(), PlaybackError> {
        self.port
            .set_spectrum_enabled(enabled)
            .map_err(PlaybackError::from)
    }

    fn stop(&self) -> Result<(), PlaybackError> {
        self.port.stop().map_err(PlaybackError::from)
    }

    fn set_next(&self, path: Option<&str>) {
        let _ = self.port.set_next(path.map(str::to_owned));
    }

    fn set_transition(&self, mode: TrackTransition, _crossfade_seconds: u8) {
        let mode = match mode {
            TrackTransition::Off => AndroidTransitionMode::Off,
            TrackTransition::Gapless | TrackTransition::Crossfade => {
                // Media3 has no crossfade. Core explicitly permits this
                // documented degradation to gapless playback.
                AndroidTransitionMode::Gapless
            }
        };
        let _ = self.port.set_transition(mode);
    }

    fn current_generation(&self) -> StreamGeneration {
        self.port
            .current_generation()
            .map_or(StreamGeneration::INITIAL, StreamGeneration::from)
    }
}

/// The subset of Core player events produced by the Android library player.
///
/// Stream metadata and spectrum frames are deliberately absent: neither is a
/// requirement of the local-library Android slice. Media3's automatic
/// transition into a pre-fed successor is distinct from reaching the end of
/// the playlist, so both completion forms remain explicit.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AndroidPlayerEvent {
    StateChanged { state: AndroidPlaybackState },
    Position { position_ms: i64, duration_ms: i64 },
    TrackFinished,
    AdvancedToNext,
    Error { message: String },
}

/// The named Kotlin-to-Rust event boundary.
///
/// Kotlin calls [`Self::emit`] directly from Media3's application thread and
/// supplies the generation that was current when the listener produced the
/// event. The bridge never reads a later generation during delivery, which
/// preserves Core's stale-event protection across a stream switch.
#[derive(uniffi::Object)]
pub struct PlaybackEventBridge {
    handler: Box<EventHandler>,
}

impl PlaybackEventBridge {
    /// Builds the Rust-owned bridge handed to the Kotlin playback service.
    pub fn new(handler: Box<EventHandler>) -> Arc<Self> {
        Arc::new(Self { handler })
    }
}

#[uniffi::export]
impl PlaybackEventBridge {
    /// Delivers one event with its production-time stream generation.
    pub fn emit(&self, generation: u64, event: AndroidPlayerEvent) {
        (self.handler)(StreamEvent {
            generation: StreamGeneration::from(generation),
            event: event.into(),
        });
    }
}

impl From<AndroidPlaybackState> for PlaybackState {
    fn from(state: AndroidPlaybackState) -> Self {
        // This projection is used for synchronous playback-command results.
        // `Media3PlaybackPort.togglePause()` returns only Playing or Paused;
        // asynchronous Buffering crosses through `AndroidPlayerEvent` below.
        match state {
            AndroidPlaybackState::Playing | AndroidPlaybackState::Buffering => Self::Playing,
            AndroidPlaybackState::Paused => Self::Paused,
            AndroidPlaybackState::Stopped => Self::Stopped,
        }
    }
}

impl From<PlaybackState> for AndroidPlaybackState {
    fn from(state: PlaybackState) -> Self {
        match state {
            PlaybackState::Playing => Self::Playing,
            PlaybackState::Paused => Self::Paused,
            PlaybackState::Stopped => Self::Stopped,
        }
    }
}

impl From<AndroidPlayerEvent> for PlayerEvent {
    fn from(event: AndroidPlayerEvent) -> Self {
        match event {
            AndroidPlayerEvent::StateChanged {
                state: AndroidPlaybackState::Buffering,
            } => Self::Buffering {
                percent: 0,
                buffered_ms: None,
            },
            AndroidPlayerEvent::StateChanged { state } => Self::StateChanged(state.into()),
            AndroidPlayerEvent::Position {
                position_ms,
                duration_ms,
            } => Self::Position {
                position_ms,
                duration_ms,
            },
            AndroidPlayerEvent::TrackFinished => Self::TrackFinished,
            AndroidPlayerEvent::AdvancedToNext => Self::AdvancedToNext,
            AndroidPlayerEvent::Error { message } => Self::Error(message.into()),
        }
    }
}
