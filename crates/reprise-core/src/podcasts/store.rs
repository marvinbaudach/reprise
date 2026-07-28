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

fn add_or_restore_in(
    conn: &Connection,
    subscription: &NewSubscription,
    now: i64,
) -> Result<i64, rusqlite::Error> {
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
             WHEN excluded.kind = 'rss' THEN podcast_subscriptions.sync_to_phone
             ELSE 0
           END,
           removed_at = NULL
         RETURNING id",
        params![
            kind_setting(subscription.kind),
            subscription.feed_url,
            subscription.title,
            subscription.author,
            subscription.image_url,
            subscription.auto_download,
            now
        ],
        |row| row.get(0),
    )?;
    if subscription.kind == PodcastKind::Youtube {
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
                sync_to_phone, added_at, removed_at
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
                sync_to_phone, added_at, removed_at
         FROM podcast_subscriptions
         WHERE id = ?1",
        [id],
        subscription_from_row,
    )
    .optional()
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
        added_at: row.get(12)?,
        removed_at: row.get(13)?,
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
mod tests {
    use super::*;

    fn conn() -> Connection {
        crate::db::open_migrated(None).unwrap()
    }

    fn subscription_draft() -> NewSubscription {
        NewSubscription {
            kind: PodcastKind::Rss,
            feed_url: "https://example.test/feed.xml".to_owned(),
            title: "Original Show".to_owned(),
            author: Some("Ada".to_owned()),
            image_url: None,
            auto_download: false,
        }
    }

    fn parsed_episode(title: &str) -> ParsedEpisode {
        ParsedEpisode {
            guid: "stable-guid".to_owned(),
            title: title.to_owned(),
            audio_url: "https://example.test/episode.mp3".to_owned(),
            page_url: None,
            published_at: Some(100),
            duration_secs: None,
        }
    }

    #[test]
    fn pod_2_episode_upsert_changes_metadata_but_preserves_listening_state() {
        let conn = conn();
        let subscription_id = add_or_restore(&conn, &subscription_draft(), 10).unwrap();
        let first = upsert_episode(&conn, subscription_id, &parsed_episode("Old"), 20)
            .unwrap()
            .expect("episode should be imported");
        save_position(&conn, first.episode_id, 8_000).unwrap();
        conn.execute(
            "UPDATE podcast_episodes SET played_at = 30 WHERE id = ?1",
            [first.episode_id],
        )
        .unwrap();

        let second = upsert_episode(&conn, subscription_id, &parsed_episode("Renamed"), 99)
            .unwrap()
            .expect("episode should be updated");
        let row = episode(&conn, first.episode_id).unwrap().unwrap();

        assert!(first.inserted);
        assert!(!second.inserted);
        assert_eq!(second.episode_id, first.episode_id);
        assert_eq!(row.title, "Renamed");
        assert_eq!(row.first_seen_at, 20);
        assert_eq!(row.position_ms, 8_000);
        assert_eq!(row.played_at, Some(30));
    }

    #[test]
    fn future_only_baseline_replaces_and_clears_atomically() {
        let conn = conn();
        let subscription_id = add_or_restore(&conn, &subscription_draft(), 10).unwrap();

        replace_future_only_baseline(
            &conn,
            subscription_id,
            &["old-a".to_owned(), "old-b".to_owned()],
        )
        .unwrap();
        assert_eq!(
            future_only_baseline(&conn, subscription_id).unwrap(),
            ["old-a".to_owned(), "old-b".to_owned()]
        );

        replace_future_only_baseline(&conn, subscription_id, &["new".to_owned()]).unwrap();
        assert_eq!(
            future_only_baseline(&conn, subscription_id).unwrap(),
            ["new".to_owned()]
        );

        clear_future_only_baseline(&conn, subscription_id).unwrap();
        assert!(future_only_baseline(&conn, subscription_id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn subscription_tombstone_cycle_updates_counts_and_can_commit() {
        let conn = conn();
        let id = add_or_restore(&conn, &subscription_draft(), 10).unwrap();
        upsert_episode(&conn, id, &parsed_episode("Episode"), 20).unwrap();
        assert_eq!(count_subscriptions(&conn).unwrap(), 1);

        tombstone_subscription(&conn, id, 30).unwrap();
        assert_eq!(count_subscriptions(&conn).unwrap(), 0);
        assert!(active_subscriptions(&conn).unwrap().is_empty());

        undo_remove_subscription(&conn, id).unwrap();
        assert_eq!(count_subscriptions(&conn).unwrap(), 1);

        tombstone_subscription(&conn, id, 40).unwrap();
        commit_remove_subscription(&conn, id).unwrap();
        assert!(subscription(&conn, id).unwrap().is_none());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM podcast_episodes", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn resubscribe_revives_existing_identity_and_history() {
        let conn = conn();
        let id = add_or_restore(&conn, &subscription_draft(), 10).unwrap();
        let episode = upsert_episode(&conn, id, &parsed_episode("Episode"), 20)
            .unwrap()
            .expect("episode should be imported");
        save_position(&conn, episode.episode_id, 12_000).unwrap();
        tombstone_subscription(&conn, id, 30).unwrap();

        let revived = add_or_restore(
            &conn,
            &NewSubscription {
                title: "Renamed Show".to_owned(),
                ..subscription_draft()
            },
            40,
        )
        .unwrap();

        assert_eq!(revived, id);
        assert_eq!(subscription(&conn, id).unwrap().unwrap().added_at, 10);
        assert_eq!(
            super::episode(&conn, episode.episode_id)
                .unwrap()
                .unwrap()
                .position_ms,
            12_000
        );
    }

    #[test]
    fn episode_finish_marks_played_and_clears_resume_position() {
        let conn = conn();
        let subscription_id = add_or_restore(&conn, &subscription_draft(), 10).unwrap();
        let result = upsert_episode(&conn, subscription_id, &parsed_episode("Episode"), 20)
            .unwrap()
            .expect("episode should be imported");
        save_position(&conn, result.episode_id, 9_000).unwrap();

        mark_played(&conn, result.episode_id, 30).unwrap();

        let row = episode(&conn, result.episode_id).unwrap().unwrap();
        assert_eq!(row.played_at, Some(30));
        assert_eq!(row.position_ms, 0);
    }

    #[test]
    fn pod_7_download_metadata_persists_and_clears_path_with_size() {
        let conn = conn();
        let subscription_id = add_or_restore(&conn, &subscription_draft(), 10).unwrap();
        let episode = upsert_episode(&conn, subscription_id, &parsed_episode("Episode"), 20)
            .unwrap()
            .expect("episode should be imported");

        set_downloaded_file(
            &conn,
            episode.episode_id,
            Some("/downloads/episode.mp3"),
            Some(41_943_040),
        )
        .unwrap();
        let downloaded = super::episode(&conn, episode.episode_id).unwrap().unwrap();
        assert_eq!(
            downloaded.downloaded_path.as_deref(),
            Some("/downloads/episode.mp3")
        );
        assert_eq!(downloaded.downloaded_bytes, Some(41_943_040));

        set_downloaded_file(&conn, episode.episode_id, None, None).unwrap();
        let cleared = super::episode(&conn, episode.episode_id).unwrap().unwrap();
        assert_eq!(cleared.downloaded_path, None);
        assert_eq!(cleared.downloaded_bytes, None);
    }

    #[test]
    fn pod_6_episode_removal_undo_and_commit_block_rss_and_youtube_reimport() {
        for kind in [PodcastKind::Rss, PodcastKind::Youtube] {
            let conn = conn();
            let subscription_id = add_or_restore(
                &conn,
                &NewSubscription {
                    kind,
                    ..subscription_draft()
                },
                10,
            )
            .unwrap();
            let episode = upsert_episode(&conn, subscription_id, &parsed_episode("Episode"), 20)
                .unwrap()
                .expect("episode should be imported");
            set_downloaded_path(&conn, episode.episode_id, Some("/kept/download.mp3")).unwrap();

            assert!(tombstone_episode(&conn, episode.episode_id, 30).unwrap());
            assert!(super::episode(&conn, episode.episode_id).unwrap().is_none());
            assert!(super::super::query::list_episodes(&conn)
                .unwrap()
                .is_empty());

            assert!(undo_remove_episode(&conn, episode.episode_id).unwrap());
            assert!(super::episode(&conn, episode.episode_id).unwrap().is_some());

            assert!(tombstone_episode(&conn, episode.episode_id, 40).unwrap());
            let retained_download = commit_remove_episode(&conn, episode.episode_id).unwrap();
            assert_eq!(retained_download.as_deref(), Some("/kept/download.mp3"));
            assert!(super::episode(&conn, episode.episode_id).unwrap().is_none());
            assert_eq!(
                conn.query_row("SELECT COUNT(*) FROM podcast_episodes", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
                0
            );

            let reimport =
                upsert_episode(&conn, subscription_id, &parsed_episode("Reimported"), 50).unwrap();
            assert!(reimport.is_none());
            assert!(super::super::query::list_episodes(&conn)
                .unwrap()
                .is_empty());
            assert_eq!(
                conn.query_row(
                    "SELECT COUNT(*) FROM podcast_episode_dismissals",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
        }
    }
}
