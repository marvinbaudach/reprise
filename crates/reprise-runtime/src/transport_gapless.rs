//! Handing the backend the next track before it needs it.
//!
//! Split out of `transport.rs` because that file reached the 800-line
//! ceiling the architecture lint enforces. Pre-feeding is a subject of its
//! own, and a fragile one: it duplicates the advance's choice of what plays
//! next, so the two have to be read together and are best kept where that is
//! obvious.

use reprise_core::library::settings::TrackTransition;
use reprise_core::playback::PlaybackBackend;

use super::{Take, Transport};
use crate::ports::{LibraryPort, TrackLocation};

impl Transport {
    /// What the advance would start, without consuming it — the same
    /// decision, taken by the same function, so the two cannot drift apart.
    fn peek_next_auto(&mut self, library: &dyn LibraryPort) -> Option<i64> {
        // The rejected entries are the advance's business to report, not the
        // pre-feed's: looking at the queue must not put a message in front of
        // a user who has not reached that entry yet.
        let mut ignored = Vec::new();
        self.next_auto(library, Take::Nothing, &mut ignored)
            .map(|(track_id, _)| track_id)
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
        &mut self,
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
