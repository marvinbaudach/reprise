use std::cell::RefCell;
use std::path::Path;

use super::*;
use crate::podcasts::store::{self, NewSubscription};

fn conn() -> Db {
    let conn = Db::open_in_memory().unwrap();
    // These tests exercise fetch/parse/store logic, not the NET-1a gate
    // itself (see the dedicated `net_1a_*` tests below), so YouTube starts
    // enabled here.
    crate::modules::set_enabled(&conn, &crate::modules::YOUTUBE_MODULE, true).unwrap();
    conn
}

#[derive(Default)]
struct NeverYoutube;

impl YoutubeFetcher for NeverYoutube {
    fn list(&self, _: &str, _: usize) -> Result<ParsedFeed, PodcastError> {
        Err(PodcastError::YtDlp("unexpected YouTube call".to_owned()))
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        Err(PodcastError::YtDlp("unexpected YouTube call".to_owned()))
    }
}

#[test]
fn untitled_youtube_listing_uses_a_non_url_fallback_title() {
    let feed = project_youtube_feed(
        super::super::youtube::YoutubeListing {
            title: None,
            episodes: Vec::new(),
        },
        25,
    );
    assert_eq!(feed.title, "YouTube source");
}

struct OfficialYoutubeFeed {
    requested_urls: RefCell<Vec<String>>,
}

impl FeedFetcher for OfficialYoutubeFeed {
    fn fetch(&self, _: &SubscriptionRow) -> Result<Response, PodcastError> {
        panic!("YouTube refresh must request the derived official feed URL");
    }

    fn fetch_url(
        &self,
        url: &str,
        _: Option<&str>,
        _: Option<&str>,
    ) -> Result<Response, PodcastError> {
        self.requested_urls.borrow_mut().push(url.to_owned());
        Ok(Response {
            body: r#"<feed xmlns="http://www.w3.org/2005/Atom"
                          xmlns:yt="http://www.youtube.com/xml/schemas/2015">
              <title>Long-form channel</title>
              <entry><id>yt:video:newest</id><yt:videoId>newest</yt:videoId>
                <title>Newest</title><published>2026-07-28T08:00:00Z</published></entry>
              <entry><id>yt:video:older</id><yt:videoId>older</yt:videoId>
                <title>Older</title><published>2026-07-27T08:00:00Z</published></entry>
            </feed>"#
                .to_owned(),
            etag: Some("\"youtube-v1\"".to_owned()),
            last_modified: None,
        })
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        panic!("refresh without auto-download must not download")
    }
}

#[test]
fn pod_10_initial_youtube_window_uses_the_official_long_form_feed() {
    let conn = conn();
    let subscription_id = store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            feed_url: "https://www.youtube.com/channel/UCabc123".to_owned(),
            title: "Channel".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    let feed = OfficialYoutubeFeed {
        requested_urls: RefCell::new(Vec::new()),
    };
    let directory = tempfile::tempdir().unwrap();

    let summary = refresh_to_root(&conn, &feed, &NeverYoutube, 10, true, directory.path()).unwrap();

    assert_eq!(summary.episodes_inserted, 2);
    assert_eq!(
        feed.requested_urls.into_inner(),
        ["https://www.youtube.com/feeds/videos.xml?playlist_id=UULFabc123"]
    );
    assert_eq!(
        super::super::query::episodes_for_subscription(&conn, subscription_id)
            .unwrap()
            .into_iter()
            .map(|episode| episode.guid)
            .collect::<Vec<_>>(),
        ["newest", "older"]
    );
}

struct ExtendedYoutube {
    requested: RefCell<Vec<(String, usize)>>,
}

impl YoutubeFetcher for ExtendedYoutube {
    fn list(&self, _: &str, _: usize) -> Result<ParsedFeed, PodcastError> {
        panic!("Load more must use the explicit ranged listing boundary")
    }

    fn list_range(&self, url: &str, end: usize) -> Result<ParsedFeed, PodcastError> {
        self.requested.borrow_mut().push((url.to_owned(), end));
        Ok(ParsedFeed {
            title: "Channel".to_owned(),
            author: None,
            image_url: None,
            episodes: (1..=end)
                .map(|index| ParsedEpisode {
                    guid: format!("video-{index}"),
                    title: format!("Video {index}"),
                    audio_url: format!("https://www.youtube.com/watch?v=video-{index}"),
                    page_url: None,
                    published_at: None,
                    duration_secs: Some(600),
                })
                .collect(),
        })
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        panic!("Load more must not download")
    }
}

#[test]
fn pod_10_load_more_fetches_and_persists_the_first_forty_items() {
    let conn = conn();
    let subscription_id = store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            feed_url: "https://www.youtube.com/channel/UCmore".to_owned(),
            title: "Channel".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    let youtube = ExtendedYoutube {
        requested: RefCell::new(Vec::new()),
    };

    let changed = load_more_youtube(&conn, &youtube, subscription_id, 40, 20).unwrap();

    assert_eq!(changed, 40);
    assert_eq!(
        youtube.requested.into_inner(),
        [("https://www.youtube.com/channel/UCmore".to_owned(), 40)]
    );
    assert_eq!(
        super::super::query::episodes_for_subscription(&conn, subscription_id)
            .unwrap()
            .len(),
        40
    );
}

/// `NET-1a`: a YouTube subscription is skipped, not fetched, when the
/// YouTube module is off — this is issue #96's other half: Podcasts on
/// must not implicitly allow YouTube (see the RSS half in
/// `pipeline_refresh_tests.rs`).
#[test]
fn net_1a_disabled_youtube_module_skips_refresh_without_fetching() {
    let conn = conn();
    crate::modules::set_enabled(&conn, &crate::modules::YOUTUBE_MODULE, false).unwrap();
    let id = store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            feed_url: "https://www.youtube.com/channel/UCabc123".to_owned(),
            title: "Channel".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    let directory = tempfile::tempdir().unwrap();

    let summary = refresh_to_root(
        &conn,
        &FakeFeedNeverCalled,
        &NeverYoutube,
        10,
        true,
        directory.path(),
    )
    .unwrap();

    assert_eq!(summary.failed, 1);
    assert_eq!(
        store::subscription(&conn, id)
            .unwrap()
            .unwrap()
            .last_outcome
            .as_deref(),
        Some("failed")
    );
}

/// `NET-1a`: the explicit "Load more" action also respects the gate —
/// every YouTube network entry point, not just the periodic refresh.
#[test]
fn net_1a_load_more_is_blocked_when_youtube_module_is_off() {
    let conn = conn();
    let subscription_id = store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            feed_url: "https://www.youtube.com/channel/UCmore".to_owned(),
            title: "Channel".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    crate::modules::set_enabled(&conn, &crate::modules::YOUTUBE_MODULE, false).unwrap();

    let result = load_more_youtube(&conn, &NeverYoutube, subscription_id, 40, 20);

    assert!(matches!(
        result,
        Err(PipelineError::YoutubeSourceUnavailable)
    ));
}

struct FakeFeedNeverCalled;

impl FeedFetcher for FakeFeedNeverCalled {
    fn fetch(&self, _: &SubscriptionRow) -> Result<Response, PodcastError> {
        panic!("a disabled subscription must not be fetched")
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        panic!("a disabled subscription must not be downloaded")
    }
}
