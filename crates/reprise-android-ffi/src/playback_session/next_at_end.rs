//! Leaving the current track on purpose, as opposed to the Next gesture.

use super::{AndroidPlaybackError, AndroidPlaybackSession};
use reprise_core::queue::Queue;

/// What a manual move of the playhead came to, decided under the session lock.
pub(super) enum PlayheadMove {
    /// The queue landed on a track, which now starts.
    Landed,
    /// Nothing follows the current track, so playback stops.
    Exhausted,
    /// The move was declined before anything changed; there is nothing to
    /// persist and nothing to stop.
    Refused,
}

impl PlayheadMove {
    /// Advances like [`Queue::next_manual`], stopping when nothing follows.
    pub(super) fn or_stop(queue: &mut Queue) -> Self {
        match queue.next_manual() {
            Some(_) => Self::Landed,
            None => Self::Exhausted,
        }
    }

    /// PLAY-8b: advances like [`Self::or_stop`], but declines at the end.
    ///
    /// The question and the move run under one lock, so a track change from
    /// the stream-event thread cannot slip between them and turn a Next that
    /// had somewhere to go into a stop.
    pub(super) fn unless_at_the_end(queue: &mut Queue) -> Self {
        if queue.has_manual_next() {
            Self::or_stop(queue)
        } else {
            Self::Refused
        }
    }
}

impl AndroidPlaybackSession {
    pub(super) fn move_playhead(
        &self,
        decide_move: impl FnOnce(&mut Queue) -> PlayheadMove,
    ) -> Result<(), AndroidPlaybackError> {
        let (outcome, queue_to_save) = {
            let mut state = self.inner.lock()?;
            let outcome = decide_move(&mut state.queue);
            match outcome {
                PlayheadMove::Refused => return Ok(()),
                PlayheadMove::Landed => state.adopt_current_for_play_intent(),
                PlayheadMove::Exhausted => {}
            }
            (outcome, state.queue.clone())
        };
        self.inner.persist_queue(queue_to_save)?;
        match outcome {
            PlayheadMove::Landed => self.inner.start_current(),
            PlayheadMove::Exhausted => self.inner.stop_backend(),
            PlayheadMove::Refused => Ok(()),
        }
    }
}

#[uniffi::export]
impl AndroidPlaybackSession {
    /// Moves on from the current track, or stops when nothing follows it.
    ///
    /// [`Self::next`] is the user's gesture and does nothing on the last
    /// track with Repeat off (PLAY-8b). A caller that is taking the playing
    /// track away, such as a deletion, still has to leave it, so it asks for
    /// this instead.
    pub fn skip_current_or_stop(&self) -> Result<(), AndroidPlaybackError> {
        if self.inner.forward_from_history()? {
            return Ok(());
        }
        self.move_playhead(PlayheadMove::or_stop)
    }
}
