//! YouTube refresh coverage for filling episode runtimes from bounded listings.

use std::cell::Cell;
use std::path::Path;

use super::youtube_test_support::*;
use super::{RefreshRequest as R, *};
use crate::podcasts::store::{self, NewSubscription};

const VIDEO_ID: &str = "abcdefghijk";
const CHANNEL_URL: &str = "https://www.youtube.com/channel/UCduration";

unsafe extern "C" fn cancel_sync_at_commit(context: *mut std::ffi::c_void) -> i32 {
    // SAFETY: The test keeps this `SyncAbort` alive until it removes the hook.
    let abort = unsafe { &*context.cast::<SyncAbort>() };
    abort.cancel();
    0
}

struct FixedFeed {
    body: String,
}

impl FeedFetcher for FixedFeed {
    fn fetch(&self, _: &SubscriptionRow) -> Result<Response, PodcastError> {
        Ok(response(&self.body))
    }

    fn fetch_url(
        &self,
        _: &str,
        _: Option<&str>,
        _: Option<&str>,
    ) -> Result<Response, PodcastError> {
        Ok(response(&self.body))
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        panic!("refresh without auto-download must not download")
    }
}

enum ListingResult {
    Duration(i64),
    Failure,
}

struct CountingYoutube {
    calls: Cell<usize>,
    guid: &'static str,
    result: ListingResult,
}

impl CountingYoutube {
    fn duration(guid: &'static str, duration_secs: i64) -> Self {
        Self {
            calls: Cell::new(0),
            guid,
            result: ListingResult::Duration(duration_secs),
        }
    }

    fn failure(guid: &'static str) -> Self {
        Self {
            calls: Cell::new(0),
            guid,
            result: ListingResult::Failure,
        }
    }
}

impl YoutubeFetcher for CountingYoutube {
    fn list(&self, _: &str, _: usize) -> Result<ParsedFeed, PodcastError> {
        panic!("duration filling must use the bounded listing primitive")
    }

    fn list_range(&self, _: &str, end: usize) -> Result<ParsedFeed, PodcastError> {
        assert_eq!(end, 200);
        self.calls.set(self.calls.get() + 1);
        match self.result {
            ListingResult::Duration(duration_secs) => Ok(ParsedFeed {
                title: Some("Channel".to_owned()),
                author: Some("Channel".to_owned()),
                image_url: None,
                episodes: vec![ParsedEpisode {
                    guid: self.guid.to_owned(),
                    title: "Listed episode".to_owned(),
                    image_url: None,
                    audio_url: format!("https://www.youtube.com/watch?v={}", self.guid),
                    page_url: None,
                    published_at: None,
                    duration_secs: Some(duration_secs),
                }],
            }),
            ListingResult::Failure => Err(PodcastError::YtDlpFailure {
                kind: crate::podcasts::ytdlp::YtDlpFailureKind::Other,
                stderr: "listing fixture failed".to_owned(),
            }),
        }
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        panic!("refresh without auto-download must not download")
    }
}

fn response(body: &str) -> Response {
    Response {
        body: body.to_owned(),
        etag: None,
        last_modified: None,
    }
}

fn youtube_feed(guid: &str) -> FixedFeed {
    FixedFeed {
        body: format!(
            r#"<feed xmlns="http://www.w3.org/2005/Atom"
                     xmlns:yt="http://www.youtube.com/xml/schemas/2015">
              <title>Videos</title><author><name>Channel</name></author>
              <entry><id>yt:video:{guid}</id><yt:videoId>{guid}</yt:videoId>
                <title>Episode</title><published>2026-09-09T08:00:00Z</published></entry>
            </feed>"#
        ),
    }
}

fn rss_feed(guid: &str) -> FixedFeed {
    FixedFeed {
        body: format!(
            r#"<rss><channel><title>RSS show</title><item><guid>{guid}</guid>
              <title>Episode</title>
              <enclosure url="https://example.test/episode.mp3" type="audio/mpeg" />
            </item></channel></rss>"#
        ),
    }
}

fn add_subscription(db: &Db, kind: PodcastKind, feed_url: &str) -> i64 {
    store::add_or_restore(
        db,
        &NewSubscription {
            kind,
            feed_url: feed_url.to_owned(),
            title: "Channel".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap()
}

fn seed_episode(db: &Db, subscription_id: i64, guid: &str, duration_secs: Option<i64>) -> i64 {
    store::upsert_episode(
        db,
        subscription_id,
        &ParsedEpisode {
            guid: guid.to_owned(),
            title: "Stored episode".to_owned(),
            image_url: None,
            audio_url: format!("https://www.youtube.com/watch?v={guid}"),
            page_url: None,
            published_at: None,
            duration_secs,
        },
        2,
    )
    .unwrap()
    .unwrap()
    .episode_id
}

fn refresh_with(
    db: &Db,
    feed: &FixedFeed,
    youtube: &CountingYoutube,
) -> Result<RefreshSummary, PipelineError> {
    let directory = tempfile::tempdir().unwrap();
    refresh_to_root(db, feed, youtube, 10, R::force(), directory.path())
}

#[test]
fn youtube_refresh_fills_a_missing_duration_from_a_bounded_listing() {
    let db = conn();
    let subscription_id = add_subscription(&db, PodcastKind::Youtube, CHANNEL_URL);
    let youtube = CountingYoutube::duration(VIDEO_ID, 225);

    let summary = refresh_with(&db, &youtube_feed(VIDEO_ID), &youtube).unwrap();

    let episode = super::super::query::episodes_for_subscription(&db, subscription_id)
        .unwrap()
        .remove(0);
    assert_eq!(summary.refreshed, 1);
    assert_eq!(episode.duration_secs, Some(225));
    assert_eq!(youtube.calls.get(), 1);
}

#[test]
fn youtube_duration_fill_never_overwrites_an_existing_runtime() {
    let db = conn();
    let subscription_id = add_subscription(&db, PodcastKind::Youtube, CHANNEL_URL);
    let episode_id = seed_episode(&db, subscription_id, VIDEO_ID, Some(186));
    seed_episode(&db, subscription_id, "lmnopqrstuv", None);
    let youtube = CountingYoutube::duration(VIDEO_ID, 175);

    refresh_with(&db, &youtube_feed(VIDEO_ID), &youtube).unwrap();

    assert_eq!(
        store::episode(&db, episode_id)
            .unwrap()
            .unwrap()
            .duration_secs,
        Some(186)
    );
    assert_eq!(youtube.calls.get(), 1);
}

#[test]
fn youtube_refresh_without_duration_gaps_skips_the_listing() {
    let db = conn();
    let subscription_id = add_subscription(&db, PodcastKind::Youtube, CHANNEL_URL);
    seed_episode(&db, subscription_id, VIDEO_ID, Some(186));
    let youtube = CountingYoutube::duration(VIDEO_ID, 175);

    refresh_with(&db, &youtube_feed(VIDEO_ID), &youtube).unwrap();

    assert_eq!(youtube.calls.get(), 0);
}

#[test]
fn youtube_channel_tab_ghost_does_not_trigger_a_duration_listing() {
    const CHANNEL_TAB_ID: &str = "UClDzr-KM5H2-bsO3xIC32mg";
    let db = conn();
    add_subscription(&db, PodcastKind::Youtube, CHANNEL_URL);
    let youtube = CountingYoutube::duration(CHANNEL_TAB_ID, 225);

    refresh_with(&db, &youtube_feed(CHANNEL_TAB_ID), &youtube).unwrap();

    assert_eq!(youtube.calls.get(), 0);
}

#[test]
fn disabled_youtube_gate_blocks_duration_listing_even_with_gaps() {
    let db = conn();
    let subscription_id = add_subscription(&db, PodcastKind::Youtube, CHANNEL_URL);
    seed_episode(&db, subscription_id, VIDEO_ID, None);
    crate::modules::set_enabled(&db, &crate::modules::YOUTUBE_MODULE, false).unwrap();
    let youtube = CountingYoutube::duration(VIDEO_ID, 225);

    let summary = refresh_with(&db, &youtube_feed(VIDEO_ID), &youtube).unwrap();

    assert_eq!(summary.failed, 1);
    assert_eq!(youtube.calls.get(), 0);
}

#[test]
fn failing_duration_listing_keeps_the_successful_refresh() {
    let db = conn();
    let subscription_id = add_subscription(&db, PodcastKind::Youtube, CHANNEL_URL);
    let youtube = CountingYoutube::failure(VIDEO_ID);

    let summary = refresh_with(&db, &youtube_feed(VIDEO_ID), &youtube).unwrap();

    assert_eq!(summary.refreshed, 1);
    assert_eq!(summary.failed, 0);
    assert_eq!(youtube.calls.get(), 1);
    assert_eq!(
        super::super::query::episodes_for_subscription(&db, subscription_id)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn failing_duration_store_keeps_the_successful_refresh() {
    let db = conn();
    let subscription_id = add_subscription(&db, PodcastKind::Youtube, CHANNEL_URL);
    db.conn()
        .execute_batch(
            "CREATE TRIGGER fail_duration_fill
             BEFORE UPDATE OF duration_secs ON podcast_episodes
             BEGIN
               SELECT RAISE(FAIL, 'duration fill failed');
             END;",
        )
        .unwrap();
    let youtube = CountingYoutube::duration(VIDEO_ID, 225);

    let summary = refresh_with(&db, &youtube_feed(VIDEO_ID), &youtube).unwrap();

    assert_eq!(summary.refreshed, 1);
    assert_eq!(summary.failed, 0);
    assert_eq!(youtube.calls.get(), 1);
    assert_eq!(
        super::super::query::episodes_for_subscription(&db, subscription_id)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn cancellation_after_commit_keeps_the_successful_refresh_without_listing() {
    let db = conn();
    let subscription_id = add_subscription(&db, PodcastKind::Youtube, CHANNEL_URL);
    let subscription = store::subscription(&db, subscription_id).unwrap().unwrap();
    let config = crate::podcasts::config::load(&db).unwrap();
    let feed = youtube_feed(VIDEO_ID);
    let youtube = CountingYoutube::duration(VIDEO_ID, 225);
    let abort = SyncAbort::new();
    let abort_at_commit = abort.clone();
    // SAFETY: The callback context remains alive until the hook is removed below.
    unsafe {
        rusqlite::ffi::sqlite3_commit_hook(
            db.conn().handle(),
            Some(cancel_sync_at_commit),
            std::ptr::from_ref(&abort_at_commit).cast_mut().cast(),
        );
    }
    let directory = tempfile::tempdir().unwrap();
    let mut summary = RefreshSummary::default();

    let result = super::sync::refresh_one_in(super::sync::RefreshOneParams {
        conn: db.conn(),
        feed_fetcher: &feed,
        youtube_fetcher: &youtube,
        now: 10,
        policy: crate::podcasts::refresh::RefreshPolicy::Force,
        download_root: directory.path(),
        config: &config,
        rss_allowed: true,
        youtube_allowed: true,
        subscription: &subscription,
        summary: &mut summary,
        abort: &abort,
        on_progress: &mut |_| {},
    });
    // SAFETY: Removing the hook before its callback context goes out of scope.
    unsafe {
        rusqlite::ffi::sqlite3_commit_hook(db.conn().handle(), None, std::ptr::null_mut());
    }

    assert!(result.is_ok());
    assert_eq!(summary.refreshed, 1);
    assert_eq!(youtube.calls.get(), 0);
    assert_eq!(
        super::super::query::episodes_for_subscription(&db, subscription_id)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn rss_refresh_never_triggers_a_youtube_duration_listing() {
    let db = conn();
    crate::modules::set_enabled(&db, &crate::modules::PODCASTS_MODULE, true).unwrap();
    add_subscription(&db, PodcastKind::Rss, "https://example.test/feed.xml");
    let youtube = CountingYoutube::duration(VIDEO_ID, 225);

    refresh_with(&db, &rss_feed(VIDEO_ID), &youtube).unwrap();

    assert_eq!(youtube.calls.get(), 0);
}
