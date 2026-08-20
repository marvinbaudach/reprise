//! Serial podcast refresh and download pipeline.

use std::path::Path;

use rusqlite::Connection;

#[path = "pipeline_retry.rs"]
mod retry;
use retry::{clear_retry, pending_retry, previous_attempt, set_retry, RetryKey};

use crate::{db::Db, source_error::SourceErrorKind};

use super::download_state::{DownloadProgress, DownloadState};
use super::feed::{ParsedEpisode, ParsedFeed};
use super::http::Response;
use super::refresh::{RefreshPolicy, RefreshRequest};
use super::store::FetchSuccess;
use super::{PodcastError, PodcastKind, SubscriptionRow};

#[path = "pipeline_load_more.rs"]
mod load_more;
pub use load_more::load_more_youtube;

const MAX_AUTO_DOWNLOADS_PER_SUBSCRIPTION: usize = 3;
const OFFICIAL_YOUTUBE_LIMIT: usize = 15;

pub trait FeedFetcher {
    fn fetch(&self, subscription: &SubscriptionRow) -> Result<Response, PodcastError>;
    fn fetch_url(
        &self,
        url: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> Result<Response, PodcastError> {
        super::http::get_feed_conditional(url, etag, last_modified)
    }
    fn download(&self, url: &str, destination: &Path) -> Result<(), PodcastError>;

    fn download_with_progress(
        &self,
        url: &str,
        destination: &Path,
        _on_progress: &mut dyn FnMut(DownloadProgress),
    ) -> Result<(), PodcastError> {
        self.download(url, destination)
    }
}

pub trait YoutubeFetcher {
    fn resolve_channel_url(&self, _url: &str) -> Result<Option<String>, PodcastError> {
        Ok(None)
    }
    fn list(&self, url: &str, limit: usize) -> Result<ParsedFeed, PodcastError>;
    fn list_range(&self, url: &str, end: usize) -> Result<ParsedFeed, PodcastError> {
        self.list(url, end)
    }
    fn download(&self, url: &str, destination: &Path) -> Result<(), PodcastError>;

    fn download_with_progress(
        &self,
        url: &str,
        destination: &Path,
        _on_progress: &mut dyn FnMut(DownloadProgress),
    ) -> Result<(), PodcastError> {
        self.download(url, destination)
    }

    fn download_with_metadata_and_progress(
        &self,
        url: &str,
        destination: &Path,
        on_progress: &mut dyn FnMut(DownloadProgress),
    ) -> Result<super::ytdlp::YoutubeDownloadMetadata, PodcastError> {
        self.download_with_progress(url, destination, on_progress)
            .map(|_| super::ytdlp::YoutubeDownloadMetadata::default())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HttpFeedFetcher;

impl FeedFetcher for HttpFeedFetcher {
    fn fetch(&self, subscription: &SubscriptionRow) -> Result<Response, PodcastError> {
        super::http::get_feed_conditional(
            &subscription.feed_url,
            subscription.etag.as_deref(),
            subscription.last_modified.as_deref(),
        )
    }

    fn download(&self, url: &str, destination: &Path) -> Result<(), PodcastError> {
        super::http::download(url, destination)
    }

    fn download_with_progress(
        &self,
        url: &str,
        destination: &Path,
        on_progress: &mut dyn FnMut(DownloadProgress),
    ) -> Result<(), PodcastError> {
        super::http::download_with_progress(url, destination, on_progress)
    }
}

impl YoutubeFetcher for super::ytdlp::YtDlp {
    fn resolve_channel_url(&self, url: &str) -> Result<Option<String>, PodcastError> {
        Ok(super::ytdlp::YtDlp::list(self, url)?.source_url)
    }

    fn list(&self, url: &str, limit: usize) -> Result<ParsedFeed, PodcastError> {
        let listing = super::youtube::project_playlist(super::ytdlp::YtDlp::list(self, url)?);
        Ok(project_youtube_feed(listing, limit))
    }

    fn download(&self, url: &str, destination: &Path) -> Result<(), PodcastError> {
        super::ytdlp::YtDlp::download(self, url, destination).map(|_| ())
    }

    fn list_range(&self, url: &str, end: usize) -> Result<ParsedFeed, PodcastError> {
        let listing =
            super::youtube::project_playlist(super::ytdlp::YtDlp::list_range(self, url, end)?);
        Ok(project_youtube_feed(listing, end))
    }

    fn download_with_progress(
        &self,
        url: &str,
        destination: &Path,
        on_progress: &mut dyn FnMut(DownloadProgress),
    ) -> Result<(), PodcastError> {
        super::ytdlp::YtDlp::download_with_progress(self, url, destination, on_progress).map(|_| ())
    }

    fn download_with_metadata_and_progress(
        &self,
        url: &str,
        destination: &Path,
        on_progress: &mut dyn FnMut(DownloadProgress),
    ) -> Result<super::ytdlp::YoutubeDownloadMetadata, PodcastError> {
        super::ytdlp::YtDlp::download_with_progress(self, url, destination, on_progress)
    }
}

pub fn project_youtube_feed(listing: super::youtube::YoutubeListing, limit: usize) -> ParsedFeed {
    // yt-dlp names the channel in `channel`/`uploader` on most channel dumps,
    // but not on all of them — and where it does not, its `title` *is* the
    // channel name, because this path fetches a channel URL rather than the
    // uploads playlist. (The useless "Videos" title comes from the RSS feed,
    // which is a different path and is handled where that feed is parsed.)
    // Falling through to `None` here is what made a freshly added channel fall
    // back to its own URL as a title.
    let channel = listing.channel.or(listing.title);
    ParsedFeed {
        title: channel.clone(),
        author: channel,
        // Was a hard `None` until 2026-08-18, which is why
        // `podcast_subscriptions.image_url` could never hold a channel avatar.
        image_url: listing.image_url,
        episodes: listing
            .episodes
            .into_iter()
            .take(limit)
            .map(|episode| ParsedEpisode {
                guid: episode.guid,
                title: episode.title,
                image_url: episode.image_url,
                audio_url: episode.audio_url,
                page_url: None,
                published_at: episode.published_at,
                duration_secs: episode.duration_secs,
            })
            .collect(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefreshFailure {
    pub subscription_id: i64,
    pub title: String,
    pub kind: SourceErrorKind,
    pub classified_cause: &'static str,
}

impl RefreshFailure {
    pub(crate) fn from_error(
        subscription_id: i64,
        title: impl Into<String>,
        error: &PodcastError,
    ) -> Self {
        Self {
            subscription_id,
            title: title.into(),
            kind: SourceErrorKind::from(error),
            classified_cause: error.classify(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RefreshSummary {
    pub attempted: usize,
    pub refreshed: usize,
    pub not_modified: usize,
    pub failed: usize,
    pub failures: Vec<RefreshFailure>,
    pub episodes_inserted: usize,
    pub episodes_updated: usize,
    pub downloads_completed: usize,
    pub downloads_failed: usize,
}

impl RefreshSummary {
    fn push_failure(&mut self, subscription: &SubscriptionRow, error: &PodcastError) {
        self.failed += 1;
        self.failures.push(RefreshFailure::from_error(
            subscription.id,
            &subscription.title,
            error,
        ));
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    Provider(#[from] PodcastError),
    #[error("YouTube channel is no longer available")]
    YoutubeSourceUnavailable,
    #[error(transparent)]
    Cleanup(#[from] super::downloads::CleanupError),
    #[error("podcast episode does not exist")]
    EpisodeNotFound,
    #[error("a download for this episode is already running")]
    DownloadAlreadyRunning,
}

#[path = "pipeline_download.rs"]
mod download;
pub use download::download_episode;
use download::download_episode_in;
#[cfg(test)]
use download::remove_completed_download;

pub fn refresh(
    db: &Db,
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    now: i64,
    request: RefreshRequest,
) -> Result<RefreshSummary, PipelineError> {
    let conn = db.conn();
    refresh_to_root_with_download_progress(
        conn,
        feed_fetcher,
        youtube_fetcher,
        now,
        request,
        &super::downloads::default_download_root(),
        &mut |_, _| {},
    )
}

pub fn refresh_with_download_progress(
    db: &Db,
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    now: i64,
    request: RefreshRequest,
    on_download: &mut dyn FnMut(i64, DownloadState),
) -> Result<RefreshSummary, PipelineError> {
    let conn = db.conn();
    refresh_to_root_with_download_progress(
        conn,
        feed_fetcher,
        youtube_fetcher,
        now,
        request,
        &super::downloads::default_download_root(),
        on_download,
    )
}

pub fn refresh_to_root(
    db: &Db,
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    now: i64,
    request: RefreshRequest,
    download_root: &Path,
) -> Result<RefreshSummary, PipelineError> {
    let conn = db.conn();
    refresh_to_root_with_download_progress(
        conn,
        feed_fetcher,
        youtube_fetcher,
        now,
        request,
        download_root,
        &mut |_, _| {},
    )
}

/// Persists the canonical channel URL a `@handle` resolved to, so later
/// refreshes skip the yt-dlp round trip entirely.
///
/// The write is refused when another subscription already holds that URL. That
/// is the right outcome — two rows must not share a `feed_url` — but it leaves
/// the handle subscription resolving forever, so it is worth a line in the log
/// rather than a discarded `bool`.
fn adopt_resolved_channel_url(
    conn: &Connection,
    subscription_id: i64,
    channel_url: &str,
) -> Result<(), rusqlite::Error> {
    if !super::store::update_feed_url_in(conn, subscription_id, channel_url)? {
        tracing::warn!(
            subscription_id,
            channel_url,
            "resolved channel URL already belongs to another subscription; keeping the handle URL"
        );
    }
    Ok(())
}

fn refresh_to_root_with_download_progress(
    conn: &Connection,
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    now: i64,
    request: RefreshRequest,
    download_root: &Path,
    on_download: &mut dyn FnMut(i64, DownloadState),
) -> Result<RefreshSummary, PipelineError> {
    let config = super::config::load_in(conn)?;
    let rss_allowed = super::config::source_network_allowed_in(conn, PodcastKind::Rss)?;
    let youtube_allowed = super::config::source_network_allowed_in(conn, PodcastKind::Youtube)?;
    let jitter = super::refresh::jitter_seconds(&database_seed(conn)?);
    let subscriptions = super::store::active_subscriptions_in(conn)?;
    let mut summary = RefreshSummary::default();
    for subscription in subscriptions {
        if let Some(kind) = request.kind {
            if subscription.kind != kind {
                continue;
            }
        }
        let retry_key = RetryKey {
            connection: std::ptr::from_ref(conn).addr(),
            subscription_id: subscription.id,
        };
        if !matches!(request.policy, RefreshPolicy::Force) {
            let retry = if subscription.last_outcome.as_deref() == Some("failed") {
                pending_retry(retry_key)
            } else {
                clear_retry(retry_key);
                None
            };
            let due = retry.map_or_else(
                || match request.policy {
                    RefreshPolicy::Due => super::refresh::refresh_due_with_hours(
                        subscription.last_fetch_at,
                        now,
                        config.refresh_hours,
                        jitter,
                    ),
                    RefreshPolicy::StaleFor { seconds } => {
                        super::refresh::refresh_due_after_seconds(
                            subscription.last_fetch_at,
                            now,
                            seconds,
                        )
                    }
                    RefreshPolicy::Force => true,
                },
                |retry| retry.is_due(now),
            );
            if !due {
                continue;
            }
        }
        summary.attempted += 1;
        let mut resolved_channel_url = None::<String>;
        let result = match subscription.kind {
            PodcastKind::Rss if rss_allowed => {
                feed_fetcher.fetch(&subscription).and_then(|response| {
                    let feed = super::feed::parse_feed(&response.body, config.import_count)?;
                    Ok((feed, Some(response)))
                })
            }
            PodcastKind::Rss => Err(PodcastError::Disabled(
                "RSS podcasts are disabled".to_owned(),
            )),
            PodcastKind::Youtube if youtube_allowed => {
                // Resolving a channel identity runs a yt-dlp subprocess, so a
                // transient failure here is ordinary. It must stay this one
                // subscription's failure, handled by the shared `Err` arm below
                // like any fetch or parse failure — propagating it with `?`
                // would abandon every remaining subscription in the batch and
                // skip recording the failure on this one.
                let channel_url = if super::youtube::long_form_feed_url(&subscription.feed_url)
                    .is_some()
                {
                    Ok(subscription.feed_url.clone())
                } else {
                    youtube_fetcher
                        .resolve_channel_url(&subscription.feed_url)
                        .map(|resolved| resolved.unwrap_or_else(|| subscription.feed_url.clone()))
                };
                let channel_url = match channel_url {
                    Ok(channel_url) => channel_url,
                    Err(error) => {
                        tracing::warn!(
                            subscription_id = subscription.id,
                            %error,
                            "could not resolve the YouTube channel identity"
                        );
                        super::store::update_fetch_failed_in(conn, subscription.id, now)?;
                        summary.push_failure(&subscription, &error);
                        continue;
                    }
                };
                if let Some(feed_url) = super::youtube::long_form_feed_url(&channel_url) {
                    if channel_url != subscription.feed_url {
                        resolved_channel_url = Some(channel_url.clone());
                    }
                    let official_feed = feed_fetcher
                        .fetch_url(
                            &feed_url,
                            subscription.etag.as_deref(),
                            subscription.last_modified.as_deref(),
                        )
                        .and_then(|response| {
                            let feed =
                                super::feed::parse_feed(&response.body, OFFICIAL_YOUTUBE_LIMIT)?;
                            Ok((feed, Some(response)))
                        });
                    match official_feed {
                        result @ (Ok(_) | Err(PodcastError::NotModified)) => result,
                        Err(_) => youtube_fetcher
                            .list(&channel_url, config.youtube_import_count)
                            .map(|feed| (feed, None)),
                    }
                } else {
                    youtube_fetcher
                        .list(&channel_url, config.youtube_import_count)
                        .map(|feed| (feed, None))
                }
            }
            PodcastKind::Youtube => Err(PodcastError::Disabled(
                "YouTube sources are disabled".to_owned(),
            )),
        };
        let (feed, response) = match result {
            Ok(result) => result,
            Err(PodcastError::NotModified) => {
                if let Some(url) = resolved_channel_url.as_deref() {
                    adopt_resolved_channel_url(conn, subscription.id, url)?;
                }
                super::store::update_fetch_not_modified_in(conn, subscription.id, now)?;
                clear_retry(retry_key);
                summary.not_modified += 1;
                continue;
            }
            Err(error) => {
                tracing::warn!(
                    subscription_id = subscription.id,
                    %error,
                    "podcast refresh failed"
                );
                let retry = if matches!(request.policy, RefreshPolicy::Force) {
                    None
                } else {
                    super::refresh::next_retry(&error, previous_attempt(retry_key), now)
                };
                if retry.is_some() {
                    // Keep the last successful-fetch timestamp due so the
                    // existing outer hourly trigger continues dispatching.
                    // The in-process retry state below still prevents an
                    // actual provider call before the computed deadline.
                    record_failed_outcome_in(conn, subscription.id)?;
                } else {
                    super::store::update_fetch_failed_in(conn, subscription.id, now)?;
                }
                set_retry(retry_key, retry);
                summary.push_failure(&subscription, &error);
                continue;
            }
        };

        if let Some(url) = resolved_channel_url.as_deref() {
            adopt_resolved_channel_url(conn, subscription.id, url)?;
        }

        let baseline = super::store::future_only_baseline_in(conn, subscription.id)?
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        let first_seen_at = if matches!(
            subscription.last_outcome.as_deref(),
            Some("ok" | "not_modified")
        ) {
            now
        } else {
            subscription.added_at
        };
        let mut new_episode_ids = Vec::new();
        for episode in &feed.episodes {
            if baseline.contains(&episode.guid) {
                continue;
            }
            let Some(upsert) =
                super::store::upsert_episode_in(conn, subscription.id, episode, first_seen_at)?
            else {
                continue;
            };
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
        super::store::update_fetch_success_in(
            conn,
            subscription.id,
            now,
            FetchSuccess {
                etag: response.and_then(|value| value.etag.as_deref()),
                last_modified: response.and_then(|value| value.last_modified.as_deref()),
                title: match subscription.kind {
                    PodcastKind::Rss => feed.title.as_deref(),
                    PodcastKind::Youtube => super::youtube::subscription_title(&feed),
                },
                author: feed.author.as_deref(),
                image_url: feed.image_url.as_deref(),
            },
        )?;
        clear_retry(retry_key);
        summary.refreshed += 1;

        if subscription.auto_download {
            for episode_id in new_episode_ids
                .into_iter()
                .take(MAX_AUTO_DOWNLOADS_PER_SUBSCRIPTION)
            {
                let Some(episode) = super::store::episode_in(conn, episode_id)? else {
                    continue;
                };
                if episode.downloaded_path.is_some() {
                    // Already satisfied by `reclaim_download` above — no
                    // network round trip, and (unlike a genuinely fresh
                    // download) not counted toward this refresh's completed
                    // total.
                    continue;
                }
                let mut on_progress = |state: DownloadState| on_download(episode_id, state);
                let outcome = download_episode_in(
                    conn,
                    feed_fetcher,
                    youtube_fetcher,
                    download_root,
                    episode_id,
                    &mut on_progress,
                )?;
                match outcome {
                    DownloadState::Downloaded { .. } => summary.downloads_completed += 1,
                    DownloadState::Failed { .. } => summary.downloads_failed += 1,
                    // `download_episode` only ever returns a terminal state.
                    DownloadState::NotDownloaded
                    | DownloadState::Queued
                    | DownloadState::Downloading { .. }
                    | DownloadState::Missing => {}
                }
            }
        }
    }
    super::downloads::enforce_cleanup_in(
        conn,
        download_root,
        config.cleanup_policy,
        config.keep_downloaded_default,
        now,
    )?;
    Ok(summary)
}

fn record_failed_outcome_in(
    conn: &Connection,
    subscription_id: i64,
) -> Result<(), rusqlite::Error> {
    // A retryable failure changes the readable state immediately without
    // pretending that the last successful fetch happened just now.
    conn.execute(
        "UPDATE podcast_subscriptions
         SET last_outcome = 'failed'
         WHERE id = ?1",
        [subscription_id],
    )?;
    Ok(())
}

fn reclaim_download(
    conn: &Connection,
    download_root: &Path,
    subscription: &SubscriptionRow,
    episode: &ParsedEpisode,
    episode_id: i64,
) -> Result<(), PipelineError> {
    let Some(row) = super::store::episode_in(conn, episode_id)? else {
        return Ok(());
    };
    if row.downloaded_path.is_some() {
        return Ok(());
    }
    if let Some(path) =
        super::downloads::reclaim_existing(download_root, &subscription.feed_url, &episode.guid)
            .map_err(super::downloads::CleanupError::from)?
    {
        let bytes = std::fs::metadata(&path)
            .map_err(super::downloads::CleanupError::from)?
            .len()
            .min(i64::MAX as u64) as i64;
        super::downloads::set_downloaded_file_in(conn, episode_id, path.to_str(), Some(bytes))?;
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
#[path = "pipeline_youtube_test_support.rs"]
mod youtube_test_support;

#[cfg(test)]
#[path = "pipeline_youtube_projection_tests.rs"]
mod youtube_projection_tests;

#[cfg(test)]
#[path = "pipeline_youtube_handle_tests.rs"]
mod youtube_handle_tests;

#[cfg(test)]
#[path = "pipeline_youtube_window_tests.rs"]
mod youtube_window_tests;

#[cfg(test)]
#[path = "pipeline_youtube_gate_tests.rs"]
mod youtube_gate_tests;

#[cfg(test)]
#[path = "pipeline_refresh_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "pipeline_download_tests.rs"]
mod download_tests;

#[cfg(test)]
#[path = "pipeline_refresh_policy_tests.rs"]
mod refresh_policy_tests;

#[cfg(test)]
#[path = "pipeline_tag_tests.rs"]
mod tag_tests;
