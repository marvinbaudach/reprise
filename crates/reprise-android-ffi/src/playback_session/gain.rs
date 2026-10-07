//! The gain a queued track plays at and the stretch of its file it covers,
//! resolved from its track id.
//!
//! The session knows every queue entry by id; the path the backend sees may be
//! a provider URI that is not the path Core stored, so neither is ever looked
//! up from the path — and the tracks a CUE sheet cuts from one file share it.

use crate::playback::AndroidPlaybackItem;
use crate::AndroidPlaybackSegment;

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

    /// The stretch of its file `track_id` plays; `None` for a whole-file
    /// track, and when the library cannot say — the whole file then plays,
    /// which is what a missing row could only ever have meant.
    fn segment_for(&self, track_id: i64) -> Option<AndroidPlaybackSegment> {
        let reader = self.library.reader().ok()?;
        let track = reprise_core::queries::query_present_track_by_id(&reader, track_id).ok()??;
        match crate::track_segment::playback_segment(&reader, &track) {
            Ok(segment) => segment,
            Err(error) => {
                tracing::warn!(track_id, %error, "could not read where a CUE track ends; playing to the end of its file");
                track.segment.map(|segment| AndroidPlaybackSegment {
                    start_ms: segment.start_ms,
                    end_ms: None,
                })
            }
        }
    }

    /// The item the backend plays for `track_id` at `uri`: its own gain and
    /// its own stretch of the file. Reads the database; call it without
    /// holding the state lock.
    pub(super) fn playback_item(&self, track_id: i64, uri: String) -> AndroidPlaybackItem {
        AndroidPlaybackItem {
            track_id: Some(track_id),
            gain_db: self.gain_db_for(track_id),
            segment: self.segment_for(track_id),
            uri,
        }
    }

    /// Pre-feeds `next` to the backend with its own gain and stretch, or clears
    /// the feed. Call it without holding the state lock: it reads the database.
    pub(super) fn feed_next(&self, next: Option<QueuedTrack>) -> Result<(), AndroidPlaybackError> {
        let backend = self.backend()?;
        backend.set_next_item(next.map(|next| self.playback_item(next.track_id, next.uri)));
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
