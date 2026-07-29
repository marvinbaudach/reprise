//! Player state and the two queues, owned in one place.
//!
//! `reprise-core` already contains the pure pieces — [`Queue`] for the
//! context the user started from, [`UpNextQueue`] for what they explicitly
//! queued next. What lived only in the GTK controller was the *binding*
//! between them and the audio backend: which of the two supplies the next
//! track, when a finished track advances the cursor, what a snapshot of all
//! that looks like. That binding is here now, with no toolkit in sight.

use reprise_core::playback::{PlaybackBackend, PlaybackState, PlayerEvent, StreamGeneration};
use reprise_core::queue::{Queue, Repeat};
use reprise_core::up_next::UpNextQueue;
use reprise_runtime_protocol::playback::{PlaybackCommand, PlaybackSnapshot};
use reprise_runtime_protocol::queue::{QueueCommand, QueueSnapshot};

use reprise_runtime_protocol::playback::ExternalMedia;

use crate::error::{Failed, Rejected, RuntimeError};
use crate::ports::{LibraryPort, PlayableTrack, TrackLocation};

/// How many ids a queue snapshot carries per section. The totals describe
/// the full sections, so a client can say "and 412 more" without the runtime
/// shipping 412 ids. Same value as the MPRIS mirror already publishes, so
/// agents see no change in window size when Task 3.3 re-points them here.
const QUEUE_WINDOW: usize = 200;

/// What is loaded in the backend, however it got there.
///
/// A library track and a radio stream differ in exactly two ways that the
/// rest of this module cares about, so those are the two extra fields: one
/// has a library id and one does not, and one advances the queue when it
/// ends while the other simply stops.
struct Loaded {
    /// Absent for anything without a library id — a stream, an episode, a
    /// preview render. A client must never invent one.
    track_id: Option<i64>,
    title: String,
    artist: String,
    album: String,
    duration_ms: i64,
    /// Whether the end of this is the queue's cue to move on. False for
    /// external media: finishing a podcast episode must not start the music
    /// that happened to be queued behind it.
    from_queue: bool,
}

impl From<PlayableTrack> for Loaded {
    fn from(track: PlayableTrack) -> Self {
        Self {
            track_id: Some(track.track_id),
            title: track.title,
            artist: track.artist,
            album: track.album,
            duration_ms: track.duration_ms,
            from_queue: true,
        }
    }
}

/// A start that did not happen, in the two shapes a client can act on.
struct Failure {
    /// The library track it was about, absent for anything without an id.
    track_id: Option<i64>,
    /// `not_playable` or `backend` — the wire vocabulary, kept here so the
    /// snapshot is a move rather than a second translation table.
    kind: &'static str,
}

/// Player and queue state.
pub(crate) struct Transport {
    queue: Queue,
    up_next: UpNextQueue,
    status: PlaybackState,
    /// What is loaded in the backend right now. `None` means nothing is —
    /// which is what makes `Stopped` distinguishable from `Paused` with a
    /// track still loaded, the distinction §9.6's idle rule hangs on.
    current: Option<Loaded>,
    position_ms: i64,
    volume: f64,
    /// Which client started the playback that is loaded, if any.
    ///
    /// Established by the two commands that *begin* a session — `PlayTracks`
    /// and `PlayExternal` — and inherited by everything that happens inside
    /// it: an automatic advance, a skip, a pause. Pressing Next in one
    /// surface does not take a session over from the surface that started
    /// it; only starting a new one does.
    initiated_by: Option<u64>,
    /// Why the last automatic start did not happen, until something plays.
    ///
    /// Kept because stopping is not self-explaining: a queue that ran out and
    /// a queue that hit three unreadable files in a row both end `Stopped`
    /// with nothing loaded. §9.5 does not allow an event saying "a track
    /// failed" — a facet says what it looks like now — so the reason lives in
    /// the facet and is cleared by the next successful start.
    failure: Option<Failure>,
    /// The stream whose reports this transport still believes.
    ///
    /// A backend event is delivered asynchronously, so one emitted for the
    /// track that *just* ended can arrive after the next has already been
    /// started. Applied blindly it advances the queue a second time — the
    /// user presses Next once and two tracks go by — or overwrites the new
    /// track's position with the old one's. The backend stamps every event
    /// with the stream it came from; this is the stamp to compare against.
    stream: StreamGeneration,
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
            initiated_by: None,
            failure: None,
            stream: StreamGeneration::INITIAL,
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
            track_id: track.and_then(|track| track.track_id),
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
            failure_kind: self.failure.as_ref().map(|failure| failure.kind.into()),
            failure_track_id: self.failure.as_ref().and_then(|failure| failure.track_id),
            initiated_by: self.current.as_ref().and(self.initiated_by),
        }
    }

    /// The queue facet, *unstamped*: the revision belongs to the runtime,
    /// which is the only place that can count observable changes to this
    /// facet without drifting from what a client actually saw. Leaving it at
    /// zero here also keeps the before/after comparison honest — a revision
    /// baked in on both sides would either always differ or never.
    pub(crate) fn queue_snapshot(&self) -> QueueSnapshot {
        QueueSnapshot {
            revision: 0,
            // What is *playing*, which is not always where the context
            // cursor stands: an explicitly queued track plays beside the
            // context without moving it.
            current_track_id: self.current.as_ref().and_then(|track| track.track_id),
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
        initiated_by: Option<u64>,
    ) -> Result<(), RuntimeError> {
        if track_ids.is_empty() {
            return Err(RuntimeError::Rejected(Rejected::NothingToPlay));
        }
        self.queue.set_tracks(track_ids, start_index);
        let Some(track_id) = self.queue.current() else {
            return Err(RuntimeError::Rejected(Rejected::NothingToPlay));
        };
        let started = self.start(backend, library, track_id);
        if started.is_ok() {
            self.initiated_by = initiated_by;
        }
        started
    }

    /// Plays something that is not a library track.
    ///
    /// The queue is left exactly as it is — not cleared, not advanced. A
    /// podcast episode plays *beside* the music, and going back to the queue
    /// afterwards must find it where the user left it.
    pub(crate) fn play_external(
        &mut self,
        backend: &dyn PlaybackBackend,
        media: &ExternalMedia,
        initiated_by: Option<u64>,
    ) -> Result<(), RuntimeError> {
        if media.location.trim().is_empty() {
            return Err(RuntimeError::Rejected(Rejected::NothingToPlay));
        }
        let started = if media.remote {
            backend.play_uri(&media.location)
        } else {
            backend.play(&media.location)
        };
        if let Err(error) = started {
            self.abandon(backend);
            return Err(backend_failed(&error));
        }
        self.current = Some(Loaded {
            track_id: None,
            title: media.title.clone(),
            artist: media.artist.clone(),
            album: String::new(),
            duration_ms: media.duration_ms,
            from_queue: false,
        });
        self.position_ms = 0;
        self.status = PlaybackState::Playing;
        self.initiated_by = initiated_by;
        self.failure = None;
        self.stream = backend.current_generation();
        Ok(())
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

    pub(crate) fn queue_command(
        &mut self,
        backend: &dyn PlaybackBackend,
        library: &dyn LibraryPort,
        command: &QueueCommand,
    ) -> Result<u64, RuntimeError> {
        // Every arm answers the same question: how many entries did this
        // actually change. Not how many it was handed — a purge for a track
        // that was never queued changed nothing, and saying otherwise would
        // put a number in a toast that the user can see is wrong.
        let affected = match command {
            // Both ends of the *explicit* queue, not one of each: "play
            // next" jumps the manual line and "add to queue" joins its back.
            // Neither touches the surrounding context, which is what makes
            // them undoable by clearing the queue.
            QueueCommand::AddNext(ids) => {
                self.up_next.prepend(ids);
                ids.len()
            }
            QueueCommand::AddLast(ids) => {
                self.up_next.append(ids);
                ids.len()
            }
            // "Clearing a queue is not a stop command" (protocol): only the
            // explicit queue goes; the current track keeps playing.
            QueueCommand::Clear => {
                let dropped = self.up_next.len();
                self.up_next = UpNextQueue::default();
                dropped
            }
            QueueCommand::Move { from, to, .. } => {
                let (from, to) = (as_index(*from), as_index(*to));
                if !self.up_next.move_item(from, to) {
                    return Err(RuntimeError::Rejected(Rejected::NoSuchQueueEntry));
                }
                1
            }
            QueueCommand::RemoveAt { positions, .. } => {
                let positions: Vec<usize> = positions.iter().map(|at| as_index(*at)).collect();
                let removed = self.up_next.remove_positions(&positions);
                if removed == 0 {
                    return Err(RuntimeError::Rejected(Rejected::NoSuchQueueEntry));
                }
                removed
            }
            QueueCommand::RemoveContextAt { positions, .. } => {
                let positions: Vec<usize> = positions.iter().map(|at| as_index(*at)).collect();
                let removed = self.queue.remove_order_positions(&positions);
                if removed == 0 {
                    return Err(RuntimeError::Rejected(Rejected::NoSuchQueueEntry));
                }
                removed
            }
            QueueCommand::PlayNextAt { position, .. } => {
                let track_id = self
                    .up_next
                    .take_at(as_index(*position))
                    .ok_or(RuntimeError::Rejected(Rejected::NoSuchQueueEntry))?;
                return self.start(backend, library, track_id).map(|()| 1);
            }
            QueueCommand::PlayContextAt { position, .. } => {
                let track_id = self
                    .queue
                    .play_order_position_now(as_index(*position))
                    .ok_or(RuntimeError::Rejected(Rejected::NoSuchQueueEntry))?;
                return self.start(backend, library, track_id).map(|()| 1);
            }
            QueueCommand::Purge(ids) => {
                let from_up_next = self.up_next.remove_ids(ids);
                // `_except_current` deliberately: a track that is playing
                // when its file is deleted finishes, because stopping the
                // music is not what deleting a file asked for.
                from_up_next + self.queue.remove_ids_except_current(ids)
            }
        };
        Ok(affected as u64)
    }

    /// Applies an asynchronous report from the audio backend.
    ///
    /// These are not commands and cannot fail towards a client — there is
    /// nobody waiting on them. A backend error stops playback and is logged.
    /// Whether a report stamped `stream` still describes what is loaded.
    ///
    /// A *newer* stamp is adopted rather than discarded: it can only mean
    /// something started a stream this transport has not caught up with yet,
    /// and refusing it would leave the runtime deaf to the very pipeline it
    /// is supposed to be reporting on.
    pub(crate) fn accepts_stream(&mut self, stream: StreamGeneration) -> bool {
        if stream < self.stream {
            return false;
        }
        self.stream = stream;
        true
    }

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
                // Only what the queue started hands back to the queue. A
                // finished episode or a stream that dropped must not launch
                // whatever music was waiting behind it — the user did not
                // ask for that, and it is loud.
                let advances = self.current.as_ref().is_none_or(|loaded| loaded.from_queue);
                if advances {
                    self.advance_past_failures(backend, library);
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
                    self.load(backend, library, next);
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
                // Recorded before the stop, because stopping clears what was
                // loaded and the id is the only thing that lets a surface say
                // *which* track dropped out.
                self.failure = Some(Failure {
                    track_id: self.current.as_ref().and_then(|loaded| loaded.track_id),
                    kind: "backend",
                });
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
                // Only a library track can be restarted from here. External
                // media has no id to resolve and, for a stream, no position
                // to resume to — the surface that knows where it came from
                // re-sends it, which is also how a radio station reconnects.
                let track_id = self
                    .current
                    .as_ref()
                    .and_then(|track| track.track_id)
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

    /// Stops playback and clears what is loaded — but only once the backend
    /// has actually gone quiet.
    ///
    /// The tempting version clears `current` unconditionally and returns the
    /// backend's error as an afterthought: a caller reading only the
    /// snapshot then sees "nothing playing" while GStreamer is still
    /// audible, and because `is_active()` reads `current`, the idle
    /// shutdown would believe it is safe to fire over a live pipeline. The
    /// conservative direction is the only one that keeps that promise: on a
    /// failed stop, `current`, `status` and `position_ms` all stay exactly
    /// as they were, so `is_active()` keeps telling the truth. The caller
    /// gets the error back and is not left stuck — the very same command
    /// that failed is the retry, and it reaches this exact `backend.stop()`
    /// call again.
    fn stop(&mut self, backend: &dyn PlaybackBackend) -> Result<(), RuntimeError> {
        backend.stop().map_err(|error| backend_failed(&error))?;
        self.status = PlaybackState::Stopped;
        self.current = None;
        self.position_ms = 0;
        // Nothing is loaded, so nobody's session is running any more. Leaving
        // the claim standing would let a surface stop playback a later client
        // started.
        self.initiated_by = None;
        Ok(())
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

    /// Starts what follows a finished track, stepping over entries that
    /// cannot be played.
    ///
    /// One unreadable file must not end a queue that still has music in it —
    /// that is the everyday case of a track deleted, renamed or on a mount
    /// that went away after the queue was built. The GTK controller has
    /// always skipped here (`playback_faults.rs`); moving playback into the
    /// runtime without bringing that rule along would be a silent regression.
    ///
    /// The bound is the number of entries the queues hold, the same rule GTK
    /// uses (`should_stop_skipping`: give every entry one chance, then stop).
    /// Without it `Repeat::All` over a queue of broken files hands the same
    /// entries back for ever and the loop never ends.
    ///
    /// Only *automatic* advancing skips. A user who presses Next gets the
    /// error back instead, because they are waiting for an answer and a
    /// surface can say "that one is gone" — see
    /// `a_failed_skip_stops_the_previous_track_rather_than_leaving_it_audible`.
    fn advance_past_failures(&mut self, backend: &dyn PlaybackBackend, library: &dyn LibraryPort) {
        let attempts = self.up_next.len().saturating_add(self.queue.len());
        // A successful `start` clears the facet, and here the success is
        // exactly what must not erase the skip that led to it: "playing
        // track 3, having stepped over track 2" is the situation, and a
        // surface that never sees it cannot tell the user their track was
        // skipped. So the last failure is carried across the start and put
        // back.
        let mut skipped = None;
        for _ in 0..attempts {
            let Some(next) = self.take_next_auto() else {
                break;
            };
            match self.start(backend, library, next) {
                Ok(()) => {
                    self.failure = skipped;
                    return;
                }
                Err(error) => {
                    skipped = Some(Failure {
                        track_id: Some(next),
                        kind: failure_kind(&error),
                    });
                }
            }
        }
        // Either nothing was left, or every candidate failed. `start` already
        // abandoned the last attempt, so the model is stopped; this makes it
        // so for the "nothing was left" path too, and keeps the reason for a
        // stop that would otherwise look like an exhausted queue.
        let _ = self.stop(backend);
        self.failure = skipped;
    }

    /// Resolves, hands the location to the backend, and adopts it as current.
    ///
    /// **A failure leaves nothing loaded and nothing playing**, which is the
    /// whole point of routing every start through here. The tempting version
    /// returns early and leaves the previous track in `current`: harmless
    /// when a *user* pressed play and gets an error back, and silently wrong
    /// when the caller was the end of the previous track. There the runtime
    /// would keep reporting a track that already finished, at a frozen
    /// position, forever — and because only *changed* facets are published,
    /// no client would ever be told.
    fn start(
        &mut self,
        backend: &dyn PlaybackBackend,
        library: &dyn LibraryPort,
        track_id: i64,
    ) -> Result<(), RuntimeError> {
        let Some(track) = library.resolve(track_id) else {
            // Logged here rather than left to the caller: this is the one
            // failure that carries no backend message, so without a line
            // here a vanished file produces no trace at all.
            tracing::warn!(track_id, "track cannot be resolved to anything playable");
            self.abandon(backend);
            return Err(RuntimeError::Failed(Failed::TrackNotPlayable));
        };
        let started = match &track.location {
            TrackLocation::Path(path) => backend.play(path),
            TrackLocation::Uri(uri) => backend.play_uri(uri),
        };
        if let Err(error) = started {
            self.abandon(backend);
            return Err(backend_failed(&error));
        }
        self.current = Some(track.into());
        self.position_ms = 0;
        self.status = PlaybackState::Playing;
        // The facet describes the situation, not the history: something is
        // playing, so there is no failure left to report.
        self.failure = None;
        // Everything the previous stream still has in flight is stale from
        // here on.
        self.stream = backend.current_generation();
        Ok(())
    }

    /// Puts the model back into the state a failed start actually leaves the
    /// world in: nothing loaded, nothing playing.
    ///
    /// The backend is stopped too, but only when something was actually
    /// loaded. Skipping that would leave the *previous* track still coming
    /// out of the speakers while the runtime reports nothing playing — the
    /// same divergence `stop()` guards against, in the other direction.
    /// Stopping a backend that was already idle, on the other hand, is a
    /// call with nothing to say.
    ///
    /// Same rule as `stop()` applies if that defensive stop itself fails:
    /// `current` is left exactly as it was rather than cleared, so
    /// `is_active()` still reports the pipeline that is presumably still
    /// running. There is no error to hand back here — the caller already has
    /// its own failure to report (the start that provoked this) — but
    /// whatever prompted the abandoned start (a retry, or a plain Stop
    /// command) reaches this same `backend.stop()` call again rather than
    /// finding the model already lying that it succeeded.
    fn abandon(&mut self, backend: &dyn PlaybackBackend) {
        if self.current.is_some() {
            if let Err(error) = backend.stop() {
                tracing::warn!(%error, "backend refused to stop after a failed start");
                return;
            }
        }
        self.current = None;
        self.position_ms = 0;
        self.status = PlaybackState::Stopped;
        self.initiated_by = None;
    }

    /// Adopts a track as current *without* telling the backend to play it —
    /// for the gapless handoff, where the audio is already running.
    fn load(&mut self, backend: &dyn PlaybackBackend, library: &dyn LibraryPort, track_id: i64) {
        match library.resolve(track_id) {
            Some(track) => {
                self.current = Some(track.into());
                self.position_ms = 0;
            }
            // The backend already handed off to a track this side cannot
            // describe. Reporting nothing loaded while `status` stays
            // `Playing` would make `is_active` lie to the idle rule, so the
            // handoff is undone rather than half-adopted.
            None => {
                tracing::warn!(track_id, "gapless handoff to an unresolvable track");
                self.abandon(backend);
            }
        }
    }
}

/// A wire position as an index. Saturating rather than wrapping: an
/// absurd position becomes an out-of-range one, which every queue operation
/// already rejects cleanly.
fn as_index(position: u64) -> usize {
    usize::try_from(position).unwrap_or(usize::MAX)
}

/// The short kind a snapshot carries for a start that did not happen.
///
/// Anything that is not one of the two playback failures would be a bug in
/// the caller — `start` returns only those — so the fallback names the
/// backend rather than inventing a third word for clients to branch on.
fn failure_kind(error: &RuntimeError) -> &'static str {
    match error {
        RuntimeError::Failed(Failed::TrackNotPlayable) => "not_playable",
        _ => "backend",
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
