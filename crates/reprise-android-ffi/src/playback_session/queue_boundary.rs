//! Windowed and identity-guarded queue operations exported to Android.

use std::collections::HashSet;

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
        let offset = usize::try_from(window.offset.max(0)).unwrap_or(usize::MAX);
        let limit = usize::try_from(window.limit.max(0)).unwrap_or(0);
        loop {
            let (ids, total) = {
                let state = self.inner.lock()?;
                let ids = state
                    .queue
                    .remaining_window(offset, limit)
                    .into_iter()
                    .map(QueueItem::Track)
                    .collect::<Vec<_>>();
                (ids, state.queue.remaining_len() as i64)
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
                state.queue.remove_ids_except_current(&missing);
                (state.next_uri(), state.queue.clone())
            };
            self.inner.persist_queue(&queue_to_save)?;
            self.inner.backend()?.set_next(next_uri.as_deref());
        }
    }
}
