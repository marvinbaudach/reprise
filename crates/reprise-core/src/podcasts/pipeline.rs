//! Serial podcast refresh and download pipeline.

use std::path::Path;

use rusqlite::Connection;

use super::feed::{ParsedEpisode, ParsedFeed};
use super::http::Response;
use super::store::FetchSuccess;
use super::{PodcastError, PodcastKind, SubscriptionRow};

const MAX_AUTO_DOWNLOADS_PER_SUBSCRIPTION: usize = 3;

pub trait FeedFetcher {
    fn fetch(&self, subscription: &SubscriptionRow) -> Result<Response, PodcastError>;
    fn download(&self, url: &str, destination: &Path) -> Result<(), PodcastError>;
}

pub trait YoutubeFetcher {
    fn list(&self, url: &str, limit: usize) -> Result<ParsedFeed, PodcastError>;
    fn download(&self, url: &str, destination: &Path) -> Result<(), PodcastError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HttpFeedFetcher;

impl FeedFetcher for HttpFeedFetcher {
    fn fetch(&self, subscription: &SubscriptionRow) -> Result<Response, PodcastError> {
        super::http::get_conditional(
            &subscription.feed_url,
            subscription.etag.as_deref(),
            subscription.last_modified.as_deref(),
        )
    }

    fn download(&self, url: &str, destination: &Path) -> Result<(), PodcastError> {
        super::http::download(url, destination)
    }
}

impl YoutubeFetcher for super::ytdlp::YtDlp {
    fn list(&self, url: &str, limit: usize) -> Result<ParsedFeed, PodcastError> {
        let listing = super::youtube::project_playlist(super::ytdlp::YtDlp::list(self, url)?);
        Ok(ParsedFeed {
            title: listing.title.unwrap_or_else(|| url.to_owned()),
            author: None,
            image_url: None,
            episodes: listing
                .episodes
                .into_iter()
                .take(limit)
                .map(|episode| ParsedEpisode {
                    guid: episode.guid,
                    title: episode.title,
                    audio_url: episode.audio_url,
                    page_url: None,
                    published_at: episode.published_at,
                    duration_secs: episode.duration_secs,
                })
                .collect(),
        })
    }

    fn download(&self, url: &str, destination: &Path) -> Result<(), PodcastError> {
        super::ytdlp::YtDlp::download(self, url, destination)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RefreshSummary {
    pub attempted: usize,
    pub refreshed: usize,
    pub not_modified: usize,
    pub failed: usize,
    pub episodes_inserted: usize,
    pub episodes_updated: usize,
    pub downloads_completed: usize,
    pub downloads_failed: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    Cleanup(#[from] super::downloads::CleanupError),
}

pub fn refresh(
    conn: &Connection,
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    now: i64,
    force: bool,
) -> Result<RefreshSummary, PipelineError> {
    refresh_to_root(
        conn,
        feed_fetcher,
        youtube_fetcher,
        now,
        force,
        &super::downloads::default_download_root(),
    )
}

pub fn refresh_to_root(
    conn: &Connection,
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    now: i64,
    force: bool,
    download_root: &Path,
) -> Result<RefreshSummary, PipelineError> {
    let config = super::config::load(conn)?;
    let jitter = super::refresh::jitter_seconds(&database_seed(conn)?);
    let subscriptions = super::store::active_subscriptions(conn)?;
    let mut summary = RefreshSummary::default();
    for subscription in subscriptions {
        if !force
            && !super::refresh::refresh_due_with_hours(
                subscription.last_fetch_at,
                now,
                config.refresh_hours,
                jitter,
            )
        {
            continue;
        }
        summary.attempted += 1;
        let result = match subscription.kind {
            PodcastKind::Rss => feed_fetcher.fetch(&subscription).and_then(|response| {
                let feed = super::feed::parse_feed(&response.body, config.import_count)?;
                Ok((feed, Some(response)))
            }),
            PodcastKind::Youtube if config.youtube_enabled => youtube_fetcher
                .list(&subscription.feed_url, config.import_count)
                .map(|feed| (feed, None)),
            PodcastKind::Youtube => Err(PodcastError::YtDlp(
                "YouTube sources are disabled".to_owned(),
            )),
        };
        let (feed, response) = match result {
            Ok(result) => result,
            Err(PodcastError::NotModified) => {
                super::store::update_fetch_not_modified(conn, subscription.id, now)?;
                summary.not_modified += 1;
                continue;
            }
            Err(error) => {
                tracing::warn!(
                    subscription_id = subscription.id,
                    %error,
                    "podcast refresh failed"
                );
                super::store::update_fetch_failed(conn, subscription.id, now)?;
                summary.failed += 1;
                continue;
            }
        };

        let baseline = super::store::future_only_baseline(conn, subscription.id)?
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        let mut new_episode_ids = Vec::new();
        for episode in &feed.episodes {
            if baseline.contains(&episode.guid) {
                continue;
            }
            let upsert = super::store::upsert_episode(conn, subscription.id, episode, now)?;
            if upsert.inserted {
                summary.episodes_inserted += 1;
                new_episode_ids.push(upsert.episode_id);
            } else {
                summary.episodes_updated += 1;
            }
            reclaim_download(
                conn,
                download_root,
                &subscription,
                episode,
                upsert.episode_id,
            )?;
        }
        let response = response.as_ref();
        super::store::update_fetch_success(
            conn,
            subscription.id,
            now,
            FetchSuccess {
                etag: response.and_then(|value| value.etag.as_deref()),
                last_modified: response.and_then(|value| value.last_modified.as_deref()),
                title: Some(&feed.title),
                author: feed.author.as_deref(),
                image_url: feed.image_url.as_deref(),
            },
        )?;
        summary.refreshed += 1;

        if subscription.auto_download {
            for episode_id in new_episode_ids
                .into_iter()
                .take(MAX_AUTO_DOWNLOADS_PER_SUBSCRIPTION)
            {
                let Some(episode) = super::store::episode(conn, episode_id)? else {
                    continue;
                };
                if episode.downloaded_path.is_some() {
                    continue;
                }
                let extension = match subscription.kind {
                    PodcastKind::Rss => super::downloads::extension_from_url(&episode.audio_url),
                    PodcastKind::Youtube => "audio",
                };
                let destination = super::downloads::download_path(
                    download_root,
                    &subscription.feed_url,
                    &episode.guid,
                    extension,
                );
                let download = super::downloads::prepare_destination(&destination)
                    .map_err(|error| PodcastError::Body(error.to_string()))
                    .and_then(|()| match subscription.kind {
                        PodcastKind::Rss => feed_fetcher.download(&episode.audio_url, &destination),
                        PodcastKind::Youtube => {
                            youtube_fetcher.download(&episode.audio_url, &destination)
                        }
                    });
                match download {
                    Ok(()) if destination.is_file() => {
                        super::store::set_downloaded_path(conn, episode_id, destination.to_str())?;
                        summary.downloads_completed += 1;
                    }
                    Ok(()) => {
                        remove_partial_download(&destination);
                        summary.downloads_failed += 1;
                    }
                    Err(error) => {
                        remove_partial_download(&destination);
                        tracing::warn!(
                            episode_id,
                            %error,
                            "podcast auto-download failed"
                        );
                        summary.downloads_failed += 1;
                    }
                }
            }
        }
    }
    super::downloads::enforce_cleanup(conn, config.cleanup_policy, now)?;
    Ok(summary)
}

fn remove_partial_download(path: &Path) {
    if let Err(error) = std::fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(
                path = %path.display(),
                %error,
                "could not remove partial podcast download"
            );
        }
    }
}

fn reclaim_download(
    conn: &Connection,
    download_root: &Path,
    subscription: &SubscriptionRow,
    episode: &ParsedEpisode,
    episode_id: i64,
) -> Result<(), PipelineError> {
    let Some(row) = super::store::episode(conn, episode_id)? else {
        return Ok(());
    };
    if row.downloaded_path.is_some() {
        return Ok(());
    }
    if let Some(path) =
        super::downloads::reclaim_existing(download_root, &subscription.feed_url, &episode.guid)
            .map_err(super::downloads::CleanupError::from)?
    {
        super::store::set_downloaded_path(conn, episode_id, path.to_str())?;
    }
    Ok(())
}

fn database_seed(conn: &Connection) -> Result<String, rusqlite::Error> {
    conn.query_row(
        "SELECT file FROM pragma_database_list WHERE name = 'main'",
        [],
        |row| row.get::<_, String>(0),
    )
    .map(|value| {
        if value.is_empty() {
            "podcasts-in-memory".to_owned()
        } else {
            value
        }
    })
}

#[cfg(test)]
mod tests {
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
            std::fs::write(destination, b"audio")
                .map_err(|error| PodcastError::Body(error.to_string()))
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
        crate::db::open_migrated(None).unwrap()
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

        let first =
            refresh_to_root(&conn, &feed, &FakeYoutube, 10, true, directory.path()).unwrap();
        assert_eq!(first.refreshed, 1);
        assert_eq!(first.episodes_inserted, 1);
        let stored = store::subscription(&conn, id).unwrap().unwrap();
        assert_eq!(stored.title, "Fetched Show");
        assert_eq!(stored.etag.as_deref(), Some("\"v1\""));
        assert_eq!(stored.last_fetch_at, Some(10));

        let second =
            refresh_to_root(&conn, &feed, &FakeYoutube, 20, true, directory.path()).unwrap();
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
        store::replace_future_only_baseline(&conn, id, &["g0".to_owned(), "g1".to_owned()])
            .unwrap();
        let feed = FakeFeed {
            responses: RefCell::new(vec![
                Ok(feed_response("Show", 2, None)),
                Ok(feed_response("Show", 3, None)),
            ]),
            ..FakeFeed::default()
        };
        let directory = tempfile::tempdir().unwrap();

        let first =
            refresh_to_root(&conn, &feed, &FakeYoutube, 10, true, directory.path()).unwrap();
        assert_eq!(first.episodes_inserted, 0);
        assert_eq!(super::super::query::count_unplayed(&conn).unwrap(), 0);

        let second =
            refresh_to_root(&conn, &feed, &FakeYoutube, 20, true, directory.path()).unwrap();
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

        let summary =
            refresh_to_root(&conn, &feed, &FakeYoutube, 10, true, directory.path()).unwrap();

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

        let summary =
            refresh_to_root(&conn, &feed, &FakeYoutube, 10, true, directory.path()).unwrap();

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

        let summary =
            refresh_to_root(&conn, &feed, &FakeYoutube, 10, true, directory.path()).unwrap();

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

        let summary =
            refresh_to_root(&conn, &feed, &FakeYoutube, 10, true, directory.path()).unwrap();

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
}
