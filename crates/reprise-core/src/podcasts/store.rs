//! Podcast subscription and episode persistence.

use rusqlite::{params, Connection, OptionalExtension};

use super::feed::ParsedEpisode;
use super::{EpisodeRow, PodcastKind, SubscriptionRow};

pub use super::downloads::{
    downloaded_paths_for_subscription, set_downloaded_file, set_downloaded_path,
};
pub use super::phone_sync::set_enabled as set_sync_to_phone;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewSubscription {
    pub kind: PodcastKind,
    pub feed_url: String,
    pub title: String,
    pub author: Option<String>,
    pub image_url: Option<String>,
    pub auto_download: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpsertResult {
    pub episode_id: i64,
    pub inserted: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FetchSuccess<'a> {
    pub etag: Option<&'a str>,
    pub last_modified: Option<&'a str>,
    pub title: Option<&'a str>,
    pub author: Option<&'a str>,
    pub image_url: Option<&'a str>,
}

pub fn add_or_restore(
    conn: &Connection,
    subscription: &NewSubscription,
    now: i64,
) -> Result<i64, rusqlite::Error> {
    add_or_restore_in(conn, subscription, now)
}

pub fn add_or_restore_with_baseline(
    conn: &Connection,
    subscription: &NewSubscription,
    now: i64,
    future_only_baseline: Option<&[String]>,
) -> Result<i64, rusqlite::Error> {
    let transaction = conn.unchecked_transaction()?;
    let subscription_id = add_or_restore_in(&transaction, subscription, now)?;
    replace_future_only_baseline_in(
        &transaction,
        subscription_id,
        future_only_baseline.unwrap_or_default(),
    )?;
    transaction.commit()?;
    Ok(subscription_id)
}

/// Inserts a subscription, or revives/updates the existing row for the
/// same `feed_url`. Phone-sync state (`sync_to_phone` and any per-device
/// selection, `POD-12`) is preserved only when the subscription's kind is
/// unchanged; a kind change at the same URL is a different content type
/// under an old sync flag, so both are cleared — this is not RSS/YouTube
/// special-casing, it is symmetric in the direction of the change.
fn add_or_restore_in(
    conn: &Connection,
    subscription: &NewSubscription,
    now: i64,
) -> Result<i64, rusqlite::Error> {
    let previous_kind: Option<String> = conn
        .query_row(
            "SELECT kind FROM podcast_subscriptions WHERE feed_url = ?1",
            [&subscription.feed_url],
            |row| row.get(0),
        )
        .optional()?;
    let new_kind = kind_setting(subscription.kind);
    let subscription_id = conn.query_row(
        "INSERT INTO podcast_subscriptions
         (kind, feed_url, title, author, image_url, auto_download, added_at, removed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)
         ON CONFLICT(feed_url) DO UPDATE SET
           kind = excluded.kind,
           title = excluded.title,
           author = excluded.author,
           image_url = COALESCE(excluded.image_url, podcast_subscriptions.image_url),
           auto_download = excluded.auto_download,
           sync_to_phone = CASE
             WHEN excluded.kind = podcast_subscriptions.kind THEN podcast_subscriptions.sync_to_phone
             ELSE 0
           END,
           removed_at = NULL
         RETURNING id",
        params![
            new_kind,
            subscription.feed_url,
            subscription.title,
            subscription.author,
            subscription.image_url,
            subscription.auto_download,
            now
        ],
        |row| row.get(0),
    )?;
    let kind_changed = previous_kind
        .as_deref()
        .is_some_and(|kind| kind != new_kind);
    if kind_changed {
        conn.execute(
            "DELETE FROM podcast_subscription_devices WHERE subscription_id = ?1",
            [subscription_id],
        )?;
    }
    Ok(subscription_id)
}

pub fn active_subscriptions(conn: &Connection) -> Result<Vec<SubscriptionRow>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT id, kind, feed_url, title, author, image_url, etag,
                last_modified, last_fetch_at, last_outcome, auto_download,
                sync_to_phone, latest_per_channel, added_at, removed_at
         FROM podcast_subscriptions
         WHERE removed_at IS NULL
         ORDER BY title COLLATE NOCASE, id",
    )?;
    let rows = statement.query_map([], subscription_from_row)?;
    rows.collect::<Result<Vec<_>, _>>()
}

pub fn subscription(
    conn: &Connection,
    id: i64,
) -> Result<Option<SubscriptionRow>, rusqlite::Error> {
    conn.query_row(
        "SELECT id, kind, feed_url, title, author, image_url, etag,
                last_modified, last_fetch_at, last_outcome, auto_download,
                sync_to_phone, latest_per_channel, added_at, removed_at
         FROM podcast_subscriptions
         WHERE id = ?1",
        [id],
        subscription_from_row,
    )
    .optional()
}

/// `MTP-36`: every persisted per-channel override among `subscription_ids`,
/// keyed by subscription id — a channel with no override (the common case,
/// especially before design 6b's channel surface exists) is simply absent
/// from the map rather than present with a placeholder, so the caller's
/// fallback to the global default (`resolve_latest_per_channel`) is the
/// only place "no override" is decided.
pub fn latest_per_channel_overrides(
    conn: &Connection,
    subscription_ids: &[i64],
) -> Result<std::collections::HashMap<i64, i64>, rusqlite::Error> {
    if subscription_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = (1..=subscription_ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT id, latest_per_channel FROM podcast_subscriptions
         WHERE id IN ({placeholders}) AND latest_per_channel IS NOT NULL"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(rusqlite::params_from_iter(subscription_ids.iter()), |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
    })?;
    rows.collect()
}

/// `MTP-36`: sets or clears (`None`) this channel's override of the global
/// "latest N per channel" default. No GTK surface calls this yet (design
/// 6b's channel page has no control for it) — it exists so the persistence
/// and the live pipeline can be tested and used independently of that UI.
pub fn set_latest_per_channel(
    conn: &Connection,
    id: i64,
    value: Option<i64>,
) -> Result<bool, rusqlite::Error> {
    Ok(conn.execute(
        "UPDATE podcast_subscriptions
         SET latest_per_channel = ?2
         WHERE id = ?1 AND removed_at IS NULL",
        params![id, value],
    )? != 0)
}

pub fn count_subscriptions(conn: &Connection) -> Result<usize, rusqlite::Error> {
    conn.query_row(
        "SELECT COUNT(*) FROM podcast_subscriptions WHERE removed_at IS NULL",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count.max(0) as usize)
}

pub fn update_subscription_details(
    conn: &Connection,
    id: i64,
    title: Option<&str>,
    auto_download: Option<bool>,
) -> Result<bool, rusqlite::Error> {
    Ok(conn.execute(
        "UPDATE podcast_subscriptions
         SET title = COALESCE(?2, title),
             auto_download = COALESCE(?3, auto_download)
         WHERE id = ?1 AND removed_at IS NULL",
        params![id, title, auto_download],
    )? != 0)
}

pub fn replace_future_only_baseline(
    conn: &Connection,
    subscription_id: i64,
    guids: &[String],
) -> Result<(), rusqlite::Error> {
    let transaction = conn.unchecked_transaction()?;
    replace_future_only_baseline_in(&transaction, subscription_id, guids)?;
    transaction.commit()
}

fn replace_future_only_baseline_in(
    conn: &Connection,
    subscription_id: i64,
    guids: &[String],
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "DELETE FROM podcast_subscription_baselines WHERE subscription_id = ?1",
        [subscription_id],
    )?;
    {
        let mut insert = conn.prepare(
            "INSERT OR IGNORE INTO podcast_subscription_baselines (subscription_id, guid)
             VALUES (?1, ?2)",
        )?;
        for guid in guids {
            insert.execute(params![subscription_id, guid])?;
        }
    }
    Ok(())
}

pub fn clear_future_only_baseline(
    conn: &Connection,
    subscription_id: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "DELETE FROM podcast_subscription_baselines WHERE subscription_id = ?1",
        [subscription_id],
    )?;
    Ok(())
}

pub fn future_only_baseline(
    conn: &Connection,
    subscription_id: i64,
) -> Result<Vec<String>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT guid FROM podcast_subscription_baselines
         WHERE subscription_id = ?1
         ORDER BY guid",
    )?;
    let guids = statement
        .query_map([subscription_id], |row| row.get(0))?
        .collect();
    guids
}

pub fn upsert_episode(
    conn: &Connection,
    subscription_id: i64,
    episode: &ParsedEpisode,
    now: i64,
) -> Result<Option<UpsertResult>, rusqlite::Error> {
    let dismissed = conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM podcast_episode_dismissals
           WHERE subscription_id = ?1 AND guid = ?2
         )",
        params![subscription_id, episode.guid],
        |row| row.get::<_, bool>(0),
    )?;
    if dismissed {
        return Ok(None);
    }
    let existing = conn
        .query_row(
            "SELECT id FROM podcast_episodes
             WHERE subscription_id = ?1 AND guid = ?2",
            params![subscription_id, episode.guid],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let episode_id = conn.query_row(
        "INSERT INTO podcast_episodes
         (subscription_id, guid, title, audio_url, page_url, published_at,
          duration_secs, first_seen_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(subscription_id, guid) DO UPDATE SET
           title = excluded.title,
           audio_url = excluded.audio_url,
           page_url = excluded.page_url,
           published_at = COALESCE(excluded.published_at, podcast_episodes.published_at),
           duration_secs = COALESCE(excluded.duration_secs, podcast_episodes.duration_secs)
         RETURNING id",
        params![
            subscription_id,
            episode.guid,
            episode.title,
            episode.audio_url,
            episode.page_url,
            episode.published_at,
            episode.duration_secs,
            now
        ],
        |row| row.get(0),
    )?;
    Ok(Some(UpsertResult {
        episode_id,
        inserted: existing.is_none(),
    }))
}

pub fn episode(conn: &Connection, id: i64) -> Result<Option<EpisodeRow>, rusqlite::Error> {
    conn.query_row(
        "SELECT e.id, e.subscription_id, e.guid, e.title, s.title,
                s.image_url, s.kind, e.audio_url, e.page_url, e.published_at,
                e.duration_secs, e.downloaded_path, e.downloaded_bytes, e.played_at,
                e.position_ms, e.first_seen_at
         FROM podcast_episodes e
         JOIN podcast_subscriptions s ON s.id = e.subscription_id
         WHERE e.id = ?1 AND s.removed_at IS NULL AND e.removed_at IS NULL",
        [id],
        episode_from_row,
    )
    .optional()
}

pub fn update_fetch_success(
    conn: &Connection,
    id: i64,
    now: i64,
    metadata: FetchSuccess<'_>,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE podcast_subscriptions SET
           last_fetch_at = ?2,
           last_outcome = 'ok',
           etag = COALESCE(?3, etag),
           last_modified = COALESCE(?4, last_modified),
           title = COALESCE(?5, title),
           author = COALESCE(?6, author),
           image_url = COALESCE(?7, image_url)
         WHERE id = ?1",
        params![
            id,
            now,
            metadata.etag,
            metadata.last_modified,
            metadata.title,
            metadata.author,
            metadata.image_url
        ],
    )?;
    Ok(())
}

pub fn update_fetch_not_modified(
    conn: &Connection,
    id: i64,
    now: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE podcast_subscriptions
         SET last_fetch_at = ?2, last_outcome = 'not_modified'
         WHERE id = ?1",
        params![id, now],
    )?;
    Ok(())
}

pub fn update_fetch_failed(conn: &Connection, id: i64, now: i64) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE podcast_subscriptions
         SET last_fetch_at = ?2, last_outcome = 'failed'
         WHERE id = ?1",
        params![id, now],
    )?;
    Ok(())
}

pub fn save_position(
    conn: &Connection,
    episode_id: i64,
    position_ms: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE podcast_episodes SET position_ms = ?2 WHERE id = ?1",
        params![episode_id, position_ms.max(0)],
    )?;
    Ok(())
}

pub fn save_duration(
    conn: &Connection,
    episode_id: i64,
    duration_secs: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE podcast_episodes
         SET duration_secs = ?2
         WHERE id = ?1 AND duration_secs IS NULL AND ?2 > 0",
        params![episode_id, duration_secs],
    )?;
    Ok(())
}

pub fn mark_played(conn: &Connection, episode_id: i64, now: i64) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE podcast_episodes
         SET played_at = ?2, position_ms = 0
         WHERE id = ?1",
        params![episode_id, now],
    )?;
    Ok(())
}

pub fn mark_unplayed(conn: &Connection, episode_id: i64) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE podcast_episodes SET played_at = NULL WHERE id = ?1",
        [episode_id],
    )?;
    Ok(())
}

pub fn tombstone_episode(conn: &Connection, id: i64, now: i64) -> Result<bool, rusqlite::Error> {
    Ok(conn.execute(
        "UPDATE podcast_episodes
         SET removed_at = ?2
         WHERE id = ?1 AND removed_at IS NULL",
        params![id, now],
    )? != 0)
}

pub fn undo_remove_episode(conn: &Connection, id: i64) -> Result<bool, rusqlite::Error> {
    Ok(conn.execute(
        "UPDATE podcast_episodes
         SET removed_at = NULL
         WHERE id = ?1 AND removed_at IS NOT NULL",
        [id],
    )? != 0)
}

pub fn commit_remove_episode(
    conn: &Connection,
    id: i64,
) -> Result<Option<String>, rusqlite::Error> {
    let transaction = conn.unchecked_transaction()?;
    let removed = transaction
        .query_row(
            "SELECT subscription_id, guid, removed_at, downloaded_path
             FROM podcast_episodes
             WHERE id = ?1 AND removed_at IS NOT NULL",
            [id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((subscription_id, guid, removed_at, downloaded_path)) = removed else {
        transaction.commit()?;
        return Ok(None);
    };
    transaction.execute(
        "INSERT INTO podcast_episode_dismissals (subscription_id, guid, removed_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(subscription_id, guid) DO UPDATE SET removed_at = excluded.removed_at",
        params![subscription_id, guid, removed_at],
    )?;
    transaction.execute("DELETE FROM podcast_episodes WHERE id = ?1", [id])?;
    transaction.commit()?;
    Ok(downloaded_path)
}

pub fn tombstone_subscription(conn: &Connection, id: i64, now: i64) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE podcast_subscriptions SET removed_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    Ok(())
}

pub fn undo_remove_subscription(conn: &Connection, id: i64) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE podcast_subscriptions SET removed_at = NULL WHERE id = ?1",
        [id],
    )?;
    Ok(())
}

pub fn commit_remove_subscription(conn: &Connection, id: i64) -> Result<(), rusqlite::Error> {
    conn.execute(
        "DELETE FROM podcast_subscriptions WHERE id = ?1 AND removed_at IS NOT NULL",
        [id],
    )?;
    Ok(())
}

fn subscription_from_row(row: &rusqlite::Row<'_>) -> Result<SubscriptionRow, rusqlite::Error> {
    let kind: String = row.get(1)?;
    Ok(SubscriptionRow {
        id: row.get(0)?,
        kind: parse_kind(&kind)?,
        feed_url: row.get(2)?,
        title: row.get(3)?,
        author: row.get(4)?,
        image_url: row.get(5)?,
        etag: row.get(6)?,
        last_modified: row.get(7)?,
        last_fetch_at: row.get(8)?,
        last_outcome: row.get(9)?,
        auto_download: row.get(10)?,
        sync_to_phone: row.get(11)?,
        latest_per_channel: row.get(12)?,
        added_at: row.get(13)?,
        removed_at: row.get(14)?,
    })
}

pub(crate) fn episode_from_row(row: &rusqlite::Row<'_>) -> Result<EpisodeRow, rusqlite::Error> {
    let kind: String = row.get(6)?;
    Ok(EpisodeRow {
        id: row.get(0)?,
        subscription_id: row.get(1)?,
        guid: row.get(2)?,
        title: row.get(3)?,
        show: row.get(4)?,
        show_image_url: row.get(5)?,
        kind: parse_kind(&kind)?,
        audio_url: row.get(7)?,
        page_url: row.get(8)?,
        published_at: row.get(9)?,
        duration_secs: row.get(10)?,
        downloaded_path: row.get(11)?,
        downloaded_bytes: row.get(12)?,
        played_at: row.get(13)?,
        position_ms: row.get(14)?,
        first_seen_at: row.get(15)?,
    })
}

fn kind_setting(kind: PodcastKind) -> &'static str {
    match kind {
        PodcastKind::Rss => "rss",
        PodcastKind::Youtube => "youtube",
    }
}

fn parse_kind(value: &str) -> Result<PodcastKind, rusqlite::Error> {
    match value {
        "rss" => Ok(PodcastKind::Rss),
        "youtube" => Ok(PodcastKind::Youtube),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            format!("unknown podcast kind {other}").into(),
        )),
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
