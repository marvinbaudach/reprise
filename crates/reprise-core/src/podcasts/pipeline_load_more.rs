//! Explicit extended YouTube listing import.

use super::*;
use crate::podcasts::store;

pub fn load_more_youtube(
    conn: &Connection,
    youtube_fetcher: &dyn YoutubeFetcher,
    subscription_id: i64,
    end: usize,
    now: i64,
) -> Result<usize, PipelineError> {
    let subscription = store::subscription(conn, subscription_id)?
        .filter(|subscription| {
            subscription.removed_at.is_none() && subscription.kind == PodcastKind::Youtube
        })
        .ok_or(PipelineError::YoutubeSourceUnavailable)?;
    // NET-1a: this explicit user action is a network entry point too, not
    // just the periodic refresh.
    if !crate::podcasts::config::source_network_allowed(conn, PodcastKind::Youtube)? {
        return Err(PipelineError::YoutubeSourceUnavailable);
    }
    let end = end.clamp(1, 40);
    let feed = youtube_fetcher.list_range(&subscription.feed_url, end)?;
    let baseline = store::future_only_baseline(conn, subscription.id)?
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let mut changed = 0;
    for episode in &feed.episodes {
        if baseline.contains(&episode.guid) {
            continue;
        }
        if store::upsert_episode(conn, subscription.id, episode, now)?.is_some() {
            changed += 1;
        }
    }
    Ok(changed)
}
