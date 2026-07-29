//! `NET-3` point 4: what the add dialog's URL path does while offline —
//! straight to a persisted subscription instead of waiting on a network
//! preview. Pure enough to need no GTK widget: the frontend's `add_dialog`
//! only translates the outcome here into status text and a callback.

use rusqlite::Connection;

use super::{discovery, store, PodcastKind};

/// While offline, a pasted URL still creates the subscription immediately
/// instead of waiting on a network preview — the title stays the URL itself
/// until the next successful refresh fetches the feed and fills it in for
/// real. `store::update_fetch_success` already `COALESCE`s the title on
/// every refresh, so nothing here has to remember to do that later.
#[must_use]
pub fn offline_subscription(
    kind: PodcastKind,
    url: &str,
    auto_download: bool,
) -> store::NewSubscription {
    store::NewSubscription {
        kind,
        feed_url: url.to_owned(),
        title: url.to_owned(),
        author: None,
        image_url: None,
        auto_download,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfflineSubscribeOutcome {
    AlreadySubscribed,
    Added { subscription_id: i64 },
}

/// The offline URL path's actual effect on the database — no preview
/// fetch, straight to a persisted subscription, or a no-op if the URL is
/// already subscribed.
pub fn offline_subscribe(
    conn: &Connection,
    kind: PodcastKind,
    url: &str,
    auto_download: bool,
) -> Result<OfflineSubscribeOutcome, rusqlite::Error> {
    let subscribed = discovery::active_source_keys(conn);
    if discovery::source_is_subscribed(kind, url, &[], &subscribed) {
        return Ok(OfflineSubscribeOutcome::AlreadySubscribed);
    }
    let subscription = offline_subscription(kind, url, auto_download);
    let subscription_id =
        store::add_or_restore(conn, &subscription, chrono::Utc::now().timestamp())?;
    Ok(OfflineSubscribeOutcome::Added { subscription_id })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let conn = crate::db::open_migrated(None).unwrap();
        crate::modules::set_enabled(&conn, &crate::modules::PODCASTS_MODULE, true).unwrap();
        crate::online_sources::set_enabled(&conn, true).unwrap();
        conn
    }

    #[test]
    fn net_3_offline_subscription_uses_the_url_as_a_placeholder_title() {
        let subscription =
            offline_subscription(PodcastKind::Rss, "https://feeds.test/show.xml", true);
        assert_eq!(subscription.kind, PodcastKind::Rss);
        assert_eq!(subscription.feed_url, "https://feeds.test/show.xml");
        assert_eq!(subscription.title, "https://feeds.test/show.xml");
        assert!(subscription.auto_download);
        assert!(subscription.author.is_none());
        assert!(subscription.image_url.is_none());
    }

    /// The decisive F4 claim: submitting a URL while offline actually
    /// creates the subscription — a real row lands in the database, not
    /// merely an insensitive-field state that a test asserting only
    /// sensitivity could never distinguish from a broken URL path.
    #[test]
    fn net_3_the_offline_url_path_persists_a_real_subscription() {
        let conn = conn();
        let url = "https://feeds.test/show.xml";

        let outcome = offline_subscribe(&conn, PodcastKind::Rss, url, false).unwrap();
        let subscription_id = match outcome {
            OfflineSubscribeOutcome::Added { subscription_id } => subscription_id,
            OfflineSubscribeOutcome::AlreadySubscribed => {
                panic!("a fresh URL must not already be subscribed")
            }
        };

        let row = store::subscription(&conn, subscription_id)
            .unwrap()
            .expect("the offline URL path must persist a real subscription row");
        assert_eq!(row.feed_url, url);
        assert_eq!(
            row.title, url,
            "the title is the URL itself until the next refresh fetches the real one"
        );
    }

    #[test]
    fn net_3_offline_subscribe_is_a_no_op_for_an_already_subscribed_url() {
        let conn = conn();
        let url = "https://feeds.test/show.xml";
        let first = offline_subscribe(&conn, PodcastKind::Rss, url, false).unwrap();
        assert!(matches!(first, OfflineSubscribeOutcome::Added { .. }));

        let second = offline_subscribe(&conn, PodcastKind::Rss, url, false).unwrap();
        assert_eq!(second, OfflineSubscribeOutcome::AlreadySubscribed);
    }
}
