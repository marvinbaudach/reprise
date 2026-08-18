//! Which listing boundary a refresh uses, and how far "Load more" reaches.

use std::cell::RefCell;
use std::path::Path;

use super::youtube_test_support::*;
use super::{RefreshRequest as R, *};
use crate::podcasts::store::{self, NewSubscription};

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
        author: None,
    };
    let directory = tempfile::tempdir().unwrap();

    let summary = refresh_to_root(
        &conn,
        &feed,
        &NeverYoutube,
        10,
        R::force(),
        directory.path(),
    )
    .unwrap();

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
    assert_eq!(
        store::subscription(&conn, subscription_id)
            .unwrap()
            .unwrap()
            .title,
        "Channel",
        "a playlist title must not replace a good channel name when the feed has no author"
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
            title: Some("Channel".to_owned()),
            author: None,
            image_url: None,
            episodes: (1..=end)
                .map(|index| ParsedEpisode {
                    guid: format!("video-{index}"),
                    title: format!("Video {index}"),
                    image_url: None,
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
