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
    /// Replaces the queue from stable track identities and starts the selected
    /// item after resolving every path from the live database.
    pub fn play_track_ids(
        &self,
        track_ids: Vec<i64>,
        start_index: u64,
    ) -> Result<(), AndroidPlaybackError> {
        let start_index =
            usize::try_from(start_index).map_err(|_| AndroidPlaybackError::InvalidRequest {
                detail: "the tapped track index does not fit this device".to_owned(),
            })?;
        if start_index >= track_ids.len() {
            return Err(AndroidPlaybackError::InvalidRequest {
                detail: "the tapped track is outside the requested list".to_owned(),
            });
        }
        let resolved = self.resolve_track_uris(track_ids, "could not resolve a played track")?;
        let resolved_start = resolved
            .iter()
            .position(|(requested_index, _, _)| *requested_index == start_index)
            .ok_or(AndroidPlaybackError::InvalidRequest {
                detail: "the tapped track is no longer in the library".to_owned(),
            })?;
        self.play_tracks(
            resolved.iter().map(|(_, track_id, _)| *track_id).collect(),
            resolved.into_iter().map(|(_, _, uri)| uri).collect(),
            u64::try_from(resolved_start).unwrap_or(u64::MAX),
        )
    }

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
        let limit = usize::try_from(window.limit.max(0)).unwrap_or(0);
        loop {
            let (ids, total, has_more) = {
                let state = self.inner.lock()?;
                let Some(upcoming_start) = queue_window_start(&state) else {
                    return Ok(TrackWindow {
                        total: 0,
                        rows: Vec::new(),
                        has_more: false,
                    });
                };
                let range =
                    signed_queue_window(upcoming_start, state.queue.len(), window.offset, limit);
                let has_more = range.end < state.queue.len();
                let ids = range
                    .filter_map(|position| state.queue.id_at_order_position(position))
                    .map(QueueItem::Track)
                    .collect::<Vec<_>>();
                let total = state.queue.len().saturating_sub(upcoming_start) as i64;
                (ids, total, has_more)
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
        let resolved =
            self.resolve_track_uris(requested_ids, "could not resolve an enqueued track")?;
        let track_ids = resolved
            .iter()
            .map(|(_, track_id, _)| *track_id)
            .collect::<Vec<_>>();
        let uris = resolved
            .into_iter()
            .map(|(_, _, uri)| uri)
            .collect::<Vec<_>>();
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

    fn resolve_track_uris(
        &self,
        requested_ids: Vec<i64>,
        error_context: &str,
    ) -> Result<Vec<(usize, i64, String)>, AndroidPlaybackError> {
        let database = self
            .inner
            .database
            .lock()
            .map_err(|_| AndroidPlaybackError::Backend {
                detail: "playback queue database was poisoned".to_owned(),
            })?;
        requested_ids
            .into_iter()
            .enumerate()
            .filter_map(|(index, track_id)| {
                queries::track_source_path(&database, track_id)
                    .map_err(|error| AndroidPlaybackError::Backend {
                        detail: format!("{error_context}: {error}"),
                    })
                    .transpose()
                    .map(|result| {
                        result.map(|path| (index, track_id, path.to_string_lossy().into_owned()))
                    })
            })
            .collect()
    }
}

fn queue_window_start(state: &super::SessionState) -> Option<usize> {
    state
        .queue
        .current_order_position()
        .map(|position| position.saturating_add(usize::from(state.current_loaded)))
}

/// Resolves a signed offset from the upcoming boundary without shifting the
/// requested span when either queue end clips it. The current row is offset
/// `-1`, the previous row `-2`, and so on.
fn signed_queue_window(
    upcoming_start: usize,
    queue_len: usize,
    offset: i64,
    limit: usize,
) -> std::ops::Range<usize> {
    let upcoming_start = i64::try_from(upcoming_start).unwrap_or(i64::MAX);
    let queue_len_i64 = i64::try_from(queue_len).unwrap_or(i64::MAX);
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let requested_start = upcoming_start.saturating_add(offset);
    let requested_end = requested_start.saturating_add(limit);
    let start = requested_start.clamp(0, queue_len_i64) as usize;
    let end = requested_end.clamp(0, queue_len_i64) as usize;
    start.min(end)..end
}

fn upcoming_order_position(state: &super::SessionState, position: u64) -> Option<usize> {
    let position = usize::try_from(position).ok()?;
    queue_window_start(state)?.checked_add(position)
}

fn stable_id_at(state: &super::SessionState, order_position: usize) -> Option<i64> {
    state.queue.id_at_order_position(order_position)
}
