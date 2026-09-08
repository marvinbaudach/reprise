//! Queue and current-track metadata restoration that never starts playback.

use std::collections::HashSet;

use reprise_core::media_integration::MprisPlaybackStatus;
use reprise_core::playback::PlaybackState;
use reprise_core::queue::{Queue, QueueSnapshot};
use reprise_core::up_next::{QueueItem, UpNextQueue};

use crate::ui::player_controller::{NowPlaying, PlayerController};

pub(in crate::ui) enum StoppedPlayTarget {
    Greeting(Vec<i64>),
    Item(QueueItem),
}

impl StoppedPlayTarget {
    pub(in crate::ui) fn item(&self) -> Option<QueueItem> {
        match self {
            Self::Greeting(ids) => ids.first().copied().map(QueueItem::Track),
            Self::Item(item) => Some(*item),
        }
    }
}

fn validated_up_next(
    mut up_next: UpNextQueue,
    current: Option<QueueItem>,
    existing_tracks: &HashSet<i64>,
    existing_episodes: &HashSet<i64>,
) -> (UpNextQueue, Option<QueueItem>) {
    let missing: Vec<_> = up_next
        .ids()
        .iter()
        .copied()
        .filter(|item| match item {
            QueueItem::Track(id) => !existing_tracks.contains(id),
            QueueItem::Episode(id) => !existing_episodes.contains(id),
        })
        .collect();
    up_next.remove_ids(&missing);
    let current = current.filter(|item| match item {
        QueueItem::Track(id) => existing_tracks.contains(id),
        QueueItem::Episode(id) => existing_episodes.contains(id),
    });
    (up_next, current)
}

impl PlayerController {
    pub(in crate::ui) fn session_queue_snapshot(&self) -> QueueSnapshot {
        self.queue.borrow().snapshot()
    }

    pub(in crate::ui) fn session_up_next_snapshot(&self) -> (UpNextQueue, Option<QueueItem>) {
        (self.up_next.borrow().clone(), self.current_up_next.get())
    }

    pub(in crate::ui) fn restore_session_queue(
        &self,
        snapshot: QueueSnapshot,
        up_next: UpNextQueue,
        current_up_next: Option<QueueItem>,
        play_origin: Option<super::play_origin::PlayOrigin>,
    ) {
        let retained = {
            let conn = &self.conn;
            match reprise_core::queries::query_queue_retained_track_ids(conn) {
                Ok(ids) => ids,
                Err(error) => {
                    tracing::warn!(%error, "could not validate restored queue IDs");
                    return;
                }
            }
        };
        let retained_episodes = match reprise_core::queries::query_available_episode_ids(&self.conn)
        {
            Ok(ids) => ids,
            Err(error) => {
                tracing::warn!(%error, "could not validate restored episode queue entries");
                return;
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
        let (up_next, current_up_next) =
            validated_up_next(up_next, current_up_next, &retained, &retained_episodes);
        *self.queue.borrow_mut() = queue;
        *self.up_next.borrow_mut() = up_next;
        self.current_up_next.set(current_up_next);
        // Restored alongside the snapshot it describes; a session without a
        // restorable queue never reaches this line, so a stale origin can't
        // outlive its context.
        *self.play_origin.borrow_mut() = play_origin;
        *self.pending_random_start.borrow_mut() = None;
        if current_up_next.is_none() {
            let random_ids = (self.random_start_chooser.borrow_mut())(&self.conn);
            match random_ids {
                Ok(ids) if ids.is_empty() => {
                    self.library_has_tracks.set(false);
                }
                Ok(ids) => {
                    self.library_has_tracks.set(true);
                    *self.pending_random_start.borrow_mut() = Some(ids);
                }
                Err(error) => {
                    tracing::warn!(%error, "could not build random startup playback snapshot");
                }
            }
        }
        self.notify_queue_changed();

        let greeting_track = self.pending_random_start_track_id().map(QueueItem::Track);
        let queue_has_tracks = self.has_playable_item();
        let shuffled = self.queue.borrow().is_shuffled();
        let repeat = self.queue.borrow().repeat();
        let current = current_up_next
            .or(greeting_track)
            .or_else(|| self.queue.borrow().current().map(QueueItem::Track));
        // START-4 places a greeting through the track list exactly like any
        // restored item. Keep that placement reflected here even though
        // greeting Play bypasses this one-shot and reaches `play_track_id` as
        // `PlaybackStarted`, which NAV-10b already maps to `MarkerOnly`.
        self.restored_placement_intact.set(current.is_some());
        self.sync_transport_enabled(queue_has_tracks);
        self.sync_shuffle_indicator(shuffled);
        self.sync_repeat_indicator(repeat);
        self.sync_state(PlaybackState::Stopped);

        self.sync_stopped_item(current);
        tracing::info!(
            queue_len = self.queue.borrow().len(),
            up_next_len = self.up_next.borrow().len(),
            ?current_up_next,
            ?current,
            greeting_track_id = ?self.pending_random_start_track_id(),
            playback = "Stopped",
            "session queue restored"
        );
    }

    fn sync_stopped_item(&self, current: Option<QueueItem>) {
        let summary = current.and_then(|item| {
            let id = item.track_id()?;
            let conn = &self.conn;
            reprise_core::queries::query_track_summary(conn, id)
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
                self.sync_position(0, summary.duration_ms);
                // The restored track is a known track — the Lyrics tab keys
                // off the same metadata the bar already shows, so it must not
                // wait for playback to start. LYR-2 still holds: the fetch
                // only leaves the machine when that tab is open.
                self.sync_lyrics_track(Some(crate::ui::player_lyrics::lyrics_query_for(&summary)));
            }
            None => {
                *self.now_playing.borrow_mut() = None;
                self.sync_clear_track();
            }
        }
        self.update_mpris_mirror(MprisPlaybackStatus::Stopped);
    }

    pub(in crate::ui) fn dismiss_random_start_greeting(&self) -> bool {
        let dismissed = self.pending_random_start.borrow_mut().take().is_some();
        if !dismissed {
            return false;
        }
        let current = self
            .current_up_next
            .get()
            .or_else(|| self.queue.borrow().current().map(QueueItem::Track));
        self.restored_placement_intact.set(current.is_some());
        self.sync_stopped_item(current);
        self.sync_transport_enabled(self.has_playable_item());
        // Drop the greeting marker before restoring the queue's real current
        // item. A stopped notification clears the marker when there is no
        // current item to replace it with.
        self.notify_playback_state_changed(PlaybackState::Stopped);
        self.notify_current_track(
            crate::ui::current_track_selection::CurrentTrackChange::SessionRestore,
        );
        dismissed
    }

    pub(in crate::ui) fn session_playback_status(&self) -> MprisPlaybackStatus {
        self.mpris_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status
    }

    pub(in crate::ui) fn pending_random_start_track_id(&self) -> Option<i64> {
        self.pending_random_start
            .borrow()
            .as_ref()
            .and_then(|ids| ids.first().copied())
    }

    /// The item the stopped bar, marker and seek-start paths present. This
    /// non-consuming projection keeps those surfaces on the same decision as
    /// [`Self::stopped_play_target`] without cloning a greeting's complete queue.
    pub(in crate::ui) fn stopped_play_target_item(&self) -> Option<QueueItem> {
        self.pending_random_start_track_id()
            .map(QueueItem::Track)
            .or_else(|| self.current_up_next.get())
            .or_else(|| self.queue.borrow().current().map(QueueItem::Track))
    }

    pub(in crate::ui) fn stopped_play_target(&self) -> Option<StoppedPlayTarget> {
        let greeting = self.pending_random_start.borrow().clone();
        greeting
            .map(StoppedPlayTarget::Greeting)
            .or_else(|| self.stopped_play_target_item().map(StoppedPlayTarget::Item))
    }

    pub(in crate::ui) fn start_stopped_play_target(
        self: &std::rc::Rc<Self>,
        target: StoppedPlayTarget,
        change: crate::ui::current_track_selection::CurrentTrackChange,
    ) {
        match target {
            StoppedPlayTarget::Greeting(ids) => {
                self.play_from_view(ids, 0, super::play_origin::PlayOrigin::library());
            }
            StoppedPlayTarget::Item(item) => self.start_current_item(item, change),
        }
    }

    pub(in crate::ui) fn has_playable_item(&self) -> bool {
        self.pending_random_start
            .borrow()
            .as_ref()
            .is_some_and(|ids| !ids.is_empty())
            || self.current_up_next.get().is_some()
            || !self.queue.borrow().is_empty()
            || !self.up_next.borrow().is_empty()
    }

    #[cfg(test)]
    pub(in crate::ui) fn set_random_start_chooser_for_test(
        &self,
        chooser: impl FnMut(&reprise_core::db::Db) -> Result<Vec<i64>, rusqlite::Error> + 'static,
    ) {
        *self.random_start_chooser.borrow_mut() = Box::new(chooser);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_pending_and_current_ids_are_validated_together() {
        let existing = HashSet::from([1, 3]);
        let mut pending = UpNextQueue::default();
        pending.append(&[
            QueueItem::Track(1),
            QueueItem::Track(2),
            QueueItem::Track(3),
            QueueItem::Track(2),
        ]);
        let episodes = HashSet::new();
        let (pending, current) =
            validated_up_next(pending, Some(QueueItem::Track(2)), &existing, &episodes);
        assert_eq!(pending.ids(), &[1, 3]);
        assert_eq!(current, None);

        let (_, current) = validated_up_next(
            UpNextQueue::default(),
            Some(QueueItem::Track(3)),
            &existing,
            &episodes,
        );
        assert_eq!(current, Some(QueueItem::Track(3)));
    }

    #[test]
    fn que_12_restored_manual_queue_cannot_seed_episode_items() {
        let tracks = HashSet::from([1]);
        let episodes = HashSet::from([7]);
        let mut pending = UpNextQueue::default();
        assert_eq!(
            pending.append(&[
                reprise_core::up_next::QueueItem::Track(1),
                reprise_core::up_next::QueueItem::Episode(7),
                reprise_core::up_next::QueueItem::Episode(8),
            ]),
            1
        );

        let (pending, current) = validated_up_next(pending, None, &tracks, &episodes);

        assert_eq!(pending.ids(), &[reprise_core::up_next::QueueItem::Track(1)]);
        assert_eq!(current, None);
    }
}
