//! Windowed and identity-guarded queue operations exported to Android.

use reprise_core::playback::PlaybackBackend;
use reprise_core::queries::{self, QueueItemMetadata};
use reprise_core::up_next::QueueItem;

use super::{AndroidPlaybackError, AndroidPlaybackSession};
use crate::{TrackRow, TrackWindow, WindowRange};

#[uniffi::export]
impl AndroidPlaybackSession {
    /// Returns only the tracks after the current one, in play order.
    pub fn upcoming_tracks(
        &self,
        window: WindowRange,
    ) -> Result<TrackWindow, AndroidPlaybackError> {
        let (ids, total) = {
            let state = self.inner.lock()?;
            let offset = usize::try_from(window.offset).unwrap_or(usize::MAX);
            let limit = usize::try_from(window.limit).unwrap_or(0);
            let ids = state
                .queue
                .remaining_window(offset, limit)
                .into_iter()
                .map(QueueItem::Track)
                .collect::<Vec<_>>();
            (ids, state.queue.remaining_len() as i64)
        };
        let database = self
            .inner
            .database
            .lock()
            .map_err(|_| AndroidPlaybackError::Backend {
                detail: "playback queue database was poisoned".to_owned(),
            })?;
        let rows = queries::query_queue_item_window(&database, &ids, 0, window.limit)
            .map_err(|error| AndroidPlaybackError::Backend {
                detail: format!("could not load the playback queue: {error}"),
            })?
            .into_iter()
            .filter_map(|metadata| match metadata {
                QueueItemMetadata::Track(track) => Some(TrackRow::from(track)),
                QueueItemMetadata::Episode(_) => None,
            })
            .collect::<Vec<_>>();
        let returned = i64::try_from(rows.len()).unwrap_or(i64::MAX);
        let has_more = window.offset.max(0).saturating_add(returned) < total;
        Ok(TrackWindow {
            total,
            rows,
            has_more,
        })
    }

    /// Promotes one identity-checked row from the future to play immediately.
    /// `position` is zero-based within the upcoming-only list, not Core's
    /// absolute play-order position.
    pub fn play_upcoming_track_now(
        &self,
        position: u64,
        expected_track_id: i64,
    ) -> Result<bool, AndroidPlaybackError> {
        let queue_to_save = {
            let mut state = self.inner.lock()?;
            let Some(order_position) = upcoming_order_position(&state.queue, position) else {
                return Ok(false);
            };
            if stable_id_at(&state, order_position) != Some(expected_track_id) {
                return Ok(false);
            }
            if state
                .queue
                .play_order_position_now(order_position)
                .is_none()
            {
                return Ok(false);
            }
            state.adopt_current();
            state.queue.clone()
        };
        self.inner.persist_queue(&queue_to_save)?;
        self.inner.start_current()?;
        Ok(true)
    }

    /// Moves one upcoming row only while its position still names its identity.
    /// Both positions are zero-based within the upcoming-only list.
    pub fn move_upcoming_track(
        &self,
        from_position: u64,
        expected_track_id: i64,
        to_position: u64,
    ) -> Result<bool, AndroidPlaybackError> {
        let (next_uri, queue_to_save) = {
            let mut state = self.inner.lock()?;
            let Some(from) = upcoming_order_position(&state.queue, from_position) else {
                return Ok(false);
            };
            let Some(to) = upcoming_order_position(&state.queue, to_position) else {
                return Ok(false);
            };
            if stable_id_at(&state, from) != Some(expected_track_id)
                || !state.queue.move_item(from, to)
            {
                return Ok(false);
            }
            (state.next_uri(), state.queue.clone())
        };
        self.inner.persist_queue(&queue_to_save)?;
        self.inner.backend()?.set_next(next_uri.as_deref());
        self.inner.notify();
        Ok(true)
    }

    /// Removes one upcoming row only while its position still names its identity.
    /// `position` is zero-based within the upcoming-only list.
    pub fn remove_upcoming_track(
        &self,
        position: u64,
        expected_track_id: i64,
    ) -> Result<bool, AndroidPlaybackError> {
        let (next_uri, queue_to_save) = {
            let mut state = self.inner.lock()?;
            let Some(order_position) = upcoming_order_position(&state.queue, position) else {
                return Ok(false);
            };
            if stable_id_at(&state, order_position) != Some(expected_track_id)
                || state.queue.remove_order_positions(&[order_position]) != 1
            {
                return Ok(false);
            }
            (state.next_uri(), state.queue.clone())
        };
        self.inner.persist_queue(&queue_to_save)?;
        self.inner.backend()?.set_next(next_uri.as_deref());
        self.inner.notify();
        Ok(true)
    }
}

fn upcoming_order_position(queue: &reprise_core::queue::Queue, position: u64) -> Option<usize> {
    let position = usize::try_from(position).ok()?;
    queue
        .current_order_position()?
        .checked_add(1)?
        .checked_add(position)
}

fn stable_id_at(state: &super::SessionState, order_position: usize) -> Option<i64> {
    state.queue.id_at_order_position(order_position)
}
