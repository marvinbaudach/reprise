//! Android's Core-owned playback queue and transport surface.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use reprise_core::playback::{PlaybackBackend, PlayerEvent, StreamEvent, StreamGeneration};
use reprise_core::queue::{Queue, Repeat};

use crate::play_recorder::{PlayRecorder, RecordedPlay};
use crate::playback::{
    AndroidPlaybackBackend, AndroidPlaybackError, AndroidPlaybackPort, AndroidPlaybackState,
};

/// Queue repeat state carried across UniFFI without stringly typed modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AndroidRepeatMode {
    Off,
    All,
    One,
}

impl From<AndroidRepeatMode> for Repeat {
    fn from(mode: AndroidRepeatMode) -> Self {
        match mode {
            AndroidRepeatMode::Off => Repeat::Off,
            AndroidRepeatMode::All => Repeat::All,
            AndroidRepeatMode::One => Repeat::One,
        }
    }
}

/// The playback state rendered by the Android surface.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AndroidPlaybackSnapshot {
    pub state: AndroidPlaybackState,
    pub current_index: Option<u64>,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub shuffled: bool,
    pub repeat: AndroidRepeatMode,
    pub error: Option<String>,
}

#[uniffi::export(callback_interface)]
pub trait AndroidPlaybackListener: Send + Sync {
    fn on_playback_changed(&self, snapshot: AndroidPlaybackSnapshot);
}

struct SessionState {
    queue: Queue,
    track_ids: Vec<i64>,
    uris: Vec<String>,
    snapshot: AndroidPlaybackSnapshot,
    stream: StreamGeneration,
    max_position_ms: i64,
    play_recorded: bool,
}

impl SessionState {
    fn new() -> Self {
        Self {
            queue: Queue::new(),
            track_ids: Vec::new(),
            uris: Vec::new(),
            snapshot: AndroidPlaybackSnapshot {
                state: AndroidPlaybackState::Stopped,
                current_index: None,
                position_ms: 0,
                duration_ms: 0,
                shuffled: false,
                repeat: AndroidRepeatMode::Off,
                error: None,
            },
            stream: StreamGeneration::INITIAL,
            max_position_ms: 0,
            play_recorded: false,
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
        self.max_position_ms = 0;
        self.play_recorded = false;
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

    fn current_track_id(&self) -> Option<i64> {
        self.snapshot
            .current_index
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| self.track_ids.get(index).copied())
    }

    fn play_to_record(&mut self, completed: bool) -> Option<i64> {
        if completed {
            self.max_position_ms = self.max_position_ms.max(self.snapshot.duration_ms);
        }
        if self.play_recorded
            || !reprise_core::library::stats::should_count_play(
                self.max_position_ms,
                self.snapshot.duration_ms,
            )
        {
            return None;
        }
        let track_id = self.current_track_id()?;
        self.play_recorded = true;
        Some(track_id)
    }
}

struct SessionInner {
    state: Mutex<SessionState>,
    backend: OnceLock<AndroidPlaybackBackend>,
    listener: Box<dyn AndroidPlaybackListener>,
    plays: PlayRecorder,
    database_path: PathBuf,
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

        let (follow_up, play_to_record) = {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            if !state.accepts(event.generation) {
                return;
            }
            match event.event {
                PlayerEvent::StateChanged(playback) => {
                    state.snapshot.state = playback.into();
                    (FollowUp::None, None)
                }
                PlayerEvent::Position {
                    position_ms,
                    duration_ms,
                } => {
                    state.snapshot.position_ms = position_ms.max(0);
                    if duration_ms > 0 {
                        state.snapshot.duration_ms = duration_ms;
                    }
                    state.max_position_ms = state.max_position_ms.max(position_ms.max(0));
                    let play = state.play_to_record(false);
                    (FollowUp::None, play)
                }
                PlayerEvent::TrackFinished => {
                    let play = state.play_to_record(true);
                    if state.queue.advance_auto().is_some() {
                        state.adopt_current();
                        (FollowUp::Start, play)
                    } else {
                        state.stop();
                        (FollowUp::Stop, play)
                    }
                }
                PlayerEvent::AdvancedToNext => {
                    let play = state.play_to_record(true);
                    if state.queue.advance_auto().is_some() {
                        state.adopt_current();
                        (FollowUp::Feed(state.next_uri()), play)
                    } else {
                        state.stop();
                        (FollowUp::Stop, play)
                    }
                }
                PlayerEvent::Error(message) => {
                    state.snapshot.state = AndroidPlaybackState::Stopped;
                    state.snapshot.error = Some(message);
                    (FollowUp::Stop, None)
                }
                PlayerEvent::StreamTags { .. } | PlayerEvent::Spectrum(_) => (FollowUp::None, None),
            }
        };

        // Queued, not written: this runs on Media3's application thread, and
        // `FollowUp::Start` below is the gapless transition into the next
        // track. See `play_recorder`.
        if let Some(track_id) = play_to_record {
            self.plays.record(RecordedPlay::now(track_id));
        }

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
        app_private_directory: &str,
        port: Box<dyn AndroidPlaybackPort>,
        listener: Box<dyn AndroidPlaybackListener>,
    ) -> Result<Self, AndroidPlaybackError> {
        let database_path = Path::new(&app_private_directory).join(crate::DATABASE_FILE_NAME);
        let database =
            reprise_core::db::Db::open_migrated(Some(&database_path)).map_err(|error| {
                AndroidPlaybackError::Backend {
                    detail: format!("could not open playback statistics database: {error}"),
                }
            })?;
        let playback_settings = crate::AndroidPlaybackSettings::load(&database);
        let transition = reprise_core::library::settings::get_track_transition(&database);
        let crossfade_seconds = reprise_core::library::settings::get_crossfade_seconds(&database);
        drop(database);
        let inner = Arc::new(SessionInner {
            state: Mutex::new(SessionState::new()),
            backend: OnceLock::new(),
            listener,
            plays: PlayRecorder::spawn(database_path.clone()),
            database_path,
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
        inner.backend()?.set_equalizer(
            playback_settings.equalizer_enabled,
            playback_settings.equalizer_curve,
        )?;
        inner
            .backend()?
            .set_transition(transition, crossfade_seconds);
        Ok(Self { inner })
    }

    pub fn play_tracks(
        &self,
        track_ids: Vec<i64>,
        uris: Vec<String>,
        start_index: u64,
    ) -> Result<(), AndroidPlaybackError> {
        if uris.is_empty() {
            return Err(AndroidPlaybackError::InvalidRequest {
                detail: "a playback queue cannot be empty".to_owned(),
            });
        }
        if track_ids.len() != uris.len() {
            return Err(AndroidPlaybackError::InvalidRequest {
                detail: "track ids and playback URIs must describe the same queue".to_owned(),
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
        let queue_indices = (0..uris.len())
            .map(|index| i64::try_from(index).unwrap_or(i64::MAX))
            .collect();
        {
            let mut state = self.inner.lock()?;
            state.track_ids = track_ids;
            state.uris = uris;
            state.queue.set_tracks(queue_indices, start_index);
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

    pub fn seek_to(&self, position_ms: i64) -> Result<(), AndroidPlaybackError> {
        self.inner
            .backend()?
            .seek_to(position_ms.max(0))
            .map_err(|error| AndroidPlaybackError::Backend {
                detail: error.to_string(),
            })
    }

    pub fn set_shuffle(&self, enabled: bool) -> Result<(), AndroidPlaybackError> {
        let next_uri = {
            let mut state = self.inner.lock()?;
            state.queue.set_shuffle(enabled);
            state.snapshot.shuffled = state.queue.is_shuffled();
            state.next_uri()
        };
        self.inner.backend()?.set_next(next_uri.as_deref());
        self.inner.notify();
        Ok(())
    }

    pub fn set_repeat(&self, mode: AndroidRepeatMode) -> Result<(), AndroidPlaybackError> {
        let next_uri = {
            let mut state = self.inner.lock()?;
            state.queue.set_repeat(mode.into());
            state.snapshot.repeat = mode;
            state.next_uri()
        };
        self.inner.backend()?.set_next(next_uri.as_deref());
        self.inner.notify();
        Ok(())
    }

    pub fn snapshot(&self) -> Result<AndroidPlaybackSnapshot, AndroidPlaybackError> {
        Ok(self.inner.lock()?.snapshot.clone())
    }

    pub fn equalizer_snapshot(
        &self,
    ) -> Result<Option<crate::AndroidEqualizerSnapshot>, AndroidPlaybackError> {
        self.inner.backend()?.equalizer_snapshot()
    }

    /// Re-reads authored settings after an explicit UI write and applies them.
    pub fn reload_playback_settings(&self) -> Result<(), AndroidPlaybackError> {
        let database =
            reprise_core::db::Db::open_ready(&self.inner.database_path).map_err(|error| {
                AndroidPlaybackError::Backend {
                    detail: format!("could not reload playback settings: {error}"),
                }
            })?;
        let playback_settings = crate::AndroidPlaybackSettings::load(&database);
        let transition = reprise_core::library::settings::get_track_transition(&database);
        let crossfade_seconds = reprise_core::library::settings::get_crossfade_seconds(&database);
        let backend = self.inner.backend()?;
        backend.set_equalizer(
            playback_settings.equalizer_enabled,
            playback_settings.equalizer_curve,
        )?;
        backend.set_transition(transition, crossfade_seconds);
        Ok(())
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
