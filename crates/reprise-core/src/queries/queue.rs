//! `ViewSource::Queue` queries over the caller-owned manual queue order.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

use crate::db::Db;
use crate::models::Track;
use crate::podcasts::EpisodeRow;
use crate::up_next::QueueItem;

use super::clauses::{ai_projection, row_to_track};
use super::MAX_WINDOW_LIMIT;

/// Hard cap for playback snapshots and the manual queue.
pub const QUEUE_LIMIT: i64 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueItemMetadata {
    Track(Track),
    Episode(EpisodeRow),
}

impl QueueItemMetadata {
    pub fn item(&self) -> QueueItem {
        match self {
            Self::Track(track) => QueueItem::Track(track.id),
            Self::Episode(episode) => QueueItem::Episode(episode.id),
        }
    }

    pub fn duration_ms(&self) -> i64 {
        match self {
            Self::Track(track) => track.duration_ms,
            Self::Episode(episode) => episode
                .duration_secs
                .unwrap_or_default()
                .saturating_mul(1_000),
        }
    }
}

/// Resolves one bounded queue window with at most one batched query for each
/// item kind present. Duplicate entries render once per occurrence.
pub fn query_queue_item_window(
    db: &Db,
    items: &[QueueItem],
    offset: i64,
    limit: i64,
) -> Result<Vec<QueueItemMetadata>, rusqlite::Error> {
    query_queue_item_window_with_observer(db.conn(), items, offset, limit, true, || {})
}

pub(super) fn query_track_window_queue(
    conn: &Connection,
    items: &[QueueItem],
    offset: i64,
    limit: i64,
    project_ai: bool,
) -> Result<Vec<Track>, rusqlite::Error> {
    Ok(
        query_queue_item_window_with_observer(conn, items, offset, limit, project_ai, || {})?
            .into_iter()
            .filter_map(|metadata| match metadata {
                QueueItemMetadata::Track(track) => Some(track),
                QueueItemMetadata::Episode(_) => None,
            })
            .collect(),
    )
}

#[cfg(test)]
pub(super) fn query_queue_item_window_counted(
    conn: &Connection,
    items: &[QueueItem],
    offset: i64,
    limit: i64,
    on_query: impl FnMut(),
) -> Result<Vec<QueueItemMetadata>, rusqlite::Error> {
    query_queue_item_window_with_observer(conn, items, offset, limit, true, on_query)
}

fn query_queue_item_window_with_observer(
    conn: &Connection,
    items: &[QueueItem],
    offset: i64,
    limit: i64,
    project_ai: bool,
    mut on_query: impl FnMut(),
) -> Result<Vec<QueueItemMetadata>, rusqlite::Error> {
    let limit = limit.clamp(0, MAX_WINDOW_LIMIT);
    if limit == 0 || offset < 0 {
        return Ok(Vec::new());
    }
    let offset = offset as usize;
    if offset >= items.len() {
        return Ok(Vec::new());
    }
    let end = offset.saturating_add(limit as usize).min(items.len());
    let slice = &items[offset..end];
    let track_ids = distinct_ids(slice.iter().filter_map(|item| item.track_id()));
    let episode_ids = distinct_ids(slice.iter().filter_map(|item| item.episode_id()));

    let mut resolved = HashMap::with_capacity(track_ids.len() + episode_ids.len());
    if !track_ids.is_empty() {
        on_query();
        for track in query_tracks(conn, &track_ids, project_ai)? {
            resolved.insert(QueueItem::Track(track.id), QueueItemMetadata::Track(track));
        }
    }
    if !episode_ids.is_empty() {
        on_query();
        for episode in query_episodes(conn, &episode_ids)? {
            resolved.insert(
                QueueItem::Episode(episode.id),
                QueueItemMetadata::Episode(episode),
            );
        }
    }

    Ok(slice
        .iter()
        .filter_map(|item| resolved.get(item).cloned())
        .collect())
}

fn distinct_ids(ids: impl Iterator<Item = i64>) -> Vec<i64> {
    ids.collect::<HashSet<_>>().into_iter().collect()
}

fn placeholders(count: usize) -> String {
    (1..=count)
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn query_tracks(
    conn: &Connection,
    ids: &[i64],
    project_ai: bool,
) -> Result<Vec<Track>, rusqlite::Error> {
    let is_ai = ai_projection(project_ai);
    let sql = format!(
        "SELECT id, path, title, artist, album, album_artist, year, track_no, genre, \
         duration_ms, bitrate_kbps, rating, play_count, last_played_at, added_at, \
         file_mtime, missing_since, missing_reason, untagged, file_size, device, inode, \
         {is_ai} AS is_ai \
         FROM tracks WHERE id IN ({})",
        placeholders(ids.len())
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(ids), row_to_track)?
        .collect();
    rows
}

fn query_episodes(conn: &Connection, ids: &[i64]) -> Result<Vec<EpisodeRow>, rusqlite::Error> {
    let sql = format!(
        "SELECT {}
         FROM podcast_episodes e
         JOIN podcast_subscriptions s ON s.id = e.subscription_id
         WHERE e.id IN ({})
           AND e.removed_at IS NULL
           AND s.removed_at IS NULL",
        crate::podcasts::query::EPISODE_COLUMNS,
        placeholders(ids.len())
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement
        .query_map(
            rusqlite::params_from_iter(ids),
            crate::podcasts::store::episode_from_row,
        )?
        .collect();
    rows
}

/// Sums durations in queue order. Missing duration contributes zero.
pub fn query_queue_duration_ms(db: &Db, items: &[QueueItem]) -> Result<i64, rusqlite::Error> {
    if items.is_empty() {
        return Ok(0);
    }
    let conn = db.conn();
    let track_ids = distinct_ids(items.iter().filter_map(|item| item.track_id()));
    let episode_ids = distinct_ids(items.iter().filter_map(|item| item.episode_id()));
    let tracks = query_durations(conn, "tracks", "id", "duration_ms", "", &track_ids)?;
    let episodes = query_durations(
        conn,
        "podcast_episodes e JOIN podcast_subscriptions s ON s.id = e.subscription_id",
        "e.id",
        "COALESCE(e.duration_secs, 0) * 1000",
        "AND e.removed_at IS NULL AND s.removed_at IS NULL",
        &episode_ids,
    )?;
    Ok(items.iter().fold(0_i64, |total, item| {
        let duration = match item {
            QueueItem::Track(id) => tracks.get(id),
            QueueItem::Episode(id) => episodes.get(id),
        }
        .copied()
        .unwrap_or_default();
        total.saturating_add(duration)
    }))
}

pub(super) fn query_queue_item_count(
    conn: &Connection,
    items: &[QueueItem],
) -> Result<i64, rusqlite::Error> {
    if items.is_empty() {
        return Ok(0);
    }
    let track_ids = distinct_ids(items.iter().filter_map(|item| item.track_id()));
    let episode_ids = distinct_ids(items.iter().filter_map(|item| item.episode_id()));
    let tracks = query_existing_ids(conn, "tracks", "id", "", &track_ids)?;
    let episodes = query_existing_ids(
        conn,
        "podcast_episodes e JOIN podcast_subscriptions s ON s.id = e.subscription_id",
        "e.id",
        "AND e.removed_at IS NULL AND s.removed_at IS NULL",
        &episode_ids,
    )?;
    Ok(items
        .iter()
        .filter(|item| match item {
            QueueItem::Track(id) => tracks.contains(id),
            QueueItem::Episode(id) => episodes.contains(id),
        })
        .count() as i64)
}

pub(super) fn query_track_count_queue(
    conn: &Connection,
    items: &[QueueItem],
) -> Result<i64, rusqlite::Error> {
    query_queue_item_count(conn, items)
}

fn query_existing_ids(
    conn: &Connection,
    source: &str,
    id_column: &str,
    predicate: &str,
    ids: &[i64],
) -> Result<HashSet<i64>, rusqlite::Error> {
    if ids.is_empty() {
        return Ok(HashSet::new());
    }
    let sql = format!(
        "SELECT {id_column} FROM {source} WHERE {id_column} IN ({}) {predicate}",
        placeholders(ids.len())
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(ids), |row| row.get(0))?
        .collect();
    rows
}

#[allow(clippy::too_many_arguments)]
fn query_durations(
    conn: &Connection,
    source: &str,
    id_column: &str,
    duration_column: &str,
    predicate: &str,
    ids: &[i64],
) -> Result<HashMap<i64, i64>, rusqlite::Error> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let sql = format!(
        "SELECT {id_column}, {duration_column}
         FROM {source}
         WHERE {id_column} IN ({}) {predicate}",
        placeholders(ids.len())
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(ids), |row| {
            let id = row.get(0)?;
            let duration: i64 = row.get(1)?;
            Ok((id, duration))
        })?
        .collect();
    rows
}

/// Active episode ids eligible to remain in or advance from the manual queue.
pub fn query_available_episode_ids(db: &Db) -> Result<HashSet<i64>, rusqlite::Error> {
    let mut statement = db.conn().prepare(
        "SELECT e.id
         FROM podcast_episodes e
         JOIN podcast_subscriptions s ON s.id = e.subscription_id
         WHERE e.removed_at IS NULL AND s.removed_at IS NULL",
    )?;
    let rows = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>();
    rows
}

/// Whether a `query_track_ids` result probably reached the queue cap.
pub fn is_queue_capped(len: usize) -> bool {
    len as i64 >= QUEUE_LIMIT
}
