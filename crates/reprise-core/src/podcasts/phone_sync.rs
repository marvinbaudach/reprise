//! Podcast source eligibility for Android synchronization (`POD-12`).
//!
//! RSS and YouTube subscriptions are equally eligible: eligibility is about
//! the subscription being active, not about its kind. Each kind lands on
//! its own device target folder (`MTP-38`) — that routing decision lives in
//! `device_sync::podcasts`, not here.

use rusqlite::{params, Connection};

use super::PodcastKind;

/// `MTP-37`: how many of the user's active subscriptions of `kind` are
/// currently selected to sync to `device_id`, out of how many exist —
/// the live counts behind the device page's Content section selection
/// summary ("N of M channels selected"). Per-item selection itself is
/// edited on the podcast/channel pages via [`set_device_enabled`], not
/// here — this is a read of that state, not a second control surface.
pub fn selection_summary(
    conn: &Connection,
    device_id: &str,
    kind: PodcastKind,
) -> Result<(usize, usize), rusqlite::Error> {
    let kind = match kind {
        PodcastKind::Rss => "rss",
        PodcastKind::Youtube => "youtube",
    };
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM podcast_subscriptions
         WHERE kind = ?1 AND removed_at IS NULL",
        params![kind],
        |row| row.get(0),
    )?;
    let selected: i64 = conn.query_row(
        "SELECT COUNT(*) FROM podcast_subscriptions s
         JOIN podcast_subscription_devices d ON d.subscription_id = s.id
         WHERE s.kind = ?1 AND s.removed_at IS NULL AND d.device_id = ?2",
        params![kind, device_id],
        |row| row.get(0),
    )?;
    Ok((
        usize::try_from(selected).unwrap_or(0),
        usize::try_from(total).unwrap_or(0),
    ))
}

pub fn set_device_enabled(
    conn: &Connection,
    subscription_id: i64,
    device_id: &str,
    enabled: bool,
) -> Result<bool, rusqlite::Error> {
    let device_id = device_id.trim();
    if device_id.is_empty() {
        return Ok(false);
    }
    let transaction = conn.unchecked_transaction()?;
    let eligible = transaction.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM podcast_subscriptions
           WHERE id = ?1 AND removed_at IS NULL
         )",
        [subscription_id],
        |row| row.get::<_, bool>(0),
    )?;
    if !eligible {
        return Ok(false);
    }
    if enabled {
        transaction.execute(
            "INSERT OR IGNORE INTO podcast_subscription_devices
             (subscription_id, device_id) VALUES (?1, ?2)",
            params![subscription_id, device_id],
        )?;
    } else {
        transaction.execute(
            "DELETE FROM podcast_subscription_devices
             WHERE subscription_id = ?1 AND device_id = ?2",
            params![subscription_id, device_id],
        )?;
    }
    transaction.execute(
        "UPDATE podcast_subscriptions
         SET sync_to_phone = EXISTS(
           SELECT 1 FROM podcast_subscription_devices
           WHERE subscription_id = ?1
         )
         WHERE id = ?1",
        [subscription_id],
    )?;
    transaction.commit()?;
    Ok(true)
}

pub fn selected_device_ids(
    conn: &Connection,
    subscription_id: i64,
) -> Result<Vec<String>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT d.device_id
         FROM podcast_subscription_devices d
         JOIN podcast_subscriptions s ON s.id = d.subscription_id
         WHERE d.subscription_id = ?1
           AND s.removed_at IS NULL
         ORDER BY d.device_id",
    )?;
    let rows = statement.query_map([subscription_id], |row| row.get(0))?;
    rows.collect()
}

pub fn set_enabled(
    conn: &Connection,
    subscription_id: i64,
    enabled: bool,
) -> Result<bool, rusqlite::Error> {
    Ok(conn.execute(
        "UPDATE podcast_subscriptions
         SET sync_to_phone = ?2
         WHERE id = ?1 AND removed_at IS NULL",
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
    fn mtp_37_selection_summary_counts_only_this_devices_selected_channels() {
        let conn = crate::db::open_migrated(None).unwrap();
        let one = store::add_or_restore(
            &conn,
            &subscription(PodcastKind::Youtube, "https://youtube.test/@one"),
            10,
        )
        .unwrap();
        let two = store::add_or_restore(
            &conn,
            &subscription(PodcastKind::Youtube, "https://youtube.test/@two"),
            10,
        )
        .unwrap();
        // A removed subscription must not inflate the total.
        let removed = store::add_or_restore(
            &conn,
            &subscription(PodcastKind::Youtube, "https://youtube.test/@gone"),
            10,
        )
        .unwrap();
        store::tombstone_subscription(&conn, removed, 1).unwrap();

        assert_eq!(
            selection_summary(&conn, "mtp:pixel", PodcastKind::Youtube).unwrap(),
            (0, 2),
            "no channel selected for this device yet"
        );

        assert!(set_device_enabled(&conn, one, "mtp:pixel", true).unwrap());
        assert_eq!(
            selection_summary(&conn, "mtp:pixel", PodcastKind::Youtube).unwrap(),
            (1, 2),
            "enabling one channel for this device changes the live count"
        );

        assert!(set_device_enabled(&conn, two, "mtp:pixel", true).unwrap());
        assert_eq!(
            selection_summary(&conn, "mtp:pixel", PodcastKind::Youtube).unwrap(),
            (2, 2)
        );

        // A different device's selection must not leak in.
        assert_eq!(
            selection_summary(&conn, "mtp:tablet", PodcastKind::Youtube).unwrap(),
            (0, 2)
        );

        assert!(set_device_enabled(&conn, one, "mtp:pixel", false).unwrap());
        assert_eq!(
            selection_summary(&conn, "mtp:pixel", PodcastKind::Youtube).unwrap(),
            (1, 2),
            "disabling a channel changes the live count back"
        );
    }

    #[test]
    fn pod_12_rss_and_youtube_subscriptions_can_equally_sync_to_phone() {
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
        assert!(set_enabled(&conn, youtube, true).unwrap());
        assert!(
            store::subscription(&conn, rss)
                .unwrap()
                .unwrap()
                .sync_to_phone
        );
        assert!(
            store::subscription(&conn, youtube)
                .unwrap()
                .unwrap()
                .sync_to_phone
        );
    }

    #[test]
    fn pod_12_restoring_a_source_under_a_different_kind_clears_phone_sync() {
        let conn = crate::db::open_migrated(None).unwrap();
        let source = store::add_or_restore(
            &conn,
            &subscription(PodcastKind::Rss, "https://example.test/source"),
            10,
        )
        .unwrap();
        assert!(set_enabled(&conn, source, true).unwrap());
        assert!(set_device_enabled(&conn, source, "mtp:pixel", true).unwrap());

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

        store::add_or_restore(
            &conn,
            &subscription(PodcastKind::Rss, "https://example.test/source"),
            30,
        )
        .unwrap();
        assert!(selected_device_ids(&conn, source).unwrap().is_empty());
    }

    #[test]
    fn pod_12_rss_phone_sync_selection_is_persisted_per_stable_device() {
        let conn = crate::db::open_migrated(None).unwrap();
        let rss = store::add_or_restore(
            &conn,
            &subscription(PodcastKind::Rss, "https://example.test/feed"),
            10,
        )
        .unwrap();

        assert!(set_device_enabled(&conn, rss, "mtp:pixel", true).unwrap());
        assert!(set_device_enabled(&conn, rss, "mtp:tablet", true).unwrap());
        assert_eq!(
            selected_device_ids(&conn, rss).unwrap(),
            ["mtp:pixel".to_owned(), "mtp:tablet".to_owned()]
        );

        assert!(set_device_enabled(&conn, rss, "mtp:pixel", false).unwrap());
        assert_eq!(
            selected_device_ids(&conn, rss).unwrap(),
            ["mtp:tablet".to_owned()]
        );
    }

    #[test]
    fn pod_12_youtube_persists_device_selection_just_like_rss() {
        let conn = crate::db::open_migrated(None).unwrap();
        let youtube = store::add_or_restore(
            &conn,
            &subscription(PodcastKind::Youtube, "https://youtube.test/@channel"),
            10,
        )
        .unwrap();

        assert!(set_device_enabled(&conn, youtube, "mtp:pixel", true).unwrap());
        assert_eq!(
            selected_device_ids(&conn, youtube).unwrap(),
            ["mtp:pixel".to_owned()]
        );

        assert!(set_device_enabled(&conn, youtube, "mtp:pixel", false).unwrap());
        assert!(selected_device_ids(&conn, youtube).unwrap().is_empty());
    }
}
