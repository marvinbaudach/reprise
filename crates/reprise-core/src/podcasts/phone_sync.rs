//! Podcast source eligibility for Android synchronization.

use rusqlite::{params, Connection};

pub fn set_enabled(
    conn: &Connection,
    subscription_id: i64,
    enabled: bool,
) -> Result<bool, rusqlite::Error> {
    Ok(conn.execute(
        "UPDATE podcast_subscriptions
         SET sync_to_phone = ?2
         WHERE id = ?1 AND removed_at IS NULL AND kind = 'rss'",
        params![subscription_id, enabled],
    )? != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::podcasts::store::{self, NewSubscription};
    use crate::podcasts::PodcastKind;

    fn subscription(kind: PodcastKind, url: &str) -> NewSubscription {
        NewSubscription {
            kind,
            feed_url: url.into(),
            title: "Source".into(),
            author: None,
            image_url: None,
            auto_download: false,
        }
    }

    #[test]
    fn pod_8_only_rss_subscriptions_can_sync_to_phone() {
        let conn = crate::db::open_migrated(None).unwrap();
        let rss = store::add_or_restore(
            &conn,
            &subscription(PodcastKind::Rss, "https://example.test/feed"),
            10,
        )
        .unwrap();
        let youtube = store::add_or_restore(
            &conn,
            &subscription(PodcastKind::Youtube, "https://youtube.test/@channel"),
            10,
        )
        .unwrap();

        assert!(set_enabled(&conn, rss, true).unwrap());
        assert!(!set_enabled(&conn, youtube, true).unwrap());
        assert!(
            store::subscription(&conn, rss)
                .unwrap()
                .unwrap()
                .sync_to_phone
        );
        assert!(
            !store::subscription(&conn, youtube)
                .unwrap()
                .unwrap()
                .sync_to_phone
        );
    }

    #[test]
    fn pod_8_restoring_a_source_as_youtube_clears_phone_sync() {
        let conn = crate::db::open_migrated(None).unwrap();
        let source = store::add_or_restore(
            &conn,
            &subscription(PodcastKind::Rss, "https://example.test/source"),
            10,
        )
        .unwrap();
        assert!(set_enabled(&conn, source, true).unwrap());

        let restored = store::add_or_restore(
            &conn,
            &subscription(PodcastKind::Youtube, "https://example.test/source"),
            20,
        )
        .unwrap();

        assert_eq!(restored, source);
        assert_eq!(
            store::subscription(&conn, source).unwrap().unwrap().kind,
            PodcastKind::Youtube
        );
        assert!(
            !store::subscription(&conn, source)
                .unwrap()
                .unwrap()
                .sync_to_phone
        );
    }
}
