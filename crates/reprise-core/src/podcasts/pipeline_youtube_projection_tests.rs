//! What a YouTube refresh projects into the store: channel image, title, episode dates.

use std::path::Path;

use super::youtube_test_support::*;
use super::{RefreshRequest as R, *};
use crate::podcasts::store::{self, NewSubscription};

/// This path fetches a *channel* URL through yt-dlp, where `title` is the
/// channel's own name — the useless "Videos" title belongs to the uploads RSS
/// feed, which is a different path and is stripped by
/// `youtube::subscription_title`. So when yt-dlp omits `channel`/`uploader`,
/// falling back to its title is right; falling through to `None` made a freshly
/// added channel take its own URL as a name (caught by the MCP
/// `adds_and_imports_a_youtube_channel_through_ytdlp` test).
/// The reported bug, at the projection: a channel picture used to be dropped
/// here (`image_url: None` was a constant), so `podcast_subscriptions.image_url`
/// could never hold anything but what the add dialog had persisted.
#[test]
fn youtube_projection_carries_the_channel_image_through() {
    let feed = project_youtube_feed(
        super::super::youtube::YoutubeListing {
            title: Some("VOID PREACHER".to_owned()),
            channel: Some("VOID PREACHER".to_owned()),
            image_url: Some("https://yt3.googleusercontent.com/ytc/AIdro=s900".to_owned()),
            episodes: vec![super::super::youtube::YoutubeEpisode {
                guid: "video-1".to_owned(),
                title: "Newest video".to_owned(),
                audio_url: "https://www.youtube.com/watch?v=video-1".to_owned(),
                published_at: Some(200),
                duration_secs: Some(60),
                image_url: Some("https://i.ytimg.com/vi/video-1/hq720.jpg".to_owned()),
            }],
        },
        25,
    );

    assert_eq!(
        feed.image_url.as_deref(),
        Some("https://yt3.googleusercontent.com/ytc/AIdro=s900")
    );
    // The episode keeps its own picture; the two never swap roles.
    assert_eq!(
        feed.episodes[0].image_url.as_deref(),
        Some("https://i.ytimg.com/vi/video-1/hq720.jpg")
    );
}

/// The regression test for the reported bug itself: the stored channel picture
/// must not follow whatever the channel published last. Starts from the exact
/// state measured in the live database on 2026-08-18 — a video thumbnail in
/// `image_url`, written by the add dialog.
struct ChannelWithNewestVideo {
    newest: &'static str,
    published_at: i64,
}

impl YoutubeFetcher for ChannelWithNewestVideo {
    fn list(&self, _: &str, _: usize) -> Result<ParsedFeed, PodcastError> {
        Ok(ParsedFeed {
            title: Some("VOID PREACHER".to_owned()),
            author: Some("VOID PREACHER".to_owned()),
            image_url: Some("https://yt3.googleusercontent.com/ytc/AIdro=s900".to_owned()),
            episodes: vec![ParsedEpisode {
                guid: self.newest.to_owned(),
                title: format!("Video {}", self.newest),
                image_url: Some(format!("https://i.ytimg.com/vi/{}/hq720.jpg", self.newest)),
                audio_url: format!("https://www.youtube.com/watch?v={}", self.newest),
                page_url: None,
                published_at: Some(self.published_at),
                duration_secs: Some(60),
            }],
        })
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        panic!("this test never downloads");
    }
}

#[test]
fn a_new_video_does_not_change_the_stored_channel_image() {
    let db = conn();
    let subscription_id = store::add_or_restore(
        &db,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            feed_url: "https://www.youtube.com/channel/UC-void".to_owned(),
            title: "VOID PREACHER".to_owned(),
            author: None,
            image_url: Some("https://i.ytimg.com/vi/old-video/hq720.jpg".to_owned()),
            auto_download: false,
        },
        1,
    )
    .unwrap();

    let directory = tempfile::tempdir().unwrap();
    let stored_image = || {
        store::active_subscriptions(&db)
            .unwrap()
            .into_iter()
            .find(|row| row.id == subscription_id)
            .unwrap()
            .image_url
    };
    let avatar = Some("https://yt3.googleusercontent.com/ytc/AIdro=s900".to_owned());

    refresh_to_root(
        &db,
        &FakeFeedNeverCalled,
        &ChannelWithNewestVideo {
            newest: "video-1",
            published_at: 100,
        },
        20,
        R::force(),
        directory.path(),
    )
    .unwrap();
    // The video thumbnail the add dialog had stored is replaced by the avatar.
    assert_eq!(stored_image(), avatar);

    refresh_to_root(
        &db,
        &FakeFeedNeverCalled,
        &ChannelWithNewestVideo {
            newest: "video-2",
            published_at: 200,
        },
        20,
        R::force(),
        directory.path(),
    )
    .unwrap();
    // The channel published something newer; the avatar stays put. Before the
    // fix the group header followed the newest episode instead.
    assert_eq!(stored_image(), avatar);
}

#[test]
fn youtube_projection_falls_back_to_the_listing_title_without_a_channel_name() {
    let feed = project_youtube_feed(
        super::super::youtube::YoutubeListing {
            title: Some("HOLLOW FALLEN".to_owned()),
            channel: None,
            image_url: None,
            episodes: Vec::new(),
        },
        25,
    );

    assert_eq!(feed.title.as_deref(), Some("HOLLOW FALLEN"));
    assert_eq!(feed.author.as_deref(), Some("HOLLOW FALLEN"));
}

#[test]
fn youtube_projection_without_any_name_leaves_the_stored_title_alone() {
    let feed = project_youtube_feed(
        super::super::youtube::YoutubeListing {
            title: None,
            channel: None,
            image_url: None,
            episodes: Vec::new(),
        },
        25,
    );

    assert_eq!(feed.title, None, "a nameless listing must not overwrite");
    assert_eq!(feed.author, None);
}

/// The RSS side is where "Videos" actually comes from, and it is dropped there:
/// only a non-empty author becomes the subscription title.
#[test]
fn youtube_rss_refresh_never_promotes_the_playlist_title() {
    use super::super::feed::ParsedFeed;

    let videos_feed = ParsedFeed {
        title: Some("Videos".to_owned()),
        author: Some("HOLLOW FALLEN".to_owned()),
        image_url: None,
        episodes: Vec::new(),
    };
    assert_eq!(
        super::super::youtube::subscription_title(&videos_feed),
        Some("HOLLOW FALLEN")
    );

    let authorless = ParsedFeed {
        title: Some("Videos".to_owned()),
        author: Some("   ".to_owned()),
        image_url: None,
        episodes: Vec::new(),
    };
    assert_eq!(
        super::super::youtube::subscription_title(&authorless),
        None,
        "a blank author must not rename the subscription to \"Videos\""
    );
}

#[test]
fn youtube_projection_uses_the_channel_name_instead_of_the_playlist_title() {
    let feed = project_youtube_feed(
        super::super::youtube::YoutubeListing {
            title: Some("Videos".to_owned()),
            channel: Some("Ferris Media".to_owned()),
            image_url: None,
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
        R::force(),
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
        R::force(),
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
