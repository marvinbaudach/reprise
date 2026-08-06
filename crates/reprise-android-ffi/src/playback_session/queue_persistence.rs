//! Core session persistence adapted to Android's stable track/URI playback state.

use std::collections::{HashMap, HashSet};

use reprise_core::db::Db;
use reprise_core::library::session;
use reprise_core::queries::{self, QueueItemMetadata};
use reprise_core::queue::{Queue, QueueSnapshotError};
use reprise_core::up_next::QueueItem;

const RESTORE_WINDOW_LIMIT: i64 = 500;

pub(super) struct RestoredQueue {
    pub(super) queue: Queue,
    pub(super) track_ids: Vec<i64>,
    pub(super) uris: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum QueuePersistenceError {
    #[error("queue query failed: {detail}")]
    Query { detail: String },
    #[error("session error: {0}")]
    Session(#[from] session::SessionError),
    #[error("queue snapshot error: {0}")]
    Snapshot(#[from] QueueSnapshotError),
}

pub(super) fn restore(db: &Db) -> Result<RestoredQueue, QueuePersistenceError> {
    let snapshot = session::load(db).queue;
    let mut queue = Queue::new();
    queue.restore_snapshot(snapshot.clone())?;

    let items = snapshot
        .ids
        .iter()
        .copied()
        .map(QueueItem::Track)
        .collect::<Vec<_>>();
    let mut paths = HashMap::new();
    for offset in (0..items.len()).step_by(RESTORE_WINDOW_LIMIT as usize) {
        let window =
            queries::query_queue_item_window(db, &items, offset as i64, RESTORE_WINDOW_LIMIT)
                .map_err(|error| QueuePersistenceError::Query {
                    detail: error.to_string(),
                })?;
        for metadata in window {
            if let QueueItemMetadata::Track(track) = metadata {
                paths.entry(track.id).or_insert(track.path);
            }
        }
    }

    let resolved = paths.keys().copied().collect::<HashSet<_>>();
    let missing = snapshot
        .ids
        .iter()
        .copied()
        .filter(|id| !resolved.contains(id))
        .collect::<Vec<_>>();
    queue.remove_ids(&missing);

    let track_ids = queue.snapshot().ids;
    let uris = track_ids
        .iter()
        .filter_map(|id| paths.get(id).cloned())
        .collect();
    Ok(RestoredQueue {
        queue,
        track_ids,
        uris,
    })
}

pub(super) fn save(db: &Db, queue: &Queue) -> Result<(), QueuePersistenceError> {
    let mut state = session::load(db);
    state.queue = queue.snapshot();
    session::save(db, &state)?;
    Ok(())
}
