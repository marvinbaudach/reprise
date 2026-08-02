use std::cell::RefCell;
use std::path::Path;

use super::*;
use crate::podcasts::store::{self, NewSubscription};

fn conn() -> Db {
    let conn = Db::open_in_memory().unwrap();
    // These tests exercise fetch/parse/store logic, not the NET-1a gate
    // itself (see the dedicated `net_1a_*` tests below), so YouTube starts
    // enabled here.
    crate::online_sources::set_enabled(&conn, true).unwrap();
    crate::modules::set_enabled(&conn, &crate::modules::YOUTUBE_MODULE, true).unwrap();
    conn
}

#[derive(Default)]
struct NeverYoutube;

impl YoutubeFetcher for NeverYoutube {
    fn list(&self, _: &str, _: usize) -> Result<ParsedFeed, PodcastError> {
        Err(PodcastError::YtDlpFailure {
            kind: crate::podcasts::ytdlp::YtDlpFailureKind::Other,
            stderr: "unexpected YouTube call".to_owned(),
        })
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        Err(PodcastError::YtDlpFailure {
            kind: crate::podcasts::ytdlp::YtDlpFailureKind::Other,
            stderr: "unexpected YouTube call".to_owned(),
        })
    }
}

#[test]
fn youtube_projection_never_uses_the_playlist_title_without_a_channel_name() {
    let feed = project_youtube_feed(
        super::super::youtube::YoutubeListing {
            title: Some("Videos".to_owned()),
            channel: None,
            episodes: Vec::new(),
        },
        25,
    );

    assert_eq!(feed.title, None);
    assert_eq!(feed.author, None);
}

#[test]
fn youtube_projection_uses_the_channel_name_instead_of_the_playlist_title() {
    let feed = project_youtube_feed(
        super::super::youtube::YoutubeListing {
            title: Some("Videos".to_owned()),
            channel: Some("Ferris Media".to_owned()),
            episodes: Vec::new(),
        },
        25,
    );

    assert_eq!(feed.title.as_deref(), Some("Ferris Media"));
    assert_eq!(feed.author.as_deref(), Some("Ferris Media"));
}

struct DatedYoutubeListing {
    published_at: i64,
}

impl YoutubeFetcher for DatedYoutubeListing {
    fn list(&self, _: &str, _: usize) -> Result<ParsedFeed, PodcastError> {
        Ok(ParsedFeed {
            title: Some("Channel".to_owned()),
            author: None,
            image_url: None,
            episodes: vec![ParsedEpisode {
                guid: "video".to_owned(),
                title: "Video".to_owned(),
                image_url: None,
                audio_url: "https://www.youtube.com/watch?v=video".to_owned(),
                page_url: None,
                published_at: Some(self.published_at),
                duration_secs: None,
            }],
        })
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        panic!("refresh without auto-download must not download")
    }
}

#[test]
fn pod_18_a_date_arriving_later_fills_an_episode_that_had_none() {
    let db = conn();
    let subscription_id = store::add_or_restore(
        &db,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            feed_url: "https://youtube.test/@dated".to_owned(),
            title: "Channel".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    let episode = ParsedEpisode {
        guid: "video".to_owned(),
        title: "Video".to_owned(),
        image_url: None,
        audio_url: "https://www.youtube.com/watch?v=video".to_owned(),
        page_url: None,
        published_at: None,
        duration_secs: None,
    };
    let episode_id = store::upsert_episode(&db, subscription_id, &episode, 10)
        .unwrap()
        .unwrap()
        .episode_id;

    let directory = tempfile::tempdir().unwrap();
    let summary = refresh_to_root(
        &db,
        &FakeFeedNeverCalled,
        &DatedYoutubeListing {
            published_at: 1_785_225_600,
        },
        20,
        true,
        directory.path(),
    )
    .unwrap();

    assert_eq!(summary.episodes_updated, 1);
    assert_eq!(
        store::episode(&db, episode_id)
            .unwrap()
            .unwrap()
            .published_at,
        Some(1_785_225_600)
    );
}

#[test]
fn pod_18_an_exact_feed_date_is_not_overwritten_by_an_approximate_one() {
    let db = conn();
    let subscription_id = store::add_or_restore(
        &db,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            feed_url: "https://youtube.test/@exact-date".to_owned(),
            title: "Channel".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    // The row already carries an exact date obtained before this approximate
    // YouTube listing. The schema stores no provenance, so non-NULL is the
    // boundary the refresh must preserve.
    let episode = ParsedEpisode {
        guid: "video".to_owned(),
        title: "Video".to_owned(),
        image_url: None,
        audio_url: "https://www.youtube.com/watch?v=video".to_owned(),
        page_url: None,
        published_at: Some(1_785_225_600),
        duration_secs: None,
    };
    let episode_id = store::upsert_episode(&db, subscription_id, &episode, 10)
        .unwrap()
        .unwrap()
        .episode_id;

    let directory = tempfile::tempdir().unwrap();
    let summary = refresh_to_root(
        &db,
        &FakeFeedNeverCalled,
        &DatedYoutubeListing {
            published_at: 1_785_312_000,
        },
        20,
        true,
        directory.path(),
    )
    .unwrap();

    assert_eq!(summary.episodes_updated, 1);
    assert_eq!(
        store::episode(&db, episode_id)
            .unwrap()
            .unwrap()
            .published_at,
        Some(1_785_225_600)
    );
}

struct OfficialYoutubeFeed {
    requested_urls: RefCell<Vec<String>>,
    author: Option<&'static str>,
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
        let author = self
            .author
            .map(|author| format!("<author><name>{author}</name></author>"))
            .unwrap_or_default();
        Ok(Response {
            body: format!(
                r#"<feed xmlns="http://www.w3.org/2005/Atom"
                          xmlns:yt="http://www.youtube.com/xml/schemas/2015">
              <title>Videos</title>{author}
              <entry><id>yt:video:newest</id><yt:videoId>newest</yt:videoId>
                <title>Newest</title><published>2026-07-28T08:00:00Z</published></entry>
              <entry><id>yt:video:older</id><yt:videoId>older</yt:videoId>
                <title>Older</title><published>2026-07-27T08:00:00Z</published></entry>
            </feed>"#
            ),
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
        author: None,
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
    assert_eq!(
        store::subscription(&conn, subscription_id)
            .unwrap()
            .unwrap()
            .title,
        "Channel",
        "a playlist title must not replace a good channel name when the feed has no author"
    );
}

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

    let summary =
        refresh_to_root(&conn, &feed, &HandleYoutube, 10, true, directory.path()).unwrap();

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
        true,
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
        true,
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

    assert_eq!(summary.failures.len(), 1);
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
