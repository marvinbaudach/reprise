//! Android's Core-owned playback queue and transport surface.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use reprise_core::playback::{PlaybackBackend, StreamGeneration};
use reprise_core::queue::{Queue, Repeat};

use crate::listen_export_recorder::ListenExportRecorder;
use crate::play_recorder::PlayRecorder;
use crate::playback::{
    AndroidPlaybackBackend, AndroidPlaybackError, AndroidPlaybackPort, AndroidPlaybackState,
};

mod history;
mod queue_boundary;
mod queue_persistence;
mod stream_events;
mod trash_boundary;

pub use trash_boundary::{AndroidTrashFailure, AndroidTrashReport, TrashAction};

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
    pub current_track_id: Option<i64>,
    pub current_track_uri: Option<String>,
    pub position_ms: i64,
    pub duration_ms: i64,
    /// Rises only when the backend reports that the current track ended.
    pub automatic_advance_count: u64,
    pub shuffled: bool,
    pub repeat: AndroidRepeatMode,
    pub error: Option<String>,
}

#[uniffi::export(callback_interface)]
pub trait AndroidPlaybackListener: Send + Sync {
    fn on_playback_changed(&self, snapshot: AndroidPlaybackSnapshot);
    fn on_listen_report_changed(&self);
}

struct SessionState {
    queue: Queue,
    /// PLAY-14 runtime playback history; see `playback_session/history.rs`.
    history: history::HistoryState,
    track_ids: Vec<i64>,
    track_index_by_id: HashMap<i64, usize>,
    uris: Vec<String>,
    snapshot: AndroidPlaybackSnapshot,
    stream: StreamGeneration,
    current_loaded: bool,
    consecutive_faults: usize,
    fault_skip_limit: Option<usize>,
    max_position_ms: i64,
    play_recorded: bool,
}

impl SessionState {
    #[cfg(test)]
    fn new() -> Self {
        Self {
            queue: Queue::new(),
            history: history::HistoryState::default(),
            track_ids: Vec::new(),
            track_index_by_id: HashMap::new(),
            uris: Vec::new(),
            snapshot: AndroidPlaybackSnapshot {
                state: AndroidPlaybackState::Stopped,
                current_index: None,
                current_track_id: None,
                current_track_uri: None,
                position_ms: 0,
                duration_ms: 0,
                automatic_advance_count: 0,
                shuffled: false,
                repeat: AndroidRepeatMode::Off,
                error: None,
            },
            stream: StreamGeneration::INITIAL,
            current_loaded: false,
            consecutive_faults: 0,
            fault_skip_limit: None,
            max_position_ms: 0,
            play_recorded: false,
        }
    }

    fn from_restored(restored: queue_persistence::RestoredQueue) -> Self {
        let repeat = match restored.queue.repeat() {
            Repeat::Off => AndroidRepeatMode::Off,
            Repeat::All => AndroidRepeatMode::All,
            Repeat::One => AndroidRepeatMode::One,
        };
        let track_index_by_id = index_tracks(&restored.track_ids);
        let current_index = restored
            .queue
            .current()
            .and_then(|track_id| track_index_by_id.get(&track_id).copied())
            .and_then(|index| u64::try_from(index).ok());
        let state = if current_index.is_some() {
            AndroidPlaybackState::Paused
        } else {
            AndroidPlaybackState::Stopped
        };
        Self {
            snapshot: AndroidPlaybackSnapshot {
                state,
                current_index,
                current_track_id: None,
                current_track_uri: None,
                position_ms: 0,
                duration_ms: 0,
                automatic_advance_count: 0,
                shuffled: restored.queue.is_shuffled(),
                repeat,
                error: None,
            },
            queue: restored.queue,
            history: history::HistoryState::default(),
            track_ids: restored.track_ids,
            track_index_by_id,
            uris: restored.uris,
            stream: StreamGeneration::INITIAL,
            current_loaded: false,
            consecutive_faults: 0,
            fault_skip_limit: None,
            max_position_ms: 0,
            play_recorded: false,
        }
    }

    fn current_uri(&self) -> Option<String> {
        if let Some(target) = self.history.presented() {
            return target.replay_uri.clone();
        }
        self.queue
            .current()
            .and_then(|track_id| self.track_index(track_id))
            .and_then(|index| self.uris.get(index).cloned())
    }

    fn next_uri(&self) -> Option<String> {
        self.queue
            .peek_auto()
            .and_then(|track_id| self.track_index(track_id))
            .and_then(|index| self.uris.get(index).cloned())
    }

    fn track_index(&self, track_id: i64) -> Option<usize> {
        self.track_index_by_id
            .get(&track_id)
            .copied()
            .filter(|index| self.track_ids.get(*index) == Some(&track_id))
    }

    fn set_tracks(&mut self, track_ids: Vec<i64>, uris: Vec<String>, start_index: usize) {
        self.track_index_by_id = index_tracks(&track_ids);
        self.track_ids = track_ids.clone();
        self.uris = uris;
        self.queue.set_tracks(track_ids, start_index);
        self.adopt_current_for_play_intent();
    }

    fn adopt_current(&mut self) {
        self.history.clear_presented();
        self.snapshot.current_index = self
            .queue
            .current_order_position()
            .and_then(|index| u64::try_from(index).ok());
        self.snapshot.position_ms = 0;
        self.snapshot.duration_ms = 0;
        self.current_loaded = false;
        self.snapshot.state = AndroidPlaybackState::Playing;
        self.max_position_ms = 0;
        self.play_recorded = false;
    }

    fn adopt_current_for_play_intent(&mut self) {
        self.reset_fault_run();
        self.adopt_current();
    }

    fn reset_fault_run(&mut self) {
        self.consecutive_faults = 0;
        self.fault_skip_limit = None;
        self.snapshot.error = None;
    }

    fn stop(&mut self) {
        self.history.clear_presented();
        self.snapshot.state = AndroidPlaybackState::Stopped;
        self.snapshot.current_index = None;
        self.snapshot.position_ms = 0;
        self.snapshot.duration_ms = 0;
        self.current_loaded = false;
    }

    fn accepts(&mut self, generation: StreamGeneration) -> bool {
        if generation < self.stream {
            return false;
        }
        self.stream = generation;
        true
    }

    fn current_track_id(&self) -> Option<i64> {
        if let Some(target) = self.history.presented() {
            return target.item.track_id();
        }
        self.queue.current()
    }

    fn presented_snapshot(&self) -> AndroidPlaybackSnapshot {
        let mut snapshot = self.snapshot.clone();
        let identity = self
            .history
            .presented()
            .and_then(|target| target.item.track_id().zip(target.replay_uri.as_ref()))
            .or_else(|| {
                self.queue.current().and_then(|track_id| {
                    self.track_index(track_id)
                        .and_then(|index| self.uris.get(index))
                        .map(|uri| (track_id, uri))
                })
            });
        match identity {
            Some((track_id, uri)) => {
                snapshot.current_track_id = Some(track_id);
                snapshot.current_track_uri = Some(uri.clone());
            }
            None => {
                snapshot.current_track_id = None;
                snapshot.current_track_uri = None;
            }
        }
        snapshot
    }

    fn play_to_record(&mut self, completed: bool) -> Option<(i64, u64)> {
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
        Some((
            track_id,
            u64::try_from(self.max_position_ms.max(0)).unwrap_or(u64::MAX),
        ))
    }
}

fn index_tracks(track_ids: &[i64]) -> HashMap<i64, usize> {
    let mut indices = HashMap::with_capacity(track_ids.len());
    for (index, track_id) in track_ids.iter().copied().enumerate() {
        indices.entry(track_id).or_insert(index);
    }
    indices
}

/// # The two mutexes, and why their order differs between operations
///
/// `state` and `database` are never held at the same time. Every caller takes
/// one inside a block that ends before the other is taken — `enqueue_tracks`
/// reads the database, drops it, then edits the state. `trash_tracks` takes the
/// database to plan, releases it for the trash callbacks, takes it again to
/// commit, edits the state after dropping that guard, and lets `persist_queue`
/// take the database a third time after the state guard drops. `upcoming_tracks`
/// has to read the state first to know *which* ids to ask the database about,
/// and takes the state again afterwards to prune what the database no longer
/// knows. That is a different order, but not a lock-order inversion: an
/// inversion needs one thread holding A while waiting for B, and no path here
/// holds either guard across the other's acquisition.
///
/// The rule this file keeps is therefore "one guard at a time", not "always
/// this order" — the latter cannot be honoured by a query whose parameters come
/// out of the state it also updates.
struct SessionInner {
    state: Mutex<SessionState>,
    library: Arc<crate::MusicLibrary>,
    backend: OnceLock<AndroidPlaybackBackend>,
    listener: Arc<dyn AndroidPlaybackListener>,
    plays: PlayRecorder,
    listen_exports: ListenExportRecorder,
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

    fn persist_queue(&self, queue: &Queue) -> Result<(), AndroidPlaybackError> {
        let database = self
            .library
            .writer()
            .map_err(|error| AndroidPlaybackError::Backend {
                detail: error.to_string(),
            })?;
        queue_persistence::save(&database, queue).map_err(|error| AndroidPlaybackError::Backend {
            detail: format!("could not save the playback queue: {error}"),
        })
    }

    fn notify(&self) {
        if let Ok(state) = self.state.lock() {
            self.listener
                .on_playback_changed(state.presented_snapshot());
        }
    }

    fn start_current(&self) -> Result<(), AndroidPlaybackError> {
        let backend = self.backend()?;
        let (uri, next_uri, history_entry) = {
            let mut state = self.lock()?;
            let track_id =
                state
                    .current_track_id()
                    .ok_or(AndroidPlaybackError::InvalidRequest {
                        detail: "the Core queue has no current track".to_owned(),
                    })?;
            let uri = state
                .current_uri()
                .ok_or(AndroidPlaybackError::InvalidRequest {
                    detail: "the Core queue has no current track".to_owned(),
                })?;
            let next_uri = state.next_uri();
            let history_entry = state.history_entry_for_started(track_id, uri.clone());
            // `play_uri` may synchronously publish this stream's first event.
            state.current_loaded = true;
            (uri, next_uri, history_entry)
        };
        if let Err(error) = backend.play_uri(&uri) {
            let detail = error.to_string();
            if let Ok(mut state) = self.state.lock() {
                state.snapshot.state = AndroidPlaybackState::Stopped;
                state.snapshot.error = Some(detail.clone());
                state.current_loaded = false;
            }
            self.notify();
            return Err(AndroidPlaybackError::Backend { detail });
        }
        {
            let mut state = self.lock()?;
            state.note_playback_started(history_entry);
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
}

/// The Android service's one owner of Core queue state and playback commands.
#[derive(uniffi::Object)]
pub struct AndroidPlaybackSession {
    inner: Arc<SessionInner>,
}

#[uniffi::export]
impl AndroidPlaybackSession {
    #[uniffi::constructor]
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(
        library: Arc<crate::MusicLibrary>,
        port: Box<dyn AndroidPlaybackPort>,
        listener: Box<dyn AndroidPlaybackListener>,
    ) -> Result<Self, AndroidPlaybackError> {
        let database = library
            .reader()
            .map_err(|error| AndroidPlaybackError::Backend {
                detail: format!("could not read the playback database: {error}"),
            })?;
        let restored = queue_persistence::restore(&database).map_err(|error| {
            AndroidPlaybackError::Backend {
                detail: format!("could not restore the playback queue: {error}"),
            }
        })?;
        let playback_settings = crate::AndroidPlaybackSettings::load(&database);
        let transition = reprise_core::library::settings::get_track_transition(&database);
        let crossfade_seconds = reprise_core::library::settings::get_crossfade_seconds(&database);
        let applied_play_sequence =
            reprise_core::library::stats::play_journal_high_water(&database).map_err(|error| {
                AndroidPlaybackError::Backend {
                    detail: format!("could not read playback statistics journal state: {error}"),
                }
            })?;
        drop(database);
        let listener: Arc<dyn AndroidPlaybackListener> = Arc::from(listener);
        let report_listener = Arc::clone(&listener);
        let inner = Arc::new(SessionInner {
            state: Mutex::new(SessionState::from_restored(restored)),
            library: Arc::clone(&library),
            backend: OnceLock::new(),
            listener,
            plays: PlayRecorder::spawn(
                library.database_path.clone(),
                library.writer_handle(),
                applied_play_sequence,
            ),
            listen_exports: ListenExportRecorder::spawn(
                library.database_path.clone(),
                library.reader_handle(),
                Arc::new(move || report_listener.on_listen_report_changed()),
            ),
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
        let queue_to_save = {
            let mut state = self.inner.lock()?;
            state.set_tracks(track_ids, uris, start_index);
            state.queue.clone()
        };
        self.inner.persist_queue(&queue_to_save)?;
        self.inner.start_current()
    }

    pub fn toggle_pause(&self) -> Result<(), AndroidPlaybackError> {
        let start_restored = {
            let mut state = self.inner.lock()?;
            let start_restored = state.snapshot.state == AndroidPlaybackState::Paused
                && !state.current_loaded
                && state.queue.current().is_some();
            if start_restored {
                state.snapshot.state = AndroidPlaybackState::Playing;
            }
            start_restored
        };
        if start_restored {
            return self.inner.start_current();
        }
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
        if self.inner.forward_from_history()? {
            return Ok(());
        }
        self.move_playhead(Queue::next_manual)
    }

    /// PLAY-14: Previous follows playback history, never the queue cursor.
    pub fn previous(&self) -> Result<(), AndroidPlaybackError> {
        self.inner.previous_from_history()
    }

    /// Moves one position backward in the queue's current play order.
    ///
    /// Android's spatial player uses this separately named route so its left
    /// and right neighbours remain reversible. The history-based `previous`
    /// entry point stays intact for callers that present playback history.
    pub fn previous_in_queue_order(&self) -> Result<(), AndroidPlaybackError> {
        let queue_to_save = {
            let mut state = self.inner.lock()?;
            let Some(previous) = state
                .queue
                .current_order_position()
                .and_then(|position| position.checked_sub(1))
            else {
                return Ok(());
            };
            if state.queue.jump_to_order_position(previous).is_none() {
                return Ok(());
            }
            state.adopt_current_for_play_intent();
            state.queue.clone()
        };
        self.inner.persist_queue(&queue_to_save)?;
        self.inner.start_current()
    }

    pub fn seek_to(&self, position_ms: i64) -> Result<(), AndroidPlaybackError> {
        self.inner
            .backend()?
            .seek_to(position_ms.max(0))
            .map_err(|error| AndroidPlaybackError::Backend {
                detail: error.to_string(),
            })
    }

    // UniFFI transfers optional byte buffers by value across the ABI.
    #[allow(clippy::needless_pass_by_value)]
    pub fn prepare_listen_report(
        &self,
        acknowledgement: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, AndroidPlaybackError> {
        crate::listen_export_journal::prepare_report(
            &self.inner.library.database_path,
            acknowledgement.as_deref(),
        )
        .map_err(|error| AndroidPlaybackError::Backend {
            detail: format!("could not prepare listen report: {error}"),
        })
    }

    pub fn set_shuffle(&self, enabled: bool) -> Result<(), AndroidPlaybackError> {
        let (next_uri, queue_to_save) = {
            let mut state = self.inner.lock()?;
            state.queue.set_shuffle(enabled);
            state.snapshot.shuffled = state.queue.is_shuffled();
            state.snapshot.current_index = state
                .queue
                .current_order_position()
                .and_then(|index| u64::try_from(index).ok());
            (state.next_uri(), state.queue.clone())
        };
        self.inner.persist_queue(&queue_to_save)?;
        self.inner.backend()?.set_next(next_uri.as_deref());
        self.inner.notify();
        Ok(())
    }

    pub fn set_repeat(&self, mode: AndroidRepeatMode) -> Result<(), AndroidPlaybackError> {
        let (next_uri, queue_to_save) = {
            let mut state = self.inner.lock()?;
            state.queue.set_repeat(mode.into());
            state.snapshot.repeat = mode;
            (state.next_uri(), state.queue.clone())
        };
        self.inner.persist_queue(&queue_to_save)?;
        self.inner.backend()?.set_next(next_uri.as_deref());
        self.inner.notify();
        Ok(())
    }

    pub fn snapshot(&self) -> Result<AndroidPlaybackSnapshot, AndroidPlaybackError> {
        Ok(self.inner.lock()?.presented_snapshot())
    }

    pub fn equalizer_snapshot(
        &self,
    ) -> Result<Option<crate::AndroidEqualizerSnapshot>, AndroidPlaybackError> {
        self.inner.backend()?.equalizer_snapshot()
    }

    /// Re-reads authored settings after an explicit UI write and applies them.
    pub fn reload_playback_settings(&self) -> Result<(), AndroidPlaybackError> {
        let (playback_settings, transition, crossfade_seconds) = {
            let database =
                self.inner
                    .library
                    .reader()
                    .map_err(|error| AndroidPlaybackError::Backend {
                        detail: format!("could not reload playback settings: {error}"),
                    })?;
            (
                crate::AndroidPlaybackSettings::load(&database),
                reprise_core::library::settings::get_track_transition(&database),
                reprise_core::library::settings::get_crossfade_seconds(&database),
            )
        };
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
        let (has_current, queue_to_save) = {
            let mut state = self.inner.lock()?;
            let has_current = move_queue(&mut state.queue).is_some();
            if has_current {
                state.adopt_current_for_play_intent();
            }
            (has_current, state.queue.clone())
        };
        self.inner.persist_queue(&queue_to_save)?;
        if has_current {
            self.inner.start_current()
        } else {
            self.inner.stop_backend()
        }
    }
}

#[cfg(test)]
#[path = "playback_session_tests.rs"]
mod tests;
