//! Queue and current-track metadata restoration that never starts playback.

use std::collections::HashSet;

use reprise_core::media_integration::MprisPlaybackStatus;
use reprise_core::playback::PlaybackState;
use reprise_core::queue::{Queue, QueueSnapshot};
use reprise_core::up_next::UpNextQueue;

use crate::ui::player_controller::{NowPlaying, PlayerController};

fn validated_up_next(
    mut up_next: UpNextQueue,
    current: Option<i64>,
    existing: &HashSet<i64>,
) -> (UpNextQueue, Option<i64>) {
    let missing: Vec<_> = up_next
        .ids()
        .iter()
        .copied()
        .filter(|id| !existing.contains(id))
        .collect();
    up_next.remove_ids(&missing);
    (up_next, current.filter(|id| existing.contains(id)))
}

#[allow(dead_code)] // Called through the Task 5 session orchestration.
pub(in crate::ui) fn restore_should_start_playback() -> bool {
    false
}

impl PlayerController {
    #[allow(dead_code)] // Wired into the close handler in Task 5.
    pub(in crate::ui) fn session_queue_snapshot(&self) -> QueueSnapshot {
        self.queue.borrow().snapshot()
    }

    pub(in crate::ui) fn session_up_next_snapshot(&self) -> (UpNextQueue, Option<i64>) {
        (self.up_next.borrow().clone(), self.current_up_next.get())
    }

    #[allow(dead_code)] // Wired into startup restoration in Task 5.
    pub(in crate::ui) fn restore_session_queue(
        &self,
        snapshot: QueueSnapshot,
        up_next: UpNextQueue,
        current_up_next: Option<i64>,
        play_origin: Option<super::play_origin::PlayOrigin>,
    ) {
        debug_assert!(!restore_should_start_playback());
        let retained = {
            let conn = self.conn.borrow();
            match reprise_core::queries::query_queue_retained_track_ids(&conn) {
                Ok(ids) => ids,
                Err(error) => {
                    tracing::warn!(%error, "could not validate restored queue IDs");
                    return;
                }
            }
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
            .filter(|id| !retained.contains(id))
            .collect();
        queue.remove_ids(&missing);
        let (up_next, current_up_next) = validated_up_next(up_next, current_up_next, &retained);
        *self.queue.borrow_mut() = queue;
        *self.up_next.borrow_mut() = up_next;
        self.current_up_next.set(current_up_next);
        // Restored alongside the snapshot it describes; a session without a
        // restorable queue never reaches this line, so a stale origin can't
        // outlive its context.
        *self.play_origin.borrow_mut() = play_origin;
        self.notify_queue_changed();

        let queue_has_tracks = !self.queue.borrow().is_empty()
            || !self.up_next.borrow().is_empty()
            || current_up_next.is_some();
        let shuffled = self.queue.borrow().is_shuffled();
        let repeat = self.queue.borrow().repeat();
        let current = current_up_next.or_else(|| self.queue.borrow().current());
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
                    album_artist: summary.album_artist.clone(),
                    genre: summary.genre.clone(),
                    artist_mbid: summary.artist_mbid.clone(),
                    art_url: None,
                    duration_ms: summary.duration_ms,
                    path: summary.path.clone(),
                });
                self.sync_track(
                    &summary.title,
                    &summary.artist,
                    &summary.album,
                    summary.year,
                );
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
            up_next_len = self.up_next.borrow().len(),
            current_up_next,
            current,
            playback = "Stopped",
            "session queue restored"
        );
    }

    pub(in crate::ui) fn session_playback_status(&self) -> MprisPlaybackStatus {
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

    #[test]
    fn restored_pending_and_current_ids_are_validated_together() {
        let existing = HashSet::from([1, 3]);
        let mut pending = UpNextQueue::default();
        pending.append(&[1, 2, 3, 2]);
        let (pending, current) = validated_up_next(pending, Some(2), &existing);
        assert_eq!(pending.ids(), &[1, 3]);
        assert_eq!(current, None);

        let (_, current) = validated_up_next(UpNextQueue::default(), Some(3), &existing);
        assert_eq!(current, Some(3));
    }
}
