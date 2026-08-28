//! Per-subscription feed sync and its observable progress contract.

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rusqlite::Connection;

use super::{
    adopt_resolved_channel_url, clear_retry, previous_attempt, reclaim_download,
    record_failed_outcome_in, set_retry, FeedFetcher, PipelineError, RefreshSummary, RetryKey,
    YoutubeFetcher, OFFICIAL_YOUTUBE_LIMIT,
};
use crate::db::Db;
use crate::podcasts::config::PodcastConfig;
use crate::podcasts::refresh::RefreshPolicy;
use crate::podcasts::store::FetchSuccess;
use crate::podcasts::{PodcastError, PodcastKind, SubscriptionRow};
use crate::source_error::SourceErrorKind;

#[derive(Clone, Debug, Default)]
pub struct SyncAbort {
    cancelled: Arc<AtomicBool>,
}

impl SyncAbort {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncError {
    Source(SourceErrorKind),
    Database,
    SubscriptionUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncProgress {
    Started,
    FeedRead { episodes_found: usize },
    FetchingArtwork,
    Done(RefreshSummary),
    Failed(SyncError),
}

struct FeedRead {
    feed: crate::podcasts::feed::ParsedFeed,
    response: Option<crate::podcasts::http::Response>,
    resolved_channel_url: Option<String>,
}

enum FeedReadOutcome {
    Changed(FeedRead),
    NotModified {
        resolved_channel_url: Option<String>,
    },
}

#[allow(clippy::too_many_arguments)]
pub fn sync_subscription(
    db: &Db,
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    now: i64,
    subscription_id: i64,
    abort: &SyncAbort,
    on_progress: &mut dyn FnMut(SyncProgress),
) -> Result<RefreshSummary, PipelineError> {
    on_progress(SyncProgress::Started);
    let result = sync_subscription_in(
        db,
        feed_fetcher,
        youtube_fetcher,
        now,
        subscription_id,
        abort,
        on_progress,
    );
    if let Err(error) = &result {
        if !matches!(error, PipelineError::SyncAborted) {
            on_progress(SyncProgress::Failed(match error {
                PipelineError::SubscriptionNotFound => SyncError::SubscriptionUnavailable,
                _ => SyncError::Database,
            }));
        }
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn sync_subscription_in(
    db: &Db,
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    now: i64,
    subscription_id: i64,
    abort: &SyncAbort,
    on_progress: &mut dyn FnMut(SyncProgress),
) -> Result<RefreshSummary, PipelineError> {
    abort_if_requested(abort)?;
    let conn = db.conn();
    let config = crate::podcasts::config::load_in(conn)?;
    let rss_allowed = crate::podcasts::config::source_network_allowed_in(conn, PodcastKind::Rss)?;
    let youtube_allowed =
        crate::podcasts::config::source_network_allowed_in(conn, PodcastKind::Youtube)?;
    let Some(subscription) = crate::podcasts::store::subscription_in(conn, subscription_id)? else {
        return Err(PipelineError::SubscriptionNotFound);
    };
    if subscription.removed_at.is_some() {
        return Err(PipelineError::SubscriptionNotFound);
    }
    let mut summary = RefreshSummary::default();
    refresh_one_in(
        conn,
        feed_fetcher,
        youtube_fetcher,
        now,
        RefreshPolicy::Force,
        &crate::podcasts::downloads::default_download_root(),
        &config,
        rss_allowed,
        youtube_allowed,
        &subscription,
        &mut summary,
        abort,
        on_progress,
    )?;
    abort_if_requested(abort)?;
    crate::podcasts::downloads::enforce_cleanup_in(
        conn,
        &crate::podcasts::downloads::default_download_root(),
        config.cleanup_policy,
        config.keep_downloaded_default,
        now,
    )?;
    if summary.failures.is_empty() {
        on_progress(SyncProgress::Done(summary.clone()));
    }
    Ok(summary)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn refresh_one_in(
    conn: &Connection,
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    now: i64,
    policy: RefreshPolicy,
    download_root: &Path,
    config: &PodcastConfig,
    rss_allowed: bool,
    youtube_allowed: bool,
    subscription: &SubscriptionRow,
    summary: &mut RefreshSummary,
    abort: &SyncAbort,
    on_progress: &mut dyn FnMut(SyncProgress),
) -> Result<(), PipelineError> {
    abort_if_requested(abort)?;
    summary.attempted += 1;
    let retry_key = RetryKey {
        connection: std::ptr::from_ref(conn).addr(),
        subscription_id: subscription.id,
    };
    let read = read_feed(
        feed_fetcher,
        youtube_fetcher,
        config,
        rss_allowed,
        youtube_allowed,
        subscription,
    );
    let read = match read {
        Ok(FeedReadOutcome::Changed(read)) => read,
        Ok(FeedReadOutcome::NotModified {
            resolved_channel_url,
        }) => {
            abort_if_requested(abort)?;
            let transaction = conn.unchecked_transaction()?;
            if let Some(url) = resolved_channel_url.as_deref() {
                adopt_resolved_channel_url(&transaction, subscription.id, url)?;
            }
            crate::podcasts::store::update_fetch_not_modified_in(
                &transaction,
                subscription.id,
                now,
            )?;
            abort_if_requested(abort)?;
            transaction.commit()?;
            clear_retry(retry_key);
            summary.not_modified += 1;
            return Ok(());
        }
        Err(error) => {
            record_failure(conn, subscription, now, policy, retry_key, &error, summary)?;
            on_progress(SyncProgress::Failed(SyncError::Source(
                SourceErrorKind::from(&error),
            )));
            return Ok(());
        }
    };

    for episodes_found in 1..=read.feed.episodes.len() {
        abort_if_requested(abort)?;
        on_progress(SyncProgress::FeedRead { episodes_found });
    }
    abort_if_requested(abort)?;
    on_progress(SyncProgress::FetchingArtwork);
    abort_if_requested(abort)?;

    let transaction = conn.unchecked_transaction()?;
    let active = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM podcast_subscriptions WHERE id = ?1 AND removed_at IS NULL)",
        [subscription.id],
        |row| row.get::<_, bool>(0),
    )?;
    if !active {
        return Err(PipelineError::SyncAborted);
    }
    if let Some(url) = read.resolved_channel_url.as_deref() {
        adopt_resolved_channel_url(&transaction, subscription.id, url)?;
    }
    let baseline = crate::podcasts::store::future_only_baseline_in(&transaction, subscription.id)?
        .into_iter()
        .collect::<HashSet<_>>();
    let first_seen_at = if matches!(
        subscription.last_outcome.as_deref(),
        Some("ok" | "not_modified")
    ) {
        now
    } else {
        subscription.added_at
    };
    for episode in &read.feed.episodes {
        abort_if_requested(abort)?;
        if baseline.contains(&episode.guid) {
            continue;
        }
        let Some(upsert) = crate::podcasts::store::upsert_episode_in(
            &transaction,
            subscription.id,
            episode,
            first_seen_at,
        )?
        else {
            continue;
        };
        if upsert.inserted {
            summary.episodes_inserted += 1;
        } else {
            summary.episodes_updated += 1;
        }
        reclaim_download(
            &transaction,
            download_root,
            subscription,
            episode,
            upsert.episode_id,
        )?;
    }
    abort_if_requested(abort)?;
    let response = read.response.as_ref();
    crate::podcasts::store::update_fetch_success_in(
        &transaction,
        subscription.id,
        now,
        FetchSuccess {
            etag: response.and_then(|value| value.etag.as_deref()),
            last_modified: response.and_then(|value| value.last_modified.as_deref()),
            title: match subscription.kind {
                PodcastKind::Rss => read.feed.title.as_deref(),
                PodcastKind::Youtube => crate::podcasts::youtube::subscription_title(&read.feed),
            },
            author: read.feed.author.as_deref(),
            image_url: read.feed.image_url.as_deref(),
        },
    )?;
    abort_if_requested(abort)?;
    transaction.commit()?;
    clear_retry(retry_key);
    summary.refreshed += 1;
    Ok(())
}

fn read_feed(
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    config: &PodcastConfig,
    rss_allowed: bool,
    youtube_allowed: bool,
    subscription: &SubscriptionRow,
) -> Result<FeedReadOutcome, PodcastError> {
    match subscription.kind {
        PodcastKind::Rss if rss_allowed => feed_fetcher
            .fetch(subscription)
            .and_then(|response| {
                let feed = crate::podcasts::feed::parse_feed(&response.body, config.import_count)?;
                Ok(FeedReadOutcome::Changed(FeedRead {
                    feed,
                    response: Some(response),
                    resolved_channel_url: None,
                }))
            })
            .or_else(|error| match error {
                PodcastError::NotModified => Ok(FeedReadOutcome::NotModified {
                    resolved_channel_url: None,
                }),
                error => Err(error),
            }),
        PodcastKind::Rss => Err(PodcastError::Disabled(
            "RSS podcasts are disabled".to_owned(),
        )),
        PodcastKind::Youtube if youtube_allowed => {
            read_youtube_feed(feed_fetcher, youtube_fetcher, config, subscription)
        }
        PodcastKind::Youtube => Err(PodcastError::Disabled(
            "YouTube sources are disabled".to_owned(),
        )),
    }
}

fn read_youtube_feed(
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    config: &PodcastConfig,
    subscription: &SubscriptionRow,
) -> Result<FeedReadOutcome, PodcastError> {
    let channel_url =
        if crate::podcasts::youtube::long_form_feed_url(&subscription.feed_url).is_some() {
            subscription.feed_url.clone()
        } else {
            youtube_fetcher
                .resolve_channel_url(&subscription.feed_url)?
                .unwrap_or_else(|| subscription.feed_url.clone())
        };
    let resolved_channel_url = (channel_url != subscription.feed_url).then(|| channel_url.clone());
    if let Some(feed_url) = crate::podcasts::youtube::long_form_feed_url(&channel_url) {
        let official = feed_fetcher
            .fetch_url(
                &feed_url,
                subscription.etag.as_deref(),
                subscription.last_modified.as_deref(),
            )
            .and_then(|response| {
                let feed =
                    crate::podcasts::feed::parse_feed(&response.body, OFFICIAL_YOUTUBE_LIMIT)?;
                Ok(FeedReadOutcome::Changed(FeedRead {
                    feed,
                    response: Some(response),
                    resolved_channel_url: resolved_channel_url.clone(),
                }))
            });
        match official {
            Ok(read) => Ok(read),
            Err(PodcastError::NotModified) => Ok(FeedReadOutcome::NotModified {
                resolved_channel_url,
            }),
            Err(_) => youtube_fetcher
                .list(&channel_url, config.youtube_import_count)
                .map(|feed| {
                    FeedReadOutcome::Changed(FeedRead {
                        feed,
                        response: None,
                        resolved_channel_url,
                    })
                }),
        }
    } else {
        youtube_fetcher
            .list(&channel_url, config.youtube_import_count)
            .map(|feed| {
                FeedReadOutcome::Changed(FeedRead {
                    feed,
                    response: None,
                    resolved_channel_url,
                })
            })
    }
}

fn record_failure(
    conn: &Connection,
    subscription: &SubscriptionRow,
    now: i64,
    policy: RefreshPolicy,
    retry_key: RetryKey,
    error: &PodcastError,
    summary: &mut RefreshSummary,
) -> Result<(), rusqlite::Error> {
    tracing::warn!(subscription_id = subscription.id, %error, "podcast refresh failed");
    let retry = if matches!(policy, RefreshPolicy::Force) {
        None
    } else {
        crate::podcasts::refresh::next_retry(error, previous_attempt(retry_key), now)
    };
    if retry.is_some() {
        record_failed_outcome_in(conn, subscription.id)?;
    } else {
        crate::podcasts::store::update_fetch_failed_in(conn, subscription.id, now)?;
    }
    set_retry(retry_key, retry);
    summary.push_failure(subscription, error);
    Ok(())
}

fn abort_if_requested(abort: &SyncAbort) -> Result<(), PipelineError> {
    if abort.is_cancelled() {
        Err(PipelineError::SyncAborted)
    } else {
        Ok(())
    }
}
