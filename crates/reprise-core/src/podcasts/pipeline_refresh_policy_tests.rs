use std::{cell::Cell, cell::RefCell, path::Path};

use super::*;
use crate::podcasts::{
    refresh::RefreshRequest,
    store::{self, NewSubscription},
};

struct FakeFeed {
    responses: RefCell<Vec<Result<Response, PodcastError>>>,
}

impl FeedFetcher for FakeFeed {
    fn fetch(&self, _: &SubscriptionRow) -> Result<Response, PodcastError> {
        self.responses.borrow_mut().remove(0)
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        panic!("refresh without auto-download must not download")
    }
}

#[derive(Default)]
struct CountingYoutube {
    calls: Cell<usize>,
}

impl YoutubeFetcher for CountingYoutube {
    fn resolve_channel_url(&self, _: &str) -> Result<Option<String>, PodcastError> {
        self.calls.set(self.calls.get() + 1);
        Ok(None)
    }

    fn list(&self, _: &str, _: usize) -> Result<ParsedFeed, PodcastError> {
        self.calls.set(self.calls.get() + 1);
        Ok(ParsedFeed {
            title: Some("Channel".to_owned()),
            author: None,
            image_url: None,
            episodes: Vec::new(),
        })
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        panic!("refresh without auto-download must not download")
    }
}

fn conn() -> Db {
    let conn = Db::open_in_memory().unwrap();
    crate::online_sources::set_enabled(&conn, true).unwrap();
    crate::modules::set_enabled(&conn, &crate::modules::PODCASTS_MODULE, true).unwrap();
    crate::modules::set_enabled(&conn, &crate::modules::YOUTUBE_MODULE, true).unwrap();
    conn
}

fn add_subscription(conn: &Db, kind: PodcastKind, url: &str) -> i64 {
    store::add_or_restore(
        conn,
        &NewSubscription {
            kind,
            feed_url: url.to_owned(),
            title: "Source".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap()
}

fn feed_response() -> Response {
    Response {
        body: "<rss><channel><title>Show</title></channel></rss>".to_owned(),
        etag: None,
        last_modified: None,
    }
}

#[test]
fn rss_scope_never_touches_a_youtube_subscription() {
    let conn = conn();
    add_subscription(&conn, PodcastKind::Rss, "https://example.test/feed");
    let youtube_id = add_subscription(
        &conn,
        PodcastKind::Youtube,
        "https://www.youtube.com/@kontrolle",
    );
    let feed = FakeFeed {
        responses: RefCell::new(vec![Ok(feed_response())]),
    };
    let youtube = CountingYoutube::default();
    let directory = tempfile::tempdir().unwrap();

    let summary = refresh_to_root(
        &conn,
        &feed,
        &youtube,
        1_000,
        RefreshRequest::force().with_kind(Some(PodcastKind::Rss)),
        directory.path(),
    )
    .unwrap();

    assert_eq!(summary.attempted, 1);
    assert_eq!(youtube.calls.get(), 0);
    assert_eq!(
        store::subscription(&conn, youtube_id)
            .unwrap()
            .unwrap()
            .last_fetch_at,
        None
    );
}

#[test]
fn youtube_scope_never_touches_an_rss_subscription() {
    let conn = conn();
    let rss_id = add_subscription(&conn, PodcastKind::Rss, "https://example.test/feed");
    add_subscription(
        &conn,
        PodcastKind::Youtube,
        "https://www.youtube.com/@kontrolle",
    );
    let feed = FakeFeed {
        responses: RefCell::new(Vec::new()),
    };
    let youtube = CountingYoutube::default();
    let directory = tempfile::tempdir().unwrap();

    let summary = refresh_to_root(
        &conn,
        &feed,
        &youtube,
        1_000,
        RefreshRequest::force().with_kind(Some(PodcastKind::Youtube)),
        directory.path(),
    )
    .unwrap();

    assert_eq!(summary.attempted, 1);
    assert!(youtube.calls.get() > 0);
    assert_eq!(
        store::subscription(&conn, rss_id)
            .unwrap()
            .unwrap()
            .last_fetch_at,
        None
    );
}

#[test]
fn stale_for_below_the_threshold_fetches_nothing_and_above_it_fetches() {
    let conn = conn();
    add_subscription(&conn, PodcastKind::Rss, "https://example.test/feed");
    let feed = FakeFeed {
        responses: RefCell::new(vec![Ok(feed_response()), Ok(feed_response())]),
    };
    let directory = tempfile::tempdir().unwrap();

    let first = refresh_to_root(
        &conn,
        &feed,
        &CountingYoutube::default(),
        1_000,
        RefreshRequest::force(),
        directory.path(),
    )
    .unwrap();
    let fresh = refresh_to_root(
        &conn,
        &feed,
        &CountingYoutube::default(),
        1_899,
        RefreshRequest::stale_for(900, Some(PodcastKind::Rss)),
        directory.path(),
    )
    .unwrap();
    let stale = refresh_to_root(
        &conn,
        &feed,
        &CountingYoutube::default(),
        1_900,
        RefreshRequest::stale_for(900, Some(PodcastKind::Rss)),
        directory.path(),
    )
    .unwrap();

    assert_eq!(first.attempted, 1);
    assert_eq!(fresh.attempted, 0);
    assert_eq!(stale.attempted, 1);
}

#[test]
fn stale_for_respects_an_open_retry_backoff() {
    // Keep this database alive for the complete test. Retry keys contain the
    // connection address, so replacing it mid-test could inherit stale state.
    let conn = conn();
    add_subscription(&conn, PodcastKind::Rss, "https://example.test/retry");
    let feed = FakeFeed {
        responses: RefCell::new(vec![
            Err(PodcastError::Transport("connection reset".to_owned())),
            Ok(feed_response()),
        ]),
    };
    let directory = tempfile::tempdir().unwrap();

    let failed = refresh_to_root(
        &conn,
        &feed,
        &CountingYoutube::default(),
        1_000,
        RefreshRequest::due(),
        directory.path(),
    )
    .unwrap();
    assert_eq!(failed.attempted, 1);
    assert_eq!(
        store::active_subscriptions(&conn).unwrap()[0]
            .last_outcome
            .as_deref(),
        Some("failed")
    );

    let waiting = refresh_to_root(
        &conn,
        &feed,
        &CountingYoutube::default(),
        1_001,
        RefreshRequest::stale_for(1, Some(PodcastKind::Rss)),
        directory.path(),
    )
    .unwrap();
    let retried = refresh_to_root(
        &conn,
        &feed,
        &CountingYoutube::default(),
        1_002,
        RefreshRequest::stale_for(1, Some(PodcastKind::Rss)),
        directory.path(),
    )
    .unwrap();

    assert_eq!(waiting.attempted, 0);
    assert_eq!(retried.attempted, 1);
}
