//! Windowed and identity-guarded queue operations exported to Android.

use std::collections::HashSet;

use reprise_core::playback::PlaybackBackend;
use reprise_core::queries::{self, QueueItemMetadata};
use reprise_core::queue::QueuePlacement;
use reprise_core::up_next::QueueItem;

use super::{AndroidPlaybackError, AndroidPlaybackSession};
use crate::{TrackRow, TrackWindow, WindowRange};

#[uniffi::export]
impl AndroidPlaybackSession {
    pub fn queue_tracks_next(&self, track_ids: Vec<i64>) -> Result<u32, AndroidPlaybackError> {
        self.enqueue_tracks(track_ids, QueuePlacement::Next)
    }

    pub fn queue_tracks_last(&self, track_ids: Vec<i64>) -> Result<u32, AndroidPlaybackError> {
        self.enqueue_tracks(track_ids, QueuePlacement::Last)
    }

    /// Returns the visible queue tail in play order. While no track is loaded,
    /// the current queue entry is visible too so an explicit enqueue does not
    /// disappear before the user chooses to play it.
    pub fn upcoming_tracks(
        &self,
        window: WindowRange,
    ) -> Result<TrackWindow, AndroidPlaybackError> {
        let offset = usize::try_from(window.offset.max(0)).unwrap_or(usize::MAX);
        let limit = usize::try_from(window.limit.max(0)).unwrap_or(0);
        loop {
            let (ids, total) = {
                let state = self.inner.lock()?;
                let Some(start) = queue_window_start(&state) else {
                    return Ok(TrackWindow {
                        total: 0,
                        rows: Vec::new(),
                        has_more: false,
                    });
                };
                let ids = (start.saturating_add(offset)..state.queue.len())
                    .take(limit)
                    .filter_map(|position| state.queue.id_at_order_position(position))
                    .map(QueueItem::Track)
                    .collect::<Vec<_>>();
                let total = state.queue.len().saturating_sub(start) as i64;
                (ids, total)
            };
            let metadata = {
                let database =
                    self.inner
                        .database
                        .lock()
                        .map_err(|_| AndroidPlaybackError::Backend {
                            detail: "playback queue database was poisoned".to_owned(),
                        })?;
                queries::query_queue_item_window(&database, &ids, 0, window.limit.max(0)).map_err(
                    |error| AndroidPlaybackError::Backend {
                        detail: format!("could not load the playback queue: {error}"),
                    },
                )?
            };
            let resolved = metadata
                .iter()
                .map(QueueItemMetadata::item)
                .collect::<HashSet<_>>();
            let missing = ids
                .iter()
                .filter_map(|item| match item {
                    QueueItem::Track(id) if !resolved.contains(item) => Some(*id),
                    QueueItem::Track(_) | QueueItem::Episode(_) => None,
                })
                .collect::<Vec<_>>();
            if missing.is_empty() {
                let rows = metadata
                    .into_iter()
                    .filter_map(|metadata| match metadata {
                        QueueItemMetadata::Track(track) => Some(TrackRow::from(track)),
                        QueueItemMetadata::Episode(_) => None,
                    })
                    .collect::<Vec<_>>();
                let returned = i64::try_from(rows.len()).unwrap_or(i64::MAX);
                let has_more = window.offset.max(0).saturating_add(returned) < total;
                return Ok(TrackWindow {
                    total,
                    rows,
                    has_more,
                });
            }

            let (next_uri, queue_to_save) = {
                let mut state = self.inner.lock()?;
                if state.current_loaded {
                    state.queue.remove_ids_except_current(&missing);
                } else {
                    state.queue.remove_ids(&missing);
                }
                (state.next_uri(), state.queue.clone())
            };
            self.inner.persist_queue(&queue_to_save)?;
            self.inner.backend()?.set_next(next_uri.as_deref());
        }
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
            let Some(order_position) = upcoming_order_position(&state, position) else {
                return Ok(false);
            };
            if stable_id_at(&state, order_position) != Some(expected_track_id) {
                return Ok(false);
            }
            let already_current = state.queue.current_order_position() == Some(order_position);
            if !already_current
                && state
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
            let Some(from) = upcoming_order_position(&state, from_position) else {
                return Ok(false);
            };
            let Some(to) = upcoming_order_position(&state, to_position) else {
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
            let Some(order_position) = upcoming_order_position(&state, position) else {
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

impl AndroidPlaybackSession {
    fn enqueue_tracks(
        &self,
        requested_ids: Vec<i64>,
        placement: QueuePlacement,
    ) -> Result<u32, AndroidPlaybackError> {
        let (track_ids, uris) = {
            let database =
                self.inner
                    .database
                    .lock()
                    .map_err(|_| AndroidPlaybackError::Backend {
                        detail: "playback queue database was poisoned".to_owned(),
                    })?;
            let mut track_ids = Vec::with_capacity(requested_ids.len());
            let mut uris = Vec::with_capacity(requested_ids.len());
            for track_id in requested_ids {
                let path = queries::track_source_path(&database, track_id).map_err(|error| {
                    AndroidPlaybackError::Backend {
                        detail: format!("could not resolve an enqueued track: {error}"),
                    }
                })?;
                if let Some(path) = path {
                    track_ids.push(track_id);
                    uris.push(path.to_string_lossy().into_owned());
                }
            }
            (track_ids, uris)
        };
        let (taken, next_uri, queue_to_save) = {
            let mut state = self.inner.lock()?;
            state.track_ids.extend_from_slice(&track_ids);
            state.uris.extend(uris);
            state.track_index_by_id = super::index_tracks(&state.track_ids);
            let taken = state.queue.enqueue(&track_ids, placement);
            (taken, state.next_uri(), state.queue.clone())
        };
        self.inner.persist_queue(&queue_to_save)?;
        self.inner.backend()?.set_next(next_uri.as_deref());
        self.inner.notify();
        Ok(u32::try_from(taken).unwrap_or(u32::MAX))
    }
}

fn queue_window_start(state: &super::SessionState) -> Option<usize> {
    state
        .queue
        .current_order_position()
        .map(|position| position.saturating_add(usize::from(state.current_loaded)))
}

fn upcoming_order_position(state: &super::SessionState, position: u64) -> Option<usize> {
    let position = usize::try_from(position).ok()?;
    queue_window_start(state)?.checked_add(position)
}

fn stable_id_at(state: &super::SessionState, order_position: usize) -> Option<i64> {
    state.queue.id_at_order_position(order_position)
}
