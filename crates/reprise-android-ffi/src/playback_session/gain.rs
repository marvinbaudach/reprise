//! The gain a queued track plays at, resolved from its track id.
//!
//! The session knows every queue entry by id; the path the backend sees may be
//! a provider URI that is not the path Core stored, so a gain is never looked
//! up from the path.

use reprise_core::playback::{PlaybackBackend, PlaybackItem};

use super::{AndroidPlaybackError, SessionInner};

/// A queue entry as the backend needs to be told about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct QueuedTrack {
    pub(super) track_id: i64,
    pub(super) uri: String,
}

impl SessionInner {
    /// The gain of `track_id` in the current ReplayGain mode. Unity when the
    /// library cannot be read; an id with no track row logs and resolves to
    /// unity inside `effective_gain_db`.
    pub(super) fn gain_db_for(&self, track_id: i64) -> f64 {
        let reader = match self.library.reader() {
            Ok(reader) => reader,
            Err(error) => {
                tracing::warn!(track_id, %error, "could not read the library for a track gain; using unity gain");
                return 0.0;
            }
        };
        let mode = reprise_core::library::settings::get_replay_gain_mode(&reader);
        reprise_core::queries::effective_gain_db(&reader, track_id, mode)
    }

    /// Pre-feeds `next` to the backend with its own gain, or clears the feed.
    /// Call it without holding the state lock: it reads the database.
    pub(super) fn feed_next(&self, next: Option<QueuedTrack>) -> Result<(), AndroidPlaybackError> {
        let backend = self.backend()?;
        let resolved = next.map(|next| (self.gain_db_for(next.track_id), next.uri));
        backend.set_next(resolved.as_ref().map(|(gain_db, uri)| PlaybackItem {
            segment: None,
            path: uri,
            gain_db: *gain_db,
        }));
        Ok(())
    }

    /// Re-resolves the gain of the playing track and of the pre-fed one, after
    /// something it depends on (the ReplayGain mode) changed, and hands both to
    /// the backend without restarting either track. Nothing is playing, nothing
    /// to do.
    pub(super) fn refresh_gains(&self) -> Result<(), AndroidPlaybackError> {
        let backend = self.backend()?;
        let (current, next) = {
            let state = self.lock()?;
            let current = state
                .current_loaded
                .then(|| state.current_track_id())
                .flatten();
            (current, state.next_track())
        };
        let Some(current) = current else {
            return Ok(());
        };
        let current_gain_db = self.gain_db_for(current);
        let next_gain_db = next.map(|next| self.gain_db_for(next.track_id));
        backend
            .set_gains(current_gain_db, next_gain_db)
            .map_err(|error| AndroidPlaybackError::Backend {
                detail: error.to_string(),
            })
    }
}
