use std::sync::Arc;

use reprise_core::playback::{PlaybackState, PlayerEvent, StreamEvent, StreamGeneration};

type EventHandler = dyn Fn(StreamEvent) + Send + Sync + 'static;

/// The playback states Media3 must report back to Core.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AndroidPlaybackState {
    Playing,
    Paused,
    Stopped,
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
        match state {
            AndroidPlaybackState::Playing => Self::Playing,
            AndroidPlaybackState::Paused => Self::Paused,
            AndroidPlaybackState::Stopped => Self::Stopped,
        }
    }
}

impl From<AndroidPlayerEvent> for PlayerEvent {
    fn from(event: AndroidPlayerEvent) -> Self {
        match event {
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
            AndroidPlayerEvent::Error { message } => Self::Error(message),
        }
    }
}
