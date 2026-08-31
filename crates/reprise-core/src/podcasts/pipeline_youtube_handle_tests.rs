//! Resolving a @handle subscription to a channel before the refresh runs.

use std::cell::RefCell;
use std::path::Path;

use super::youtube_test_support::*;
use super::{RefreshRequest as R, *};
use crate::podcasts::store::{self, NewSubscription};

struct HandleYoutube;

impl YoutubeFetcher for HandleYoutube {
    fn resolve_channel_url(&self, url: &str) -> Result<Option<String>, PodcastError> {
        assert_eq!(url, "https://www.youtube.com/@show");
        Ok(Some(
            "https://www.youtube.com/channel/UCresolved".to_owned(),
        ))
    }

    fn list(&self, _: &str, _: usize) -> Result<ParsedFeed, PodcastError> {
        panic!("a resolved channel identity must use the official Atom feed")
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        panic!("refresh without auto-download must not download")
    }
}

#[test]
fn handle_subscription_resolves_channel_identity_before_refresh() {
    let conn = conn();
    let subscription_id = store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            feed_url: "https://www.youtube.com/@show".to_owned(),
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
        author: Some("Renamed Channel"),
    };
    let directory = tempfile::tempdir().unwrap();

    let summary = refresh_to_root(
        &conn,
        &feed,
        &HandleYoutube,
        10,
        R::force(),
        directory.path(),
    )
    .unwrap();

    assert_eq!(summary.episodes_inserted, 2);
    assert_eq!(
        feed.requested_urls.into_inner(),
        ["https://www.youtube.com/feeds/videos.xml?playlist_id=UULFresolved"]
    );
    assert_eq!(
        store::subscription(&conn, subscription_id)
            .unwrap()
            .unwrap()
            .feed_url,
        "https://www.youtube.com/channel/UCresolved"
    );
    assert_eq!(
        store::subscription(&conn, subscription_id)
            .unwrap()
            .unwrap()
            .title,
        "Renamed Channel"
    );
    let episodes = super::super::query::episodes_for_subscription(&conn, subscription_id).unwrap();
    assert!(episodes
        .iter()
        .all(|episode| episode.published_at.is_some()));
}

struct UnresolvableHandle;

impl YoutubeFetcher for UnresolvableHandle {
    fn resolve_channel_url(&self, _: &str) -> Result<Option<String>, PodcastError> {
        Err(PodcastError::YtDlpFailure {
            kind: crate::podcasts::ytdlp::YtDlpFailureKind::HelperMissing,
            stderr: "yt-dlp unavailable".to_owned(),
        })
    }

    fn list(&self, _: &str, _: usize) -> Result<ParsedFeed, PodcastError> {
        Err(PodcastError::YtDlpFailure {
            kind: crate::podcasts::ytdlp::YtDlpFailureKind::HelperMissing,
            stderr: "yt-dlp unavailable".to_owned(),
        })
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        panic!("refresh without auto-download must not download")
    }
}

struct PlainRssFeed;

impl FeedFetcher for PlainRssFeed {
    fn fetch(&self, _: &SubscriptionRow) -> Result<Response, PodcastError> {
        Ok(Response {
            body: r#"<rss><channel><title>Show</title><item>
              <title>Episode</title><guid>rss-1</guid>
              <enclosure url="https://example.test/rss-1.mp3" type="audio/mpeg"/>
            </item></channel></rss>"#
                .to_owned(),
            etag: None,
            last_modified: None,
        })
    }

    fn fetch_url(
        &self,
        _: &str,
        _: Option<&str>,
        _: Option<&str>,
    ) -> Result<Response, PodcastError> {
        Err(PodcastError::Transport("no official feed here".to_owned()))
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        panic!("refresh without auto-download must not download")
    }
}

/// A channel identity that cannot be resolved is one subscription's failure,
/// exactly like a fetch or parse failure: it is recorded on that subscription
/// and the batch carries on. Propagating it out of the loop instead — which is
/// what `?` on `resolve_channel_url` did — aborted the whole refresh cycle for
/// every other subscription and never even recorded the failure on the broken
/// one. `resolve_channel_url` calls a yt-dlp subprocess, so a transient failure
/// there is the expected case, not an exceptional one.
#[test]
fn a_channel_that_cannot_be_resolved_fails_alone_and_never_aborts_the_batch() {
    let conn = conn();
    crate::modules::set_enabled(&conn, &crate::modules::PODCASTS_MODULE, true).unwrap();
    let youtube_id = store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            feed_url: "https://www.youtube.com/@show".to_owned(),
            title: "Channel".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    let rss_id = store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Rss,
            feed_url: "https://example.test/feed.xml".to_owned(),
            title: "Show".to_owned(),
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
        &PlainRssFeed,
        &UnresolvableHandle,
        10,
        R::force(),
        directory.path(),
    )
    .expect("one unresolvable channel must not abort the whole refresh");

    assert_eq!(summary.attempted, 2);
    assert_eq!(
        summary.failures,
        [RefreshFailure {
            subscription_id: youtube_id,
            title: "Channel".to_owned(),
            kind: crate::source_error::SourceErrorKind::HelperOutdated,
            classified_cause: crate::podcasts::ytdlp::YtDlpFailureKind::HelperMissing
                .user_message(),
        }]
    );
    assert_eq!(summary.failed, summary.failures.len());
    assert_eq!(summary.episodes_inserted, 1);
    assert_eq!(
        super::super::query::episodes_for_subscription(&conn, rss_id)
            .unwrap()
            .len(),
        1,
        "the healthy subscription still refreshes"
    );
}

struct UnavailableOfficialFeed;

impl FeedFetcher for UnavailableOfficialFeed {
    fn fetch(&self, _: &SubscriptionRow) -> Result<Response, PodcastError> {
        panic!("YouTube refresh must not use the subscription fetch boundary")
    }

    fn fetch_url(
        &self,
        _: &str,
        _: Option<&str>,
        _: Option<&str>,
    ) -> Result<Response, PodcastError> {
        Err(PodcastError::Transport(
            "fixture official feed unavailable".to_owned(),
        ))
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        panic!("refresh without auto-download must not download")
    }
}

struct UnchangedOfficialFeed;

impl FeedFetcher for UnchangedOfficialFeed {
    fn fetch(&self, _: &SubscriptionRow) -> Result<Response, PodcastError> {
        panic!("YouTube refresh must not use the subscription fetch boundary")
    }

    fn fetch_url(
        &self,
        _: &str,
        _: Option<&str>,
        _: Option<&str>,
    ) -> Result<Response, PodcastError> {
        Err(PodcastError::NotModified)
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        panic!("refresh without auto-download must not download")
    }
}

#[test]
fn resolved_handle_is_adopted_when_the_official_feed_is_not_modified() {
    let conn = conn();
    let subscription_id = store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            feed_url: "https://www.youtube.com/@show".to_owned(),
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
        &UnchangedOfficialFeed,
        &HandleYoutube,
        10,
        R::force(),
        directory.path(),
    )
    .unwrap();

    assert_eq!(summary.not_modified, 1);
    assert_eq!(
        store::subscription(&conn, subscription_id)
            .unwrap()
            .unwrap()
            .feed_url,
        "https://www.youtube.com/channel/UCresolved"
    );
}

struct DatedFlatPlaylist;

impl YoutubeFetcher for DatedFlatPlaylist {
    fn resolve_channel_url(&self, _: &str) -> Result<Option<String>, PodcastError> {
        Ok(Some(
            "https://www.youtube.com/channel/UCresolved".to_owned(),
        ))
    }

    fn list(&self, _: &str, _: usize) -> Result<ParsedFeed, PodcastError> {
        Ok(ParsedFeed {
            title: Some("Channel".to_owned()),
            author: None,
            image_url: None,
            episodes: vec![ParsedEpisode {
                guid: "fallback".to_owned(),
                title: "Fallback".to_owned(),
                image_url: Some("https://img.test/fallback.jpg".to_owned()),
                audio_url: "https://www.youtube.com/watch?v=fallback".to_owned(),
                page_url: None,
                published_at: Some(1_785_369_600),
                duration_secs: None,
            }],
        })
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        panic!("refresh without auto-download must not download")
    }
}

#[test]
fn resolved_handle_falls_back_to_dated_flat_playlist_when_atom_is_unavailable() {
    let conn = conn();
    let subscription_id = store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            feed_url: "https://www.youtube.com/@show".to_owned(),
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
        &UnavailableOfficialFeed,
        &DatedFlatPlaylist,
        10,
        R::force(),
        directory.path(),
    )
    .unwrap();

    assert_eq!(summary.episodes_inserted, 1);
    let episode = super::super::query::episodes_for_subscription(&conn, subscription_id)
        .unwrap()
        .remove(0);
    assert_eq!(episode.published_at, Some(1_785_369_600));
    assert_eq!(
        episode.image_url.as_deref(),
        Some("https://img.test/fallback.jpg")
    );
}
