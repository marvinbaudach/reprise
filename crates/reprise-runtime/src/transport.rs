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
use reprise_runtime_protocol::queue::QueueSnapshot;

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
/// A library track and a radio stream differ in the ways the rest of this
/// module cares about, and each of those is a field: one has a library id and
/// one does not, one advances the queue when it ends while the other simply
/// stops, one is named by that id and the other by whatever the surface calls
/// it, and one has an end to seek within.
struct Loaded {
    /// Absent for anything without a library id — a stream, an episode, a
    /// preview render. A client must never invent one.
    track_id: Option<i64>,
    title: String,
    artist: String,
    album: String,
    duration_ms: i64,
    /// Where this came from.
    ///
    /// Two questions hang on it, and a bool could only answer one. Whether
    /// the end of this hands back to the queue — external media must not
    /// start the music queued behind it — and where Previous goes, which
    /// differs for a track that jumped the line.
    source: Source,
    /// What the surface calls this, for anything without a library id.
    /// Opaque here — the runtime carries it and never reads it.
    external_ref: Option<String>,
    /// Whether this has no end. Only a stream sets it.
    live: bool,
}

/// What supplied the loaded item.
///
/// The distinction the GTK controller has always drawn and the runtime did
/// not: an explicitly queued track plays *beside* the context rather than
/// inside it, so the context cursor stays where it was and going back means
/// going back to the entry that was interrupted.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    /// The surrounding context the user started from.
    Context,
    /// The explicit "play next" queue.
    PlayNext,
    /// Not a library track at all — a stream, an episode, a preview render.
    External,
}

impl From<PlayableTrack> for Loaded {
    fn from(track: PlayableTrack) -> Self {
        Self {
            track_id: Some(track.track_id),
            title: track.title,
            artist: track.artist,
            album: track.album,
            duration_ms: track.duration_ms,
            // `start` corrects this for a track that jumped the line; the
            // context is the ordinary case and the safe default.
            source: Source::Context,
            // A library track is named by its id and has an end. Both of
            // these describe the items that have neither.
            external_ref: None,
            live: false,
        }
    }
}

/// Whether looking at what comes next also removes it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Take {
    /// The advance: the entry is consumed.
    Entry,
    /// The pre-feed: the queue is left exactly as it was.
    Nothing,
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
    /// Why playback is stopped, when the runtime knows something `status`
    /// alone does not say.
    ///
    /// Set only where playback ended *by itself*: the loaded item played to
    /// its end. A user's Stop clears it, because a client that just issued
    /// Stop needs no explanation and a client that did not must not be told
    /// the content ran out when it did not. A failure clears it too — the
    /// failure facet is the fuller answer, and two facets naming the same
    /// stop is two chances to disagree about it.
    stopped_reason: Option<&'static str>,
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
            stopped_reason: None,
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
            external_ref: track.and_then(|track| track.external_ref.clone()),
            live: track.is_some_and(|track| track.live),
            stopped_reason: self.stopped_reason.map(Into::into),
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
        let started = self.start(backend, library, track_id, Source::Context);
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
            source: Source::External,
            // Empty means the surface offered no identity. Keeping that as
            // `None` rather than `Some("")` leaves one case where there would
            // otherwise be two, and spares every reader the same check.
            external_ref: Some(media.external_ref.clone()).filter(|it| !it.is_empty()),
            live: media.live,
        });
        self.position_ms = 0;
        self.status = PlaybackState::Playing;
        self.initiated_by = initiated_by;
        self.failure = None;
        self.stopped_reason = None;
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
            PlaybackCommand::Stop => self.stop_hard(backend),
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
                if self.external_is_loaded() {
                    let _ = self.stop(backend);
                    // The episode ran to its end. A surface acts on that —
                    // marks it played, offers the next one — and must not act
                    // on a stop the user asked for, which looks identical
                    // from the status alone.
                    self.stopped_reason = Some("finished");
                } else {
                    self.advance_past_failures(backend, library);
                }
            }
            PlayerEvent::AdvancedToNext => {
                // The backend handed off to a pre-fed track without a
                // restart, so the audio is already rolling: advance the model
                // by one and do NOT call play.
                //
                // The same choice the pre-feed made, by the same function:
                // the backend is already playing what it was handed, so
                // picking anything else here abandons audio that is rolling.
                if self.external_is_loaded() {
                    // Unreachable while the pre-feed holds its own end of
                    // this — it arms nothing during external playback, and a
                    // backend only hands off to something it was armed with.
                    // Kept because the alternative is adopting a library
                    // track as what is playing while an episode is what is
                    // audible, and because GTK refuses this handoff in the
                    // same words (`player_event_handling.rs`).
                    tracing::warn!("ignoring a gapless handoff during external playback");
                    return;
                }
                let mut stepped_over = Vec::new();
                if let Some((next, source)) = self.take_next_auto(library, &mut stepped_over) {
                    self.load(backend, library, next, source);
                    self.failure = unplayable(&stepped_over);
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
                // The failure facet is the fuller answer; leaving a stale
                // `finished` beside it would have the two contradict each
                // other about the same stop.
                self.stopped_reason = None;
                let _ = self.stop(backend);
            }
        }
    }

    /// Whether what is loaded is not a library track.
    ///
    /// Next and Previous do nothing at all in that case, which is what the
    /// GTK controller does: both are gated on being in queue mode
    /// (`queue_transport.rs`) and return without so much as a toast. Falling
    /// through into the context instead swaps a live stream for a library
    /// track the user never asked for, and consumes a queued entry on a
    /// press that was meant for the stream.
    pub(super) fn external_is_loaded(&self) -> bool {
        self.current
            .as_ref()
            .is_some_and(|loaded| loaded.source == Source::External)
    }

    /// What an automatic advance plays next, and where it came from.
    ///
    /// One function for two callers on purpose. The advance consumes what it
    /// finds; the pre-feed only looks. They have to agree, and two functions
    /// with "keep these in step" in a comment is exactly what stopped being
    /// true: a pre-feed that names a different track than the advance will
    /// take promises the backend a handoff the runtime then abandons — and
    /// abandoning calls `stop` on a pipeline that is by then already playing
    /// the pre-fed track, cutting off audio mid-segue.
    ///
    /// Availability is filtered here rather than left to the retry loop in
    /// `advance_past_failures`, because the pre-feed has no retry loop: it
    /// gets one guess and the gapless handoff acts on it.
    ///
    /// Repeat-one is about the thing that is playing, not about which queue
    /// supplied it. Asking the explicit queue first turns "repeat this" into
    /// "play the next queued track" — the opposite instruction — and eats an
    /// entry the user lined up. Asking the context first repeats the entry a
    /// queued track jumped in front of, which is not what the user asked to
    /// hear again either.
    /// `stepped_over` collects every entry the filter rejected. Skipping
    /// silently would land on the right track and leave the user wondering
    /// where the one they queued went; the caller turns the last of these
    /// into the reason the snapshot reports.
    fn next_auto(
        &mut self,
        library: &dyn LibraryPort,
        take: Take,
        stepped_over: &mut Vec<i64>,
    ) -> Option<(i64, Source)> {
        let mut is_available = |track_id: i64| {
            let playable = library.resolve(track_id).is_some();
            if !playable {
                stepped_over.push(track_id);
            }
            playable
        };
        if self.queue.repeat() == Repeat::One {
            if let Some((track_id, source)) = self
                .current
                .as_ref()
                .and_then(|loaded| Some((loaded.track_id?, loaded.source)))
            {
                if is_available(track_id) {
                    return Some((track_id, source));
                }
            }
        }
        let queued = match take {
            Take::Entry => self.up_next.take_first_matching(&mut is_available),
            Take::Nothing => self.up_next.first_matching(&mut is_available),
        };
        if let Some(track_id) = queued {
            return Some((track_id, Source::PlayNext));
        }
        match take {
            Take::Entry => self.queue.advance_auto_matching(&mut is_available),
            Take::Nothing => self.queue.peek_auto_matching(&mut is_available),
        }
        .map(|track_id| (track_id, Source::Context))
    }

    /// The advancing form of [`Self::next_auto`]: consumes what it returns.
    fn take_next_auto(
        &mut self,
        library: &dyn LibraryPort,
        stepped_over: &mut Vec<i64>,
    ) -> Option<(i64, Source)> {
        self.next_auto(library, Take::Entry, stepped_over)
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
        // Entries the availability filter rejected before they were ever
        // attempted. They are skips just as much as a failed start is, and a
        // user whose queued track silently never plays is owed the same
        // answer either way.
        let mut stepped_over = Vec::new();
        for _ in 0..attempts {
            let Some((next, source)) = self.take_next_auto(library, &mut stepped_over) else {
                break;
            };
            match self.start(backend, library, next, source) {
                Ok(()) => {
                    self.failure = skipped.or_else(|| unplayable(&stepped_over));
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
        self.failure = skipped.or_else(|| unplayable(&stepped_over));
        // Only when nothing went wrong: a queue that ran out ended by itself
        // and a surface may offer to extend it, but a queue that gave up on
        // three unreadable files did not, and the failure facet is the answer
        // there. Both leave `Stopped` with nothing loaded, which is exactly
        // why the difference has to be said rather than inferred.
        if self.failure.is_none() {
            self.stopped_reason = Some("finished");
        }
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
        source: Source,
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
        self.current = Some(Loaded {
            source,
            ..track.into()
        });
        self.position_ms = 0;
        self.status = PlaybackState::Playing;
        // The facet describes the situation, not the history: something is
        // playing, so there is no failure and no stop left to explain.
        self.failure = None;
        self.stopped_reason = None;
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
    fn load(
        &mut self,
        backend: &dyn PlaybackBackend,
        library: &dyn LibraryPort,
        track_id: i64,
        source: Source,
    ) {
        match library.resolve(track_id) {
            Some(track) => {
                self.current = Some(Loaded {
                    source,
                    ..track.into()
                });
                self.position_ms = 0;
                // Something is playing again, so no stop is left to explain.
                // `start` says this next to its own `failure = None`; this is
                // the other way a track becomes current and it owes the same.
                //
                // Not reachable through the GStreamer backend, which clears
                // its pre-fed slot on every stop and every restart, so a real
                // handoff always follows a `start` that has already cleared
                // this. That is a promise made in another crate about a trait
                // this file only sees the near side of — the invariant is
                // documented here, so it is held here.
                self.stopped_reason = None;
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

/// The last entry an advance stepped over, as the failure a snapshot
/// reports. The *last* rather than the first: a surface shows one message,
/// and the most recent skip is the one closest to what is playing now.
fn unplayable(stepped_over: &[i64]) -> Option<Failure> {
    stepped_over.last().map(|&track_id| Failure {
        track_id: Some(track_id),
        kind: "not_playable",
    })
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

#[path = "transport_queue.rs"]
mod queue_editing;

#[path = "transport_gapless.rs"]
mod gapless;

#[path = "transport_session.rs"]
mod session;

#[path = "transport_controls.rs"]
mod controls;
