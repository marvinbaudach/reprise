//! Podcast episode queries.

use rusqlite::{params, Connection, OptionalExtension};

use super::{EpisodeRow, PodcastKind, SourceGroup};

const EPISODE_COLUMNS: &str =
    "e.id, e.subscription_id, e.guid, e.title, s.title, s.image_url, s.kind,
     e.audio_url, e.page_url, e.published_at, e.duration_secs,
     e.downloaded_path, e.downloaded_bytes, e.played_at, e.position_ms,
     e.first_seen_at";

pub fn list_episodes(conn: &Connection) -> Result<Vec<EpisodeRow>, rusqlite::Error> {
    let sql = format!(
        "SELECT {EPISODE_COLUMNS}
         FROM podcast_episodes e
         JOIN podcast_subscriptions s ON s.id = e.subscription_id
         WHERE s.removed_at IS NULL AND e.removed_at IS NULL
         ORDER BY e.published_at IS NULL, e.published_at DESC, e.first_seen_at DESC, e.id DESC"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map([], super::store::episode_from_row)?;
    rows.collect::<Result<Vec<_>, _>>()
}

pub fn episodes_for_subscription(
    conn: &Connection,
    subscription_id: i64,
) -> Result<Vec<EpisodeRow>, rusqlite::Error> {
    let sql = format!(
        "SELECT {EPISODE_COLUMNS}
         FROM podcast_episodes e
         JOIN podcast_subscriptions s ON s.id = e.subscription_id
         WHERE s.removed_at IS NULL
           AND e.removed_at IS NULL
           AND e.subscription_id = ?1
         ORDER BY e.published_at IS NULL, e.published_at DESC, e.first_seen_at DESC, e.id DESC"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map([subscription_id], super::store::episode_from_row)?;
    rows.collect::<Result<Vec<_>, _>>()
}

pub fn list_source_groups(
    conn: &Connection,
    kind: PodcastKind,
) -> Result<Vec<SourceGroup>, rusqlite::Error> {
    let subscriptions = super::store::active_subscriptions(conn)?;
    subscriptions
        .into_iter()
        .filter(|subscription| subscription.kind == kind)
        .map(|subscription| {
            Ok(SourceGroup {
                subscription_id: subscription.id,
                title: subscription.title,
                author: subscription.author,
                image_url: subscription.image_url,
                kind: subscription.kind,
                sync_to_phone: subscription.sync_to_phone,
                episodes: episodes_for_subscription(conn, subscription.id)?,
            })
        })
        .collect()
}

pub fn count_unplayed(conn: &Connection) -> Result<usize, rusqlite::Error> {
    conn.query_row(
        "SELECT COUNT(*)
         FROM podcast_episodes e
         JOIN podcast_subscriptions s ON s.id = e.subscription_id
         WHERE s.removed_at IS NULL
           AND e.removed_at IS NULL
           AND e.played_at IS NULL",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count.max(0) as usize)
}

pub fn count_unplayed_for_kind(
    conn: &Connection,
    kind: PodcastKind,
) -> Result<usize, rusqlite::Error> {
    conn.query_row(
        "SELECT COUNT(*)
         FROM podcast_episodes e
         JOIN podcast_subscriptions s ON s.id = e.subscription_id
         WHERE s.removed_at IS NULL
           AND e.removed_at IS NULL
           AND e.played_at IS NULL
           AND s.kind = ?1",
        [match kind {
            PodcastKind::Rss => "rss",
            PodcastKind::Youtube => "youtube",
        }],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count.max(0) as usize)
}

/// Returns the chronologically following unplayed episode in one show.
///
/// A dated reference only admits episodes published later than the reference.
/// An undated reference starts at the show's oldest known unplayed episode.
pub fn next_unplayed_of_show(
    conn: &Connection,
    subscription_id: i64,
    after_published_at: Option<i64>,
) -> Result<Option<EpisodeRow>, rusqlite::Error> {
    let sql = format!(
        "SELECT {EPISODE_COLUMNS}
         FROM podcast_episodes e
         JOIN podcast_subscriptions s ON s.id = e.subscription_id
         WHERE s.removed_at IS NULL
           AND e.removed_at IS NULL
           AND e.subscription_id = ?1
           AND e.played_at IS NULL
           AND (?2 IS NULL OR e.published_at > ?2)
         ORDER BY e.published_at IS NULL, e.published_at ASC, e.first_seen_at ASC, e.id ASC
         LIMIT 1"
    );
    conn.query_row(
        &sql,
        params![subscription_id, after_published_at],
        super::store::episode_from_row,
    )
    .optional()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::podcasts::feed::ParsedEpisode;
    use crate::podcasts::store::{self, NewSubscription};
    fn conn() -> Connection {
        crate::db::open_migrated(None).unwrap()
    }

    fn add_show(conn: &Connection, url: &str, title: &str) -> i64 {
        store::add_or_restore(
            conn,
            &NewSubscription {
                kind: PodcastKind::Rss,
                feed_url: url.to_owned(),
                title: title.to_owned(),
                author: None,
                image_url: None,
                auto_download: false,
            },
            1,
        )
        .unwrap()
    }

    fn add_episode(
        conn: &Connection,
        subscription_id: i64,
        guid: &str,
        published_at: Option<i64>,
    ) -> i64 {
        store::upsert_episode(
            conn,
            subscription_id,
            &ParsedEpisode {
                guid: guid.to_owned(),
                title: guid.to_owned(),
                audio_url: format!("https://example.test/{guid}.mp3"),
                page_url: None,
                published_at,
                duration_secs: None,
            },
            published_at.unwrap_or_default() + 1_000,
        )
        .unwrap()
        .expect("episode should be imported")
        .episode_id
    }

    #[test]
    fn unplayed_count_includes_new_and_resume_but_not_played_or_removed_shows() {
        let conn = conn();
        let active = add_show(&conn, "https://example.test/active", "Active");
        let removed = add_show(&conn, "https://example.test/removed", "Removed");
        let new = add_episode(&conn, active, "new", Some(10));
        let resume = add_episode(&conn, active, "resume", Some(20));
        let played = add_episode(&conn, active, "played", Some(30));
        add_episode(&conn, removed, "hidden", Some(40));
        store::save_position(&conn, resume, 500).unwrap();
        store::mark_played(&conn, played, 100).unwrap();
        store::tombstone_subscription(&conn, removed, 100).unwrap();

        assert_eq!(count_unplayed(&conn).unwrap(), 2);
        assert!(store::episode(&conn, new).unwrap().is_some());
    }

    #[test]
    fn pod_4_finish_offers_next_unplayed_of_show() {
        let conn = conn();
        let show = add_show(&conn, "https://example.test/show", "Show");
        let other = add_show(&conn, "https://example.test/other", "Other");
        add_episode(&conn, show, "before", Some(90));
        let played = add_episode(&conn, show, "played-next", Some(110));
        add_episode(&conn, other, "other-show", Some(115));
        let expected = add_episode(&conn, show, "expected", Some(120));
        add_episode(&conn, show, "later", Some(130));
        store::mark_played(&conn, played, 200).unwrap();

        let next = next_unplayed_of_show(&conn, show, Some(100))
            .unwrap()
            .unwrap();
        assert_eq!(next.id, expected);
        assert_eq!(next.title, "expected");
        assert!(next_unplayed_of_show(&conn, show, Some(130))
            .unwrap()
            .is_none());
    }

    #[test]
    fn episode_listing_sorts_undated_entries_last() {
        let conn = conn();
        let show = add_show(&conn, "https://example.test/show", "Show");
        add_episode(&conn, show, "undated", None);
        add_episode(&conn, show, "older", Some(10));
        add_episode(&conn, show, "newer", Some(20));

        let titles = list_episodes(&conn)
            .unwrap()
            .into_iter()
            .map(|episode| episode.title)
            .collect::<Vec<_>>();
        assert_eq!(titles, ["newer", "older", "undated"]);
    }

    #[test]
    fn src_5_groups_episodes_by_source_identity_and_keeps_episode_order() {
        let conn = conn();
        let first = add_show(&conn, "https://example.test/first", "Same title");
        let second = add_show(&conn, "https://example.test/second", "Same title");
        add_episode(&conn, first, "first-old", Some(10));
        add_episode(&conn, first, "first-new", Some(20));
        add_episode(&conn, second, "second", Some(30));

        let groups = list_source_groups(&conn, PodcastKind::Rss).unwrap();

        assert_eq!(groups.len(), 2);
        assert_ne!(groups[0].subscription_id, groups[1].subscription_id);
        let first_group = groups
            .iter()
            .find(|group| group.subscription_id == first)
            .unwrap();
        assert_eq!(
            first_group
                .episodes
                .iter()
                .map(|episode| episode.title.as_str())
                .collect::<Vec<_>>(),
            ["first-new", "first-old"]
        );
    }

    #[test]
    fn src_5_rss_and_youtube_groups_are_separate_library_queries() {
        let conn = conn();
        add_show(&conn, "https://example.test/rss", "RSS");
        store::add_or_restore(
            &conn,
            &NewSubscription {
                kind: PodcastKind::Youtube,
                feed_url: "https://youtube.test/@channel".into(),
                title: "YouTube".into(),
                author: None,
                image_url: None,
                auto_download: false,
            },
            1,
        )
        .unwrap();

        assert_eq!(
            list_source_groups(&conn, PodcastKind::Rss).unwrap().len(),
            1
        );
        assert_eq!(
            list_source_groups(&conn, PodcastKind::Youtube)
                .unwrap()
                .len(),
            1
        );
    }
}
