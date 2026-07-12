//! Queue and current-track metadata restoration that never starts playback.

use std::collections::HashSet;

use reprise_core::media_integration::MprisPlaybackStatus;
use reprise_core::playback::PlaybackState;
use reprise_core::queue::{Queue, QueueSnapshot};

use crate::ui::player_controller::{NowPlaying, PlayerController};

#[allow(dead_code)] // Called through the Task 5 session orchestration.
pub(super) fn restore_should_start_playback() -> bool {
    false
}

impl PlayerController {
    #[allow(dead_code)] // Wired into the close handler in Task 5.
    pub(super) fn session_queue_snapshot(&self) -> QueueSnapshot {
        self.queue.borrow().snapshot()
    }

    #[allow(dead_code)] // Wired into startup restoration in Task 5.
    pub(super) fn restore_session_queue(&self, snapshot: QueueSnapshot) {
        debug_assert!(!restore_should_start_playback());
        let existing = {
            let conn = self.conn.borrow();
            let mut statement = match conn.prepare("SELECT id FROM tracks WHERE missing = 0") {
                Ok(statement) => statement,
                Err(error) => {
                    tracing::warn!(%error, "could not validate restored queue IDs");
                    return;
                }
            };
            let result = match statement.query_map([], |row| row.get::<_, i64>(0)) {
                Ok(rows) => rows.filter_map(Result::ok).collect::<HashSet<_>>(),
                Err(error) => {
                    tracing::warn!(%error, "could not read track IDs for restored queue");
                    return;
                }
            };
            result
        };

        let mut queue = Queue::new();
        if let Err(error) = queue.restore_snapshot(snapshot) {
            tracing::warn!(%error, "invalid session queue ignored");
            return;
        }
        let missing: Vec<_> = queue
            .snapshot()
            .ids
            .into_iter()
            .filter(|id| !existing.contains(id))
            .collect();
        queue.remove_ids(&missing);
        *self.queue.borrow_mut() = queue;
        self.notify_queue_changed();

        let queue_has_tracks = !self.queue.borrow().is_empty();
        let shuffled = self.queue.borrow().is_shuffled();
        let repeat = self.queue.borrow().repeat();
        let current = self.queue.borrow().current();
        self.sync_transport_enabled(queue_has_tracks);
        self.sync_shuffle_indicator(shuffled);
        self.sync_repeat_indicator(repeat);
        self.sync_state(PlaybackState::Stopped);

        let summary = current.and_then(|id| {
            let conn = self.conn.borrow();
            reprise_core::queries::query_track_summary(&conn, id)
                .inspect_err(
                    |error| tracing::warn!(%error, id, "could not restore current track metadata"),
                )
                .ok()
                .flatten()
                .map(|summary| (id, summary))
        });
        match summary {
            Some((id, summary)) => {
                *self.now_playing.borrow_mut() = Some(NowPlaying {
                    id,
                    title: summary.title.clone(),
                    artist: summary.artist.clone(),
                    album: summary.album.clone(),
                    duration_ms: summary.duration_ms,
                    path: summary.path.clone(),
                });
                self.sync_track(&summary.title, &summary.artist, &summary.album);
                self.sync_cover(&summary.path);
            }
            None => {
                *self.now_playing.borrow_mut() = None;
                self.sync_clear_track();
            }
        }
        self.update_mpris_mirror(MprisPlaybackStatus::Stopped);
        tracing::info!(
            queue_len = self.queue.borrow().len(),
            current,
            playback = "Stopped",
            "session queue restored"
        );
    }

    pub(super) fn session_playback_status(&self) -> MprisPlaybackStatus {
        self.mpris_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_restore_never_starts_playback() {
        assert!(!restore_should_start_playback());
    }
}
