//! Serial podcast refresh and download pipeline.

use std::path::Path;

use rusqlite::Connection;

use super::download_state::{DownloadProgress, DownloadState};
use super::feed::{ParsedEpisode, ParsedFeed};
use super::http::Response;
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
        super::http::get_conditional(url, etag, last_modified)
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
    fn list(&self, url: &str, limit: usize) -> Result<ParsedFeed, PodcastError> {
        let listing = super::youtube::project_playlist(super::ytdlp::YtDlp::list(self, url)?);
        Ok(project_youtube_feed(listing, limit))
    }

    fn download(&self, url: &str, destination: &Path) -> Result<(), PodcastError> {
        super::ytdlp::YtDlp::download(self, url, destination)
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
        super::ytdlp::YtDlp::download_with_progress(self, url, destination, on_progress)
    }
}

pub fn project_youtube_feed(listing: super::youtube::YoutubeListing, limit: usize) -> ParsedFeed {
    ParsedFeed {
        title: listing.title.unwrap_or_else(|| "YouTube source".to_owned()),
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
    Provider(#[from] PodcastError),
    #[error("YouTube channel is no longer available")]
    YoutubeSourceUnavailable,
    #[error(transparent)]
    Cleanup(#[from] super::downloads::CleanupError),
    #[error("podcast episode does not exist")]
    EpisodeNotFound,
}

/// Downloads one specific episode by id, synchronously. This is the same
/// body the auto-download branch of `refresh_to_root_with_download_progress`
/// runs for newly discovered episodes of an `auto_download` subscription —
/// factored out (Block H, MCP parity) so `music_manage_episodes`'s `download`
/// action, and (`MTP-44`/`POD-7`) the GTK worker's manual and device-sync
/// preparation downloads, all drive the exact same download path instead of
/// a second one that could drift from it. Idempotent: an episode that
/// already has a downloaded file is reported `Downloaded` immediately
/// without a second network round trip.
///
/// `NET-1a`: gated per the episode's own source kind, not a blanket check —
/// a download is a network (RSS) or subprocess (yt-dlp) entry point in its
/// own right, so this is the one place that check lives now that every
/// caller funnels through here.
pub fn download_episode(
    conn: &Connection,
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    download_root: &Path,
    episode_id: i64,
    on_progress: &mut dyn FnMut(DownloadState),
) -> Result<DownloadState, PipelineError> {
    let episode = super::store::episode(conn, episode_id)?.ok_or(PipelineError::EpisodeNotFound)?;
    if episode.downloaded_path.is_some() {
        let bytes = episode.downloaded_bytes.unwrap_or(0).max(0) as u64;
        let state = DownloadState::Downloaded { bytes };
        on_progress(state.clone());
        return Ok(state);
    }
    let subscription = super::store::subscription(conn, episode.subscription_id)?
        .ok_or(PipelineError::EpisodeNotFound)?;
    let extension = super::downloads::extension_for(subscription.kind, &episode.audio_url);
    let destination = super::downloads::download_path(
        download_root,
        &subscription.feed_url,
        &episode.guid,
        extension,
    );
    on_progress(DownloadState::Queued);
    let mut state = DownloadState::Downloading {
        received_bytes: 0,
        total_bytes: None,
    };
    on_progress(state.clone());
    if !super::config::source_network_allowed(conn, episode.kind)? {
        let state = DownloadState::Failed {
            message: "this source is disabled".to_owned(),
        };
        on_progress(state.clone());
        return Ok(state);
    }
    let download = super::downloads::download_atomically(&destination, |temporary| {
        let mut report = |progress: DownloadProgress| {
            state = super::download_state::downloading(
                &state,
                progress.received_bytes,
                progress.total_bytes,
            );
            on_progress(state.clone());
        };
        match subscription.kind {
            PodcastKind::Rss => {
                feed_fetcher.download_with_progress(&episode.audio_url, temporary, &mut report)
            }
            PodcastKind::Youtube => {
                youtube_fetcher.download_with_progress(&episode.audio_url, temporary, &mut report)
            }
        }
    });
    match download {
        Ok(bytes) => {
            let Some(destination_path) = destination.to_str() else {
                remove_completed_download(&destination);
                tracing::warn!(episode_id, "podcast download path is not valid UTF-8");
                let state = DownloadState::Failed {
                    message: "podcast download path is not valid UTF-8".to_owned(),
                };
                on_progress(state.clone());
                return Ok(state);
            };
            let persisted = match super::downloads::persist_completed_if_active(
                conn,
                episode_id,
                destination_path,
                bytes,
            ) {
                Ok(persisted) => persisted,
                Err(error) => {
                    remove_completed_download(&destination);
                    let state = DownloadState::Failed {
                        message: error.to_string(),
                    };
                    on_progress(state.clone());
                    return Err(error.into());
                }
            };
            let state = if persisted {
                DownloadState::Downloaded { bytes }
            } else {
                remove_completed_download(&destination);
                DownloadState::Failed {
                    message: "podcast episode no longer exists".to_owned(),
                }
            };
            on_progress(state.clone());
            Ok(state)
        }
        Err(error) => {
            // POD-13: never let the raw provider error reach the UI or a
            // normal-level log line — it can echo the request URL (an
            // episode's `audio_url` may carry a private per-subscriber
            // token, `SRC-5`) or, for yt-dlp, a local download path. Log and
            // store only the classified reason.
            let reason = error.classify();
            tracing::warn!(episode_id, reason, "podcast download failed");
            let state = DownloadState::Failed {
                message: reason.to_owned(),
            };
            on_progress(state.clone());
            Ok(state)
        }
    }
}

pub fn refresh(
    conn: &Connection,
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    now: i64,
    force: bool,
) -> Result<RefreshSummary, PipelineError> {
    refresh_with_download_progress(
        conn,
        feed_fetcher,
        youtube_fetcher,
        now,
        force,
        &mut |_, _| {},
    )
}

pub fn refresh_with_download_progress(
    conn: &Connection,
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    now: i64,
    force: bool,
    on_download: &mut dyn FnMut(i64, DownloadState),
) -> Result<RefreshSummary, PipelineError> {
    refresh_to_root_with_download_progress(
        conn,
        feed_fetcher,
        youtube_fetcher,
        now,
        force,
        &super::downloads::default_download_root(),
        on_download,
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
    refresh_to_root_with_download_progress(
        conn,
        feed_fetcher,
        youtube_fetcher,
        now,
        force,
        download_root,
        &mut |_, _| {},
    )
}

fn refresh_to_root_with_download_progress(
    conn: &Connection,
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    now: i64,
    force: bool,
    download_root: &Path,
    on_download: &mut dyn FnMut(i64, DownloadState),
) -> Result<RefreshSummary, PipelineError> {
    let config = super::config::load(conn)?;
    let rss_allowed = super::config::source_network_allowed(conn, PodcastKind::Rss)?;
    let youtube_allowed = super::config::source_network_allowed(conn, PodcastKind::Youtube)?;
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
                if let Some(feed_url) = super::youtube::long_form_feed_url(&subscription.feed_url) {
                    feed_fetcher
                        .fetch_url(
                            &feed_url,
                            subscription.etag.as_deref(),
                            subscription.last_modified.as_deref(),
                        )
                        .and_then(|response| {
                            let feed =
                                super::feed::parse_feed(&response.body, OFFICIAL_YOUTUBE_LIMIT)?;
                            Ok((feed, Some(response)))
                        })
                } else {
                    youtube_fetcher
                        .list(&subscription.feed_url, config.youtube_import_count)
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
            let Some(upsert) = super::store::upsert_episode(conn, subscription.id, episode, now)?
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
                    // Already satisfied by `reclaim_download` above — no
                    // network round trip, and (unlike a genuinely fresh
                    // download) not counted toward this refresh's completed
                    // total.
                    continue;
                }
                let mut on_progress = |state: DownloadState| on_download(episode_id, state);
                let outcome = download_episode(
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
    super::downloads::enforce_cleanup(
        conn,
        download_root,
        config.cleanup_policy,
        config.keep_downloaded_default,
        now,
    )?;
    Ok(summary)
}

fn remove_completed_download(path: &Path) {
    if let Err(error) = std::fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(
                path = %path.display(),
                %error,
                "could not remove unclaimed podcast download"
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
        let bytes = std::fs::metadata(&path)
            .map_err(super::downloads::CleanupError::from)?
            .len()
            .min(i64::MAX as u64) as i64;
        super::store::set_downloaded_file(conn, episode_id, path.to_str(), Some(bytes))?;
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
#[path = "pipeline_youtube_tests.rs"]
mod youtube_tests;

#[cfg(test)]
#[path = "pipeline_refresh_tests.rs"]
mod tests;
