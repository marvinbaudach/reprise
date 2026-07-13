//! Local play-count and optional ListenBrainz completion paths extracted from
//! the edge-tight `player_controller` module.

use reprise_core::library::stats;
use reprise_core::scrobbling::{self, TrackMetadata};

use crate::ui::player_controller::PlayerController;

impl PlayerController {
    pub(super) fn begin_scrobble(&self, track: TrackMetadata) {
        if !self.listenbrainz.is_active() {
            return;
        }
        if let Err(error) = track.validate() {
            tracing::debug!(%error, "track metadata cannot be scrobbled");
            return;
        }
        self.scrobble_session
            .borrow_mut()
            .begin(track.clone(), now_unix());
        self.listenbrainz.playing_now(track);
    }

    /// Finishes both per-track accounting paths exactly once. The existing
    /// `current_track.take()` remains the idempotency guard for local counts;
    /// `ScrobbleSession::finish` independently takes its pending listen.
    pub(super) fn evaluate_play_tracking(&self) {
        let Some((track_id, duration_ms)) = self.current_track.take() else {
            return;
        };
        let max_position_ms = self.max_position_ms.replace(0);

        let scrobble = self
            .scrobble_session
            .borrow_mut()
            .finish(max_position_ms, self.listenbrainz.is_active());
        if let Some(listen) = scrobble {
            let queued = {
                let conn = self.conn.borrow();
                scrobbling::enqueue(&conn, &listen)
            };
            match queued {
                Ok(queue_id) => {
                    tracing::debug!(queue_id, "ListenBrainz listen queued");
                    self.listenbrainz.flush();
                }
                Err(error) => tracing::warn!(%error, "could not queue ListenBrainz listen"),
            }
        }

        if !stats::should_count_play(max_position_ms, duration_ms) {
            return;
        }
        let result = {
            let conn = self.conn.borrow();
            stats::record_play(&conn, track_id, now_unix())
        };
        match result {
            Ok(()) => {
                tracing::debug!(track_id, max_position_ms, duration_ms, "play recorded");
            }
            Err(error) => {
                tracing::error!(%error, track_id, "failed to record play");
            }
        }
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}
