//! What a user asks the transport for.
//!
//! Split out of `transport.rs` because that file reached the 800-line
//! ceiling the architecture lint enforces. These are the buttons: resume,
//! pause, stop, skip, seek. What they share is that a person pressed them,
//! which is why several of them behave differently from the automatic paths
//! that look superficially the same — a manual Next is not an advance, and a
//! user pressing Stop is not a queue running out.

use reprise_core::playback::{PlaybackBackend, PlaybackState};
use reprise_core::queue::Queue;

use super::{backend_failed, Seek, Source, Transport};
use crate::error::{Rejected, RuntimeError};
use crate::ports::LibraryPort;

impl Transport {
    pub(super) fn resume(
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
                let (track_id, source) = self
                    .current
                    .as_ref()
                    .and_then(|track| track.track_id)
                    .or_else(|| self.queue.current())
                    .map(|track_id| (track_id, Source::Context))
                    // The explicit queue counts too. Stopping consumes the
                    // context but leaves what the user queued by hand, and
                    // a Play that answers "nothing to play" while a track
                    // they queued is sitting right there is a player that
                    // looks broken.
                    .or_else(|| {
                        self.up_next
                            .pop_front()
                            .map(|track_id| (track_id, Source::PlayNext))
                    })
                    .ok_or(RuntimeError::Rejected(Rejected::NothingToPlay))?;
                self.start(backend, library, track_id, source)
            }
        }
    }

    pub(super) fn pause(&mut self, backend: &dyn PlaybackBackend) -> Result<(), RuntimeError> {
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
    /// A user pressing Stop, as opposed to playback ending on its own.
    ///
    /// The context belongs to the playback it was started for, so it goes
    /// with it — otherwise a queue view keeps showing a session the user
    /// ended. What they queued by hand outlives it: those entries were never
    /// part of that context, and QUE-3's "the section contains only the
    /// still-pending future" is as true after a stop as before one.
    ///
    /// Repeat and shuffle are settings rather than part of the context, so
    /// they survive being carried across the replacement.
    ///
    /// Only the command does this. A queue that simply ran out has already
    /// consumed its context, and clearing on that path would also clear it
    /// when a track fails or an episode ends — none of which is a user
    /// saying "stop".
    pub(super) fn stop_hard(&mut self, backend: &dyn PlaybackBackend) -> Result<(), RuntimeError> {
        // Read before stopping: `stop` clears `current`, which would throw
        // away the one fact this decision turns on.
        //
        // A stream or an episode played *beside* the context and never
        // touched it. Stopping one is not the user ending their music
        // session, and clearing the queue here destroys a playlist position
        // they have no way to get back — triggered by an action that has
        // nothing to do with their music.
        let was_music = self
            .current
            .as_ref()
            .is_none_or(|loaded| loaded.source != Source::External);
        self.stop(backend)?;
        // Pressing Stop is a deliberate move on, so the reason the last
        // automatic start did not happen stops describing the situation.
        // Carrying it forward would have a surface explaining a track the
        // user has already left behind.
        self.failure = None;
        // Same reasoning, and one more: a stale `finished` here would tell a
        // podcast surface to mark an episode played that the user stopped
        // halfway through.
        self.stopped_reason = None;
        if !was_music {
            return Ok(());
        }
        let repeat = self.queue.repeat();
        let shuffled = self.queue.is_shuffled();
        self.queue = Queue::new();
        self.queue.set_repeat(repeat);
        if shuffled {
            self.queue.set_shuffle(true);
        }
        Ok(())
    }

    pub(super) fn stop(&mut self, backend: &dyn PlaybackBackend) -> Result<(), RuntimeError> {
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

    pub(super) fn skip_forward(
        &mut self,
        backend: &dyn PlaybackBackend,
        library: &dyn LibraryPort,
    ) -> Result<(), RuntimeError> {
        if self.external_is_loaded() {
            return Ok(());
        }
        // The explicit queue wins over the context: it is what the user
        // asked for most recently and most deliberately.
        let next = self
            .up_next
            .pop_front()
            .map(|track_id| (track_id, Source::PlayNext))
            .or_else(|| {
                self.queue
                    .next_manual()
                    .map(|track_id| (track_id, Source::Context))
            });
        match next {
            Some((track_id, source)) => self.start(backend, library, track_id, source),
            None => self.stop(backend),
        }
    }

    pub(super) fn skip_back(
        &mut self,
        backend: &dyn PlaybackBackend,
        library: &dyn LibraryPort,
    ) -> Result<(), RuntimeError> {
        if self.external_is_loaded() {
            return Ok(());
        }
        // A queued track played *beside* the context, so going back means
        // returning to the entry it interrupted — not stepping the context
        // on top of that, which lands a track further back than the user
        // ever heard. At the head of the context it is the difference
        // between replaying the current entry and Previous doing nothing.
        let interrupted = self
            .current
            .as_ref()
            .is_some_and(|loaded| loaded.source == Source::PlayNext);
        let target = if interrupted {
            self.queue.current()
        } else {
            self.queue.previous()
        };
        match target {
            Some(track_id) => self.start(backend, library, track_id, Source::Context),
            // Nothing before the first track: staying put is the expected
            // behaviour of every player, not an error worth reporting.
            None => Ok(()),
        }
    }

    /// Seeks, either by an offset from where the playhead is or to an
    /// absolute point.
    ///
    /// Both, because they are different intentions and one cannot stand in
    /// for the other. A relative seek is "thirty seconds back" and has to be
    /// resolved against the position at the moment it is applied. Turning a
    /// scrubber drag into a relative seek makes it depend on how long the
    /// message took to arrive: the same drag lands somewhere else under load,
    /// and repeated drags drift.
    ///
    /// A live stream refuses both. It has no length to seek within, and
    /// clamping to a duration of zero would silently turn every seek into a
    /// jump to the start — the worst possible answer to "let me skip the ad".
    ///
    /// The upper clamp applies only when the length is actually known.
    /// `duration_ms == 0` means *unknown*, not *empty*: a downloaded episode
    /// reports zero until the first position report arrives, and clamping a
    /// seek against it collapses every target to the start — the same silent
    /// jump the live refusal exists to prevent, in the one case the refusal
    /// deliberately lets through. The backend knows the real length even
    /// while this side does not, so an unclamped target is answered by the
    /// only party that can answer it.
    pub(super) fn seek(
        &mut self,
        backend: &dyn PlaybackBackend,
        seek: Seek,
    ) -> Result<(), RuntimeError> {
        let Some(loaded) = self.current.as_ref() else {
            return Err(RuntimeError::Rejected(Rejected::NothingToPlay));
        };
        if loaded.live {
            return Err(RuntimeError::Rejected(Rejected::NotSeekable));
        }
        let duration_ms = loaded.duration_ms;
        let aimed = match seek {
            Seek::By(delta_ms) => self.position_ms.saturating_add(delta_ms),
            Seek::To(position_ms) => position_ms,
        };
        let target = if duration_ms > 0 {
            aimed.clamp(0, duration_ms)
        } else {
            // No upper bound to clamp against. The lower one always holds:
            // there is nothing before the start of anything.
            aimed.max(0)
        };
        backend
            .seek_to(target)
            .map_err(|error| backend_failed(&error))?;
        self.position_ms = target;
        Ok(())
    }
}
