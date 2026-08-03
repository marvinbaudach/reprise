//! Android's Core-owned playback queue and transport surface.

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use reprise_core::library::settings::TrackTransition;
use reprise_core::playback::{PlaybackBackend, PlayerEvent, StreamEvent, StreamGeneration};
use reprise_core::queue::Queue;

use crate::playback::{
    AndroidPlaybackBackend, AndroidPlaybackError, AndroidPlaybackPort, AndroidPlaybackState,
};

/// The playback state rendered by the Android surface.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AndroidPlaybackSnapshot {
    pub state: AndroidPlaybackState,
    pub current_index: Option<u64>,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub error: Option<String>,
}

#[uniffi::export(callback_interface)]
pub trait AndroidPlaybackListener: Send + Sync {
    fn on_playback_changed(&self, snapshot: AndroidPlaybackSnapshot);
}

struct SessionState {
    queue: Queue,
    uris: Vec<String>,
    snapshot: AndroidPlaybackSnapshot,
    stream: StreamGeneration,
}

impl SessionState {
    fn new() -> Self {
        Self {
            queue: Queue::new(),
            uris: Vec::new(),
            snapshot: AndroidPlaybackSnapshot {
                state: AndroidPlaybackState::Stopped,
                current_index: None,
                position_ms: 0,
                duration_ms: 0,
                error: None,
            },
            stream: StreamGeneration::INITIAL,
        }
    }

    fn current_uri(&self) -> Option<String> {
        self.queue
            .current()
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| self.uris.get(index).cloned())
    }

    fn next_uri(&self) -> Option<String> {
        self.queue
            .peek_auto()
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| self.uris.get(index).cloned())
    }

    fn adopt_current(&mut self) {
        self.snapshot.current_index = self
            .queue
            .current()
            .and_then(|index| u64::try_from(index).ok());
        self.snapshot.position_ms = 0;
        self.snapshot.duration_ms = 0;
        self.snapshot.error = None;
        self.snapshot.state = AndroidPlaybackState::Playing;
    }

    fn stop(&mut self) {
        self.snapshot.state = AndroidPlaybackState::Stopped;
        self.snapshot.current_index = None;
        self.snapshot.position_ms = 0;
        self.snapshot.duration_ms = 0;
    }

    fn accepts(&mut self, generation: StreamGeneration) -> bool {
        if generation < self.stream {
            return false;
        }
        self.stream = generation;
        true
    }
}

struct SessionInner {
    state: Mutex<SessionState>,
    backend: OnceLock<AndroidPlaybackBackend>,
    listener: Box<dyn AndroidPlaybackListener>,
}

impl SessionInner {
    fn lock(&self) -> Result<MutexGuard<'_, SessionState>, AndroidPlaybackError> {
        self.state
            .lock()
            .map_err(|_| AndroidPlaybackError::Backend {
                detail: "playback session state was poisoned".to_owned(),
            })
    }

    fn backend(&self) -> Result<&AndroidPlaybackBackend, AndroidPlaybackError> {
        self.backend.get().ok_or(AndroidPlaybackError::Backend {
            detail: "playback backend was not initialized".to_owned(),
        })
    }

    fn notify(&self) {
        if let Ok(state) = self.state.lock() {
            self.listener.on_playback_changed(state.snapshot.clone());
        }
    }

    fn start_current(&self) -> Result<(), AndroidPlaybackError> {
        let (uri, next_uri) = {
            let state = self.lock()?;
            let uri = state
                .current_uri()
                .ok_or(AndroidPlaybackError::InvalidRequest {
                    detail: "the Core queue has no current track".to_owned(),
                })?;
            (uri, state.next_uri())
        };
        let backend = self.backend()?;
        if let Err(error) = backend.play_uri(&uri) {
            let detail = error.to_string();
            if let Ok(mut state) = self.state.lock() {
                state.snapshot.state = AndroidPlaybackState::Stopped;
                state.snapshot.error = Some(detail.clone());
            }
            self.notify();
            return Err(AndroidPlaybackError::Backend { detail });
        }
        {
            let mut state = self.lock()?;
            state.stream = backend.current_generation();
        }
        backend.set_next(next_uri.as_deref());
        self.notify();
        Ok(())
    }

    fn stop_backend(&self) -> Result<(), AndroidPlaybackError> {
        self.backend()?
            .stop()
            .map_err(|error| AndroidPlaybackError::Backend {
                detail: error.to_string(),
            })?;
        self.lock()?.stop();
        self.notify();
        Ok(())
    }

    fn handle_event(&self, event: StreamEvent) {
        enum FollowUp {
            None,
            Start,
            Feed(Option<String>),
            Stop,
        }

        let follow_up = {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            if !state.accepts(event.generation) {
                return;
            }
            match event.event {
                PlayerEvent::StateChanged(playback) => {
                    state.snapshot.state = playback.into();
                    FollowUp::None
                }
                PlayerEvent::Position {
                    position_ms,
                    duration_ms,
                } => {
                    state.snapshot.position_ms = position_ms.max(0);
                    if duration_ms > 0 {
                        state.snapshot.duration_ms = duration_ms;
                    }
                    FollowUp::None
                }
                PlayerEvent::TrackFinished => {
                    if state.queue.advance_auto().is_some() {
                        state.adopt_current();
                        FollowUp::Start
                    } else {
                        state.stop();
                        FollowUp::Stop
                    }
                }
                PlayerEvent::AdvancedToNext => {
                    if state.queue.advance_auto().is_some() {
                        state.adopt_current();
                        FollowUp::Feed(state.next_uri())
                    } else {
                        state.stop();
                        FollowUp::Stop
                    }
                }
                PlayerEvent::Error(message) => {
                    state.snapshot.state = AndroidPlaybackState::Stopped;
                    state.snapshot.error = Some(message);
                    FollowUp::Stop
                }
                PlayerEvent::StreamTags { .. } | PlayerEvent::Spectrum(_) => FollowUp::None,
            }
        };

        match follow_up {
            FollowUp::None => self.notify(),
            FollowUp::Start => {
                let _ = self.start_current();
            }
            FollowUp::Feed(next_uri) => {
                if let Ok(backend) = self.backend() {
                    backend.set_next(next_uri.as_deref());
                }
                self.notify();
            }
            FollowUp::Stop => {
                if let Ok(backend) = self.backend() {
                    let _ = backend.stop();
                }
                self.notify();
            }
        }
    }
}

/// The Android service's one owner of Core queue state and playback commands.
#[derive(uniffi::Object)]
pub struct AndroidPlaybackSession {
    inner: Arc<SessionInner>,
}

#[uniffi::export]
impl AndroidPlaybackSession {
    #[uniffi::constructor]
    pub fn new(
        port: Box<dyn AndroidPlaybackPort>,
        listener: Box<dyn AndroidPlaybackListener>,
    ) -> Result<Self, AndroidPlaybackError> {
        let inner = Arc::new(SessionInner {
            state: Mutex::new(SessionState::new()),
            backend: OnceLock::new(),
            listener,
        });
        let weak = Arc::downgrade(&inner);
        let backend = AndroidPlaybackBackend::new(
            port,
            Box::new(move |event| {
                if let Some(inner) = weak.upgrade() {
                    inner.handle_event(event);
                }
            }),
        )
        .map_err(|error| AndroidPlaybackError::Backend {
            detail: error.to_string(),
        })?;
        if inner.backend.set(backend).is_err() {
            return Err(AndroidPlaybackError::Backend {
                detail: "playback backend was initialized twice".to_owned(),
            });
        }
        inner.backend()?.set_transition(TrackTransition::Gapless, 0);
        Ok(Self { inner })
    }

    pub fn play_tracks(
        &self,
        uris: Vec<String>,
        start_index: u64,
    ) -> Result<(), AndroidPlaybackError> {
        if uris.is_empty() {
            return Err(AndroidPlaybackError::InvalidRequest {
                detail: "a playback queue cannot be empty".to_owned(),
            });
        }
        let start_index =
            usize::try_from(start_index).map_err(|_| AndroidPlaybackError::InvalidRequest {
                detail: "the tapped track index does not fit this device".to_owned(),
            })?;
        if start_index >= uris.len() {
            return Err(AndroidPlaybackError::InvalidRequest {
                detail: "the tapped track is outside the visible list".to_owned(),
            });
        }
        let ids = (0..uris.len())
            .map(|index| i64::try_from(index).unwrap_or(i64::MAX))
            .collect();
        {
            let mut state = self.inner.lock()?;
            state.uris = uris;
            state.queue.set_tracks(ids, start_index);
            state.adopt_current();
        }
        self.inner.start_current()
    }

    pub fn toggle_pause(&self) -> Result<(), AndroidPlaybackError> {
        let playback = self.inner.backend()?.toggle_pause().map_err(|error| {
            AndroidPlaybackError::Backend {
                detail: error.to_string(),
            }
        })?;
        self.inner.lock()?.snapshot.state = playback.into();
        self.inner.notify();
        Ok(())
    }

    pub fn next(&self) -> Result<(), AndroidPlaybackError> {
        self.move_playhead(Queue::next_manual)
    }

    pub fn previous(&self) -> Result<(), AndroidPlaybackError> {
        self.move_playhead(Queue::previous)
    }

    pub fn snapshot(&self) -> Result<AndroidPlaybackSnapshot, AndroidPlaybackError> {
        Ok(self.inner.lock()?.snapshot.clone())
    }
}

impl AndroidPlaybackSession {
    fn move_playhead(
        &self,
        move_queue: impl FnOnce(&mut Queue) -> Option<i64>,
    ) -> Result<(), AndroidPlaybackError> {
        let has_current = {
            let mut state = self.inner.lock()?;
            let has_current = move_queue(&mut state.queue).is_some();
            if has_current {
                state.adopt_current();
            }
            has_current
        };
        if has_current {
            self.inner.start_current()
        } else {
            self.inner.stop_backend()
        }
    }
}
