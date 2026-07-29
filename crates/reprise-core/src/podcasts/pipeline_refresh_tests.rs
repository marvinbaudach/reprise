use std::cell::RefCell;

use super::*;
use crate::podcasts::store::{self, NewSubscription};

#[derive(Default)]
struct FakeFeed {
    responses: RefCell<Vec<Result<Response, PodcastError>>>,
    downloads: RefCell<Vec<String>>,
}

impl FeedFetcher for FakeFeed {
    fn fetch(&self, _: &SubscriptionRow) -> Result<Response, PodcastError> {
        self.responses.borrow_mut().remove(0)
    }

    fn download(&self, url: &str, destination: &Path) -> Result<(), PodcastError> {
        self.downloads.borrow_mut().push(url.to_owned());
        std::fs::write(destination, b"audio").map_err(|error| PodcastError::Body(error.to_string()))
    }
}

#[derive(Default)]
struct FakeYoutube;

impl YoutubeFetcher for FakeYoutube {
    fn list(&self, _: &str, _: usize) -> Result<ParsedFeed, PodcastError> {
        Err(PodcastError::YtDlp("unexpected YouTube call".to_owned()))
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        Err(PodcastError::YtDlp("unexpected YouTube call".to_owned()))
    }
}

struct PartialFailureFeed {
    response: RefCell<Option<Response>>,
}

impl FeedFetcher for PartialFailureFeed {
    fn fetch(&self, _: &SubscriptionRow) -> Result<Response, PodcastError> {
        Ok(self.response.borrow_mut().take().unwrap())
    }

    fn download(&self, _: &str, destination: &Path) -> Result<(), PodcastError> {
        std::fs::write(destination, b"partial")
            .map_err(|error| PodcastError::Body(error.to_string()))?;
        Err(PodcastError::Transport("connection reset".to_owned()))
    }
}

fn conn() -> Connection {
    let conn = crate::db::open_migrated(None).unwrap();
    // These tests exercise fetch/parse/store logic, not the NET-1a gate
    // itself (see the dedicated `net_1a_*` tests below), so Podcasts
    // starts enabled here.
    crate::modules::set_enabled(&conn, &crate::modules::PODCASTS_MODULE, true).unwrap();
    conn
}

fn add_subscription(conn: &Connection, url: &str, auto_download: bool) -> i64 {
    store::add_or_restore(
        conn,
        &NewSubscription {
            kind: PodcastKind::Rss,
            feed_url: url.to_owned(),
            title: "Show".to_owned(),
            author: None,
            image_url: None,
            auto_download,
        },
        1,
    )
    .unwrap()
}

fn feed_response(title: &str, episode_count: usize, etag: Option<&str>) -> Response {
    let items = (0..episode_count)
        .map(|index| {
            format!(
                "<item><guid>g{index}</guid><title>Episode {index}</title>\
                 <enclosure url=\"https://example.test/{index}.mp3\" type=\"audio/mpeg\"/>\
                 <pubDate>Wed, 22 Jul 2026 10:{index:02}:00 +0000</pubDate></item>"
            )
        })
        .collect::<String>();
    Response {
        body: format!("<rss><channel><title>{title}</title>{items}</channel></rss>"),
        etag: etag.map(str::to_owned),
        last_modified: None,
    }
}

#[test]
fn conditional_cycle_stores_headers_then_only_bumps_not_modified_state() {
    let conn = conn();
    let id = add_subscription(&conn, "https://example.test/feed", false);
    let feed = FakeFeed {
        responses: RefCell::new(vec![
            Ok(feed_response("Fetched Show", 1, Some("\"v1\""))),
            Err(PodcastError::NotModified),
        ]),
        ..FakeFeed::default()
    };
    let directory = tempfile::tempdir().unwrap();

    let first = refresh_to_root(&conn, &feed, &FakeYoutube, 10, true, directory.path()).unwrap();
    assert_eq!(first.refreshed, 1);
    assert_eq!(first.episodes_inserted, 1);
    let stored = store::subscription(&conn, id).unwrap().unwrap();
    assert_eq!(stored.title, "Fetched Show");
    assert_eq!(stored.etag.as_deref(), Some("\"v1\""));
    assert_eq!(stored.last_fetch_at, Some(10));

    let second = refresh_to_root(&conn, &feed, &FakeYoutube, 20, true, directory.path()).unwrap();
    assert_eq!(second.not_modified, 1);
    let stored = store::subscription(&conn, id).unwrap().unwrap();
    assert_eq!(stored.last_fetch_at, Some(20));
    assert_eq!(stored.last_outcome.as_deref(), Some("not_modified"));
    assert_eq!(stored.etag.as_deref(), Some("\"v1\""));
}

#[test]
fn future_only_baseline_skips_known_guids_and_keeps_importing_new_ones() {
    let conn = conn();
    let id = add_subscription(&conn, "https://example.test/feed", false);
    store::replace_future_only_baseline(&conn, id, &["g0".to_owned(), "g1".to_owned()]).unwrap();
    let feed = FakeFeed {
        responses: RefCell::new(vec![
            Ok(feed_response("Show", 2, None)),
            Ok(feed_response("Show", 3, None)),
        ]),
        ..FakeFeed::default()
    };
    let directory = tempfile::tempdir().unwrap();

    let first = refresh_to_root(&conn, &feed, &FakeYoutube, 10, true, directory.path()).unwrap();
    assert_eq!(first.episodes_inserted, 0);
    assert_eq!(super::super::query::count_unplayed(&conn).unwrap(), 0);

    let second = refresh_to_root(&conn, &feed, &FakeYoutube, 20, true, directory.path()).unwrap();
    assert_eq!(second.episodes_inserted, 1);
    assert_eq!(super::super::query::count_unplayed(&conn).unwrap(), 1);
    assert_eq!(
        store::future_only_baseline(&conn, id).unwrap(),
        ["g0".to_owned(), "g1".to_owned()]
    );
}

#[test]
fn one_failed_subscription_does_not_block_the_next() {
    let conn = conn();
    let failed = add_subscription(&conn, "https://example.test/failed", false);
    let succeeded = add_subscription(&conn, "https://example.test/succeeded", false);
    let feed = FakeFeed {
        responses: RefCell::new(vec![
            Err(PodcastError::Transport("offline".to_owned())),
            Ok(feed_response("Working", 1, None)),
        ]),
        ..FakeFeed::default()
    };
    let directory = tempfile::tempdir().unwrap();

    let summary = refresh_to_root(&conn, &feed, &FakeYoutube, 10, true, directory.path()).unwrap();

    assert_eq!(summary.attempted, 2);
    assert_eq!(summary.failed, 1);
    assert_eq!(summary.refreshed, 1);
    assert_eq!(
        store::subscription(&conn, failed)
            .unwrap()
            .unwrap()
            .last_outcome
            .as_deref(),
        Some("failed")
    );
    assert_eq!(
        store::subscription(&conn, succeeded)
            .unwrap()
            .unwrap()
            .last_outcome
            .as_deref(),
        Some("ok")
    );
}

#[test]
fn auto_download_is_capped_at_three_new_episodes_per_run() {
    let conn = conn();
    let id = add_subscription(&conn, "https://example.test/feed", true);
    let feed = FakeFeed {
        responses: RefCell::new(vec![Ok(feed_response("Show", 5, None))]),
        ..FakeFeed::default()
    };
    let directory = tempfile::tempdir().unwrap();

    let summary = refresh_to_root(&conn, &feed, &FakeYoutube, 10, true, directory.path()).unwrap();

    assert_eq!(summary.downloads_completed, 3);
    assert_eq!(feed.downloads.borrow().len(), 3);
    let downloaded = super::super::query::episodes_for_subscription(&conn, id)
        .unwrap()
        .into_iter()
        .filter(|episode| episode.downloaded_path.is_some())
        .count();
    assert_eq!(downloaded, 3);
}

#[test]
fn pod_7_auto_download_reports_episode_states_during_refresh() {
    let conn = conn();
    add_subscription(&conn, "https://example.test/feed", true);
    let feed = FakeFeed {
        responses: RefCell::new(vec![Ok(feed_response("Show", 1, None))]),
        ..FakeFeed::default()
    };
    let directory = tempfile::tempdir().unwrap();
    let mut events = Vec::new();

    refresh_to_root_with_download_progress(
        &conn,
        &feed,
        &FakeYoutube,
        10,
        true,
        directory.path(),
        &mut |episode_id, state| events.push((episode_id, state)),
    )
    .unwrap();

    assert!(matches!(events[0].1, DownloadState::Queued));
    assert!(matches!(
        events[1].1,
        DownloadState::Downloading {
            received_bytes: 0,
            total_bytes: None,
        }
    ));
    assert!(matches!(
        events.last(),
        Some((_, DownloadState::Downloaded { bytes: 5 }))
    ));
}

/// Block H (MCP parity): `download_episode` is the function
/// `music_manage_episodes`'s `download` action calls directly, outside a
/// refresh pass. It must actually perform the download (not just report a
/// state) and it must be idempotent — calling it again on an episode that
/// already has a file must not hit the network a second time.
#[test]
fn download_episode_downloads_a_specific_episode_by_id_and_persists_its_size() {
    let conn = conn();
    let id = add_subscription(&conn, "https://example.test/feed", false);
    let feed = FakeFeed {
        responses: RefCell::new(vec![Ok(feed_response("Show", 1, None))]),
        ..FakeFeed::default()
    };
    let directory = tempfile::tempdir().unwrap();
    refresh_to_root(&conn, &feed, &FakeYoutube, 10, true, directory.path()).unwrap();
    let episode_id = super::super::query::episodes_for_subscription(&conn, id).unwrap()[0].id;
    assert!(
        feed.downloads.borrow().is_empty(),
        "auto_download is off, so refresh must not have downloaded anything yet"
    );

    let outcome = download_episode(
        &conn,
        &feed,
        &FakeYoutube,
        directory.path(),
        episode_id,
        &mut |_| {},
    )
    .unwrap();

    assert_eq!(outcome, DownloadState::Downloaded { bytes: 5 });
    assert_eq!(feed.downloads.borrow().len(), 1);
    let stored = super::super::store::episode(&conn, episode_id)
        .unwrap()
        .unwrap();
    assert!(stored.downloaded_path.is_some());
    assert_eq!(stored.downloaded_bytes, Some(5));
}

#[test]
fn download_episode_is_idempotent_and_does_not_redownload_an_existing_file() {
    let conn = conn();
    let id = add_subscription(&conn, "https://example.test/feed", false);
    let feed = FakeFeed {
        responses: RefCell::new(vec![Ok(feed_response("Show", 1, None))]),
        ..FakeFeed::default()
    };
    let directory = tempfile::tempdir().unwrap();
    refresh_to_root(&conn, &feed, &FakeYoutube, 10, true, directory.path()).unwrap();
    let episode_id = super::super::query::episodes_for_subscription(&conn, id).unwrap()[0].id;
    download_episode(
        &conn,
        &feed,
        &FakeYoutube,
        directory.path(),
        episode_id,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(feed.downloads.borrow().len(), 1, "first call must download");

    let second = download_episode(
        &conn,
        &feed,
        &FakeYoutube,
        directory.path(),
        episode_id,
        &mut |_| {},
    )
    .unwrap();

    assert_eq!(second, DownloadState::Downloaded { bytes: 5 });
    assert_eq!(
        feed.downloads.borrow().len(),
        1,
        "an already-downloaded episode must not be fetched a second time"
    );
}

#[test]
fn download_episode_reports_not_found_for_an_unknown_id() {
    let conn = conn();
    let feed = FakeFeed::default();
    let directory = tempfile::tempdir().unwrap();

    let error = download_episode(
        &conn,
        &feed,
        &FakeYoutube,
        directory.path(),
        999_999,
        &mut |_| {},
    )
    .unwrap_err();

    assert!(matches!(error, PipelineError::EpisodeNotFound));
}

#[test]
fn existing_guid_keyed_file_is_reclaimed_without_downloading_again() {
    let conn = conn();
    let id = add_subscription(&conn, "https://example.test/feed", true);
    let feed = FakeFeed {
        responses: RefCell::new(vec![Ok(feed_response("Show", 1, None))]),
        ..FakeFeed::default()
    };
    let directory = tempfile::tempdir().unwrap();
    let existing = super::super::downloads::download_path(
        directory.path(),
        "https://example.test/feed",
        "g0",
        "mp3",
    );
    super::super::downloads::prepare_destination(&existing).unwrap();
    std::fs::write(&existing, b"orphan").unwrap();

    let summary = refresh_to_root(&conn, &feed, &FakeYoutube, 10, true, directory.path()).unwrap();

    assert_eq!(summary.downloads_completed, 0);
    assert!(feed.downloads.borrow().is_empty());
    assert_eq!(
        super::super::query::episodes_for_subscription(&conn, id).unwrap()[0]
            .downloaded_path
            .as_deref(),
        existing.to_str()
    );
}

#[test]
fn failed_download_does_not_leave_a_reclaimable_partial_file() {
    let conn = conn();
    let id = add_subscription(&conn, "https://example.test/feed", true);
    let feed = PartialFailureFeed {
        response: RefCell::new(Some(feed_response("Show", 1, None))),
    };
    let directory = tempfile::tempdir().unwrap();

    let summary = refresh_to_root(&conn, &feed, &FakeYoutube, 10, true, directory.path()).unwrap();

    assert_eq!(summary.downloads_failed, 1);
    let episode = super::super::query::episodes_for_subscription(&conn, id).unwrap()[0].clone();
    assert!(episode.downloaded_path.is_none());
    assert!(super::super::downloads::reclaim_existing(
        directory.path(),
        "https://example.test/feed",
        "g0"
    )
    .unwrap()
    .is_none());
}

/// `NET-1a`: a subscription whose kind's module is off is skipped, not
/// fetched — this is the RSS half of the per-kind gate that issue #96
/// requires (YouTube's half is covered in `pipeline_youtube_tests.rs`).
#[test]
fn net_1a_disabled_podcasts_module_skips_rss_refresh_without_fetching() {
    let conn = conn();
    crate::modules::set_enabled(&conn, &crate::modules::PODCASTS_MODULE, false).unwrap();
    let id = add_subscription(&conn, "https://example.test/feed", false);
    let feed = FakeFeed::default(); // no responses queued: fetch() would panic/underflow if called
    let directory = tempfile::tempdir().unwrap();

    let summary = refresh_to_root(&conn, &feed, &FakeYoutube, 10, true, directory.path()).unwrap();

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

/// `NET-1a`: the global online-sources gate blocks a refresh even when the
/// per-source module (Podcasts) is on — "off really means off" for every
/// source, not just the three named network modules.
#[test]
fn net_1a_global_gate_off_blocks_rss_refresh_even_with_podcasts_on() {
    let conn = conn();
    crate::online_sources::set_enabled(&conn, false).unwrap();
    let id = add_subscription(&conn, "https://example.test/feed", false);
    let feed = FakeFeed::default();
    let directory = tempfile::tempdir().unwrap();

    let summary = refresh_to_root(&conn, &feed, &FakeYoutube, 10, true, directory.path()).unwrap();

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
