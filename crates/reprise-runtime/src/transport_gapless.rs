//! Handing the backend the next track before it needs it.
//!
//! Split out of `transport.rs` because that file reached the 800-line
//! ceiling the architecture lint enforces. Pre-feeding is a subject of its
//! own, and a fragile one: it duplicates the advance's choice of what plays
//! next, so the two have to be read together and are best kept where that is
//! obvious.

use reprise_core::library::settings::TrackTransition;
use reprise_core::playback::PlaybackBackend;
use reprise_core::queue::Repeat;

use super::Transport;
use crate::ports::{LibraryPort, TrackLocation};

impl Transport {
    /// What an automatic advance would start, without consuming anything.
    ///
    /// Deliberately the same shape as [`Self::take_next_auto`]: a pre-feed
    /// that disagrees with the advance hands the backend a track it will
    /// then have to abandon, which is worse than the gap it was meant to
    /// avoid. Any change to one belongs in the other.
    fn peek_next_auto(&self, library: &dyn LibraryPort) -> Option<i64> {
        let is_available = |track_id: i64| library.resolve(track_id).is_some();
        if self.queue.repeat() == Repeat::One {
            if let Some(track_id) = self.current.as_ref().and_then(|loaded| loaded.track_id) {
                return is_available(track_id).then_some(track_id);
            }
        }
        if let Some(&track_id) = self.up_next.ids().first() {
            if is_available(track_id) {
                return Some(track_id);
            }
        }
        self.queue.peek_auto_matching(is_available)
    }

    /// Tells the backend what to hand off to when the current track ends.
    ///
    /// Called after anything that can change the answer — a start, a queue
    /// edit, a repeat or shuffle toggle, a skip — because `set_next`'s
    /// contract is last-write-wins: a value left standing after the upcoming
    /// track changed is a handoff to a track the user has displaced.
    ///
    /// Unconditional rather than deduplicated, exactly as the GTK controller
    /// does it. A backend that receives the same path twice does nothing;
    /// a runtime that tracks what it last fed has one more piece of state to
    /// get wrong.
    ///
    /// Only a local path is pre-fed. `set_next` takes a path, and a backend
    /// has a separate entry point for remote media — feeding a URI through
    /// this one would be a handoff that fails at the moment it is needed.
    pub(crate) fn refresh_pre_feed(
        &self,
        backend: &dyn PlaybackBackend,
        library: &dyn LibraryPort,
        transition: TrackTransition,
    ) {
        if transition == TrackTransition::Off {
            backend.set_next(None);
            return;
        }
        let path = self
            .peek_next_auto(library)
            .and_then(|track_id| library.resolve(track_id))
            .and_then(|track| match track.location {
                TrackLocation::Path(path) => Some(path),
                TrackLocation::Uri(_) => None,
            });
        backend.set_next(path.as_deref());
    }
}
