//! Player state and the two queues, owned in one place.
//!
//! `reprise-core` already contains the pure pieces — [`Queue`] for the
//! context the user started from, [`UpNextQueue`] for what they explicitly
//! queued next. What lived only in the GTK controller was the *binding*
//! between them and the audio backend: which of the two supplies the next
//! track, when a finished track advances the cursor, what a snapshot of all
//! that looks like. That binding is here now, with no toolkit in sight.

use reprise_core::playback::{PlaybackBackend, PlaybackState, PlayerEvent};
use reprise_core::queue::{Queue, Repeat};
use reprise_core::up_next::UpNextQueue;
use reprise_runtime_protocol::playback::{PlaybackCommand, PlaybackSnapshot};
use reprise_runtime_protocol::queue::{QueueCommand, QueueSnapshot};

use crate::error::{Failed, Rejected, RuntimeError};
use crate::ports::{LibraryPort, PlayableTrack, TrackLocation};

/// How many ids a queue snapshot carries per section. The totals describe
/// the full sections, so a client can say "and 412 more" without the runtime
/// shipping 412 ids. Same value as the MPRIS mirror already publishes, so
/// agents see no change in window size when Task 3.3 re-points them here.
const QUEUE_WINDOW: usize = 200;

/// Player and queue state.
pub(crate) struct Transport {
    queue: Queue,
    up_next: UpNextQueue,
    status: PlaybackState,
    /// What is loaded in the backend right now. `None` means nothing is —
    /// which is what makes `Stopped` distinguishable from `Paused` with a
    /// track still loaded, the distinction §9.6's idle rule hangs on.
    current: Option<PlayableTrack>,
    position_ms: i64,
    volume: f64,
}

impl Transport {
    pub(crate) fn new() -> Self {
        Self {
            queue: Queue::new(),
            up_next: UpNextQueue::default(),
            status: PlaybackState::Stopped,
            current: None,
            position_ms: 0,
            volume: 1.0,
        }
    }

    /// Whether anything is loaded. §9.6: a paused track still counts as
    /// playback, so the idle timer must not start.
    pub(crate) fn is_active(&self) -> bool {
        self.current.is_some()
    }

    pub(crate) fn playback_snapshot(&self) -> PlaybackSnapshot {
        let track = self.current.as_ref();
        PlaybackSnapshot {
            status: match self.status {
                PlaybackState::Playing => "playing".into(),
                PlaybackState::Paused => "paused".into(),
                PlaybackState::Stopped => "stopped".into(),
            },
            track_id: track.map(|track| track.track_id),
            title: track.map(|track| track.title.clone()).unwrap_or_default(),
            artist: track.map(|track| track.artist.clone()).unwrap_or_default(),
            album: track.map(|track| track.album.clone()).unwrap_or_default(),
            duration_ms: track.map_or(0, |track| track.duration_ms),
            position_ms: self.position_ms,
            volume: self.volume,
            shuffle: self.queue.is_shuffled(),
            repeat: match self.queue.repeat() {
                Repeat::Off => "off".into(),
                Repeat::All => "all".into(),
                Repeat::One => "one".into(),
            },
        }
    }

    pub(crate) fn queue_snapshot(&self) -> QueueSnapshot {
        QueueSnapshot {
            // What is *playing*, which is not always where the context
            // cursor stands: an explicitly queued track plays beside the
            // context without moving it.
            current_track_id: self.current.as_ref().map(|track| track.track_id),
            play_next_track_ids: self
                .up_next
                .ids()
                .iter()
                .copied()
                .take(QUEUE_WINDOW)
                .collect(),
            context_track_ids: self.queue.remaining_window(0, QUEUE_WINDOW),
            play_next_total: self.up_next.len() as u64,
            context_total: self.queue.remaining_len() as u64,
        }
    }

    /// Replaces the context queue and starts playing at `start_index`.
    pub(crate) fn play_tracks(
        &mut self,
        backend: &dyn PlaybackBackend,
        library: &dyn LibraryPort,
        track_ids: Vec<i64>,
        start_index: usize,
    ) -> Result<(), RuntimeError> {
        if track_ids.is_empty() {
            return Err(RuntimeError::Rejected(Rejected::NothingToPlay));
        }
        self.queue.set_tracks(track_ids, start_index);
        let Some(track_id) = self.queue.current() else {
            return Err(RuntimeError::Rejected(Rejected::NothingToPlay));
        };
        self.start(backend, library, track_id)
    }

    pub(crate) fn playback_command(
        &mut self,
        backend: &dyn PlaybackBackend,
        library: &dyn LibraryPort,
        command: &PlaybackCommand,
    ) -> Result<(), RuntimeError> {
        match command {
            PlaybackCommand::Play => self.resume(backend, library),
            PlaybackCommand::Pause => self.pause(backend),
            PlaybackCommand::Stop => self.stop(backend),
            PlaybackCommand::Next => self.skip_forward(backend, library),
            PlaybackCommand::Previous => self.skip_back(backend, library),
            PlaybackCommand::SetVolume(volume) => {
                // Clamping rather than rejecting: the protocol promises the
                // applied value comes back in the next snapshot, so a client
                // learns what happened without a second round trip.
                self.volume = volume.clamp(0.0, 1.0);
                backend.set_volume(self.volume);
                Ok(())
            }
            PlaybackCommand::Seek(delta_ms) => self.seek(backend, *delta_ms),
            PlaybackCommand::SetShuffle(on) => {
                self.queue.set_shuffle(*on);
                Ok(())
            }
            PlaybackCommand::SetRepeat(mode) => {
                let repeat = match mode.as_str() {
                    "off" => Repeat::Off,
                    "all" => Repeat::All,
                    "one" => Repeat::One,
                    _ => return Err(RuntimeError::Rejected(Rejected::UnknownRepeatMode)),
                };
                self.queue.set_repeat(repeat);
                Ok(())
            }
        }
    }

    pub(crate) fn queue_command(&mut self, command: &QueueCommand) {
        match command {
            QueueCommand::AddNext(ids) => self.up_next.append(ids),
            QueueCommand::AddLast(ids) => self.queue.append_tracks(ids),
            // "Clearing a queue is not a stop command" (protocol): only the
            // explicit queue goes; the current track keeps playing.
            QueueCommand::Clear => self.up_next = UpNextQueue::default(),
        }
    }

    /// Applies an asynchronous report from the audio backend.
    ///
    /// These are not commands and cannot fail towards a client — there is
    /// nobody waiting on them. A backend error stops playback and is logged.
    pub(crate) fn player_event(
        &mut self,
        backend: &dyn PlaybackBackend,
        library: &dyn LibraryPort,
        event: &PlayerEvent,
    ) {
        match event {
            PlayerEvent::StateChanged(state) => self.status = *state,
            PlayerEvent::Position {
                position_ms,
                duration_ms,
            } => {
                self.position_ms = *position_ms;
                // The backend knows the real duration; the library's value is
                // a guess from tags. Adopt it once it is known.
                if let Some(track) = self.current.as_mut() {
                    if *duration_ms > 0 {
                        track.duration_ms = *duration_ms;
                    }
                }
            }
            PlayerEvent::TrackFinished => {
                if let Some(next) = self.take_next_auto() {
                    let _ = self.start(backend, library, next);
                } else {
                    let _ = self.stop(backend);
                }
            }
            PlayerEvent::AdvancedToNext => {
                // The backend handed off to a pre-fed track without a
                // restart, so the audio is already rolling: advance the model
                // by one and do NOT call play. Nothing pre-feeds yet (that
                // arrives with the gapless setting in Task 3.3), which is
                // exactly why this branch stays defensive rather than absent.
                if let Some(next) = self.take_next_auto() {
                    self.load(library, next);
                }
            }
            PlayerEvent::StreamTags { title, .. } => {
                if let (Some(track), Some(title)) = (self.current.as_mut(), title.as_ref()) {
                    track.title.clone_from(title);
                }
            }
            // A visualizer frame arrives ~60×/s and is a rendering concern of
            // whichever surface draws it, not runtime state. Publishing it as
            // an event would flood every client's mailbox.
            PlayerEvent::Spectrum(_) => {}
            PlayerEvent::Error(message) => {
                tracing::warn!(%message, "playback backend reported an error");
                let _ = self.stop(backend);
            }
        }
    }

    fn resume(
        &mut self,
        backend: &dyn PlaybackBackend,
        library: &dyn LibraryPort,
    ) -> Result<(), RuntimeError> {
        match self.status {
            PlaybackState::Playing => Ok(()),
            PlaybackState::Paused => {
                self.status = backend
                    .toggle_pause()
                    .map_err(|error| backend_failed(&error))?;
                Ok(())
            }
            PlaybackState::Stopped => {
                let track_id = self
                    .current
                    .as_ref()
                    .map(|track| track.track_id)
                    .or_else(|| self.queue.current())
                    .ok_or(RuntimeError::Rejected(Rejected::NothingToPlay))?;
                self.start(backend, library, track_id)
            }
        }
    }

    fn pause(&mut self, backend: &dyn PlaybackBackend) -> Result<(), RuntimeError> {
        if self.status != PlaybackState::Playing {
            return Ok(());
        }
        self.status = backend
            .toggle_pause()
            .map_err(|error| backend_failed(&error))?;
        Ok(())
    }

    fn stop(&mut self, backend: &dyn PlaybackBackend) -> Result<(), RuntimeError> {
        let result = backend.stop().map_err(|error| backend_failed(&error));
        self.status = PlaybackState::Stopped;
        self.current = None;
        self.position_ms = 0;
        result
    }

    fn skip_forward(
        &mut self,
        backend: &dyn PlaybackBackend,
        library: &dyn LibraryPort,
    ) -> Result<(), RuntimeError> {
        // The explicit queue wins over the context: it is what the user
        // asked for most recently and most deliberately.
        let next = self
            .up_next
            .pop_front()
            .or_else(|| self.queue.next_manual());
        match next {
            Some(track_id) => self.start(backend, library, track_id),
            None => self.stop(backend),
        }
    }

    fn skip_back(
        &mut self,
        backend: &dyn PlaybackBackend,
        library: &dyn LibraryPort,
    ) -> Result<(), RuntimeError> {
        match self.queue.previous() {
            Some(track_id) => self.start(backend, library, track_id),
            // Nothing before the first track: staying put is the expected
            // behaviour of every player, not an error worth reporting.
            None => Ok(()),
        }
    }

    fn seek(&mut self, backend: &dyn PlaybackBackend, delta_ms: i64) -> Result<(), RuntimeError> {
        let Some(duration_ms) = self.current.as_ref().map(|track| track.duration_ms) else {
            return Err(RuntimeError::Rejected(Rejected::NothingToPlay));
        };
        let target = self
            .position_ms
            .saturating_add(delta_ms)
            .clamp(0, duration_ms.max(0));
        backend
            .seek_to(target)
            .map_err(|error| backend_failed(&error))?;
        self.position_ms = target;
        Ok(())
    }

    /// What plays when the current track ends by itself, as opposed to being
    /// skipped: the explicit queue first, then the context's own advance
    /// (which is where repeat and shuffle apply).
    fn take_next_auto(&mut self) -> Option<i64> {
        self.up_next
            .pop_front()
            .or_else(|| self.queue.advance_auto())
    }

    /// Resolves, hands the location to the backend, and adopts it as current.
    fn start(
        &mut self,
        backend: &dyn PlaybackBackend,
        library: &dyn LibraryPort,
        track_id: i64,
    ) -> Result<(), RuntimeError> {
        let track = library
            .resolve(track_id)
            .ok_or(RuntimeError::Failed(Failed::TrackNotPlayable))?;
        let started = match &track.location {
            TrackLocation::Path(path) => backend.play(path),
            TrackLocation::Uri(uri) => backend.play_uri(uri),
        };
        started.map_err(|error| backend_failed(&error))?;
        self.current = Some(track);
        self.position_ms = 0;
        self.status = PlaybackState::Playing;
        Ok(())
    }

    /// Adopts a track as current *without* telling the backend to play it —
    /// for the gapless handoff, where the audio is already running.
    fn load(&mut self, library: &dyn LibraryPort, track_id: i64) {
        self.current = library.resolve(track_id);
        self.position_ms = 0;
    }
}

/// Keeps the backend's message in the log, where a path is allowed, and
/// hands the client the category (§9.7).
fn backend_failed(error: &reprise_core::playback::PlaybackError) -> RuntimeError {
    tracing::warn!(%error, "playback backend rejected a command");
    RuntimeError::Failed(Failed::PlaybackBackend)
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod transport_tests;
