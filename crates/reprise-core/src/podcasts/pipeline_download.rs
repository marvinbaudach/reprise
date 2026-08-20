//! One download executor shared by refresh, GTK, device sync, and MCP.

use std::path::Path;

use rusqlite::Connection;

use crate::db::Db;

use super::{FeedFetcher, PipelineError, YoutubeFetcher};
use crate::podcasts::{
    config,
    download_state::{self, DownloadProgress, DownloadState},
    downloads, episode_tags, store, PodcastError, PodcastKind,
};

/// Downloads one specific episode by id, synchronously. This is the same
/// executor the background fill-up, `music_manage_episodes`'s `download`
/// action, and (`MTP-44`/`POD-7`) the GTK worker's manual and device-sync
/// preparation downloads all drive, instead of maintaining paths that could
/// drift from each other. Idempotent: an episode that already has a downloaded
/// file is reported `Downloaded` immediately without a second network round
/// trip.
///
/// `NET-1a`: gated per the episode's own source kind, not a blanket check —
/// a download is a network (RSS) or subprocess (yt-dlp) entry point in its
/// own right, so this is the one place that check lives now that every
/// caller funnels through here.
pub fn download_episode(
    db: &Db,
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    download_root: &Path,
    episode_id: i64,
    on_progress: &mut dyn FnMut(DownloadState),
) -> Result<DownloadState, PipelineError> {
    download_episode_in(
        db.conn(),
        feed_fetcher,
        youtube_fetcher,
        download_root,
        episode_id,
        on_progress,
    )
}

pub(super) fn download_episode_in(
    conn: &Connection,
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    download_root: &Path,
    episode_id: i64,
    on_progress: &mut dyn FnMut(DownloadState),
) -> Result<DownloadState, PipelineError> {
    let episode = store::episode_in(conn, episode_id)?.ok_or(PipelineError::EpisodeNotFound)?;
    if episode.downloaded_path.is_some() {
        let bytes = episode.downloaded_bytes.unwrap_or(0).max(0) as u64;
        let state = DownloadState::Downloaded { bytes };
        on_progress(state.clone());
        return Ok(state);
    }
    // Held for the rest of this call. A concurrent caller gets
    // `DownloadAlreadyRunning` rather than a second run over the same `.part`
    // file.
    let Some(_claim) = super::super::download_claims::claim(episode_id) else {
        return Err(PipelineError::DownloadAlreadyRunning);
    };
    let subscription = store::subscription_in(conn, episode.subscription_id)?
        .ok_or(PipelineError::EpisodeNotFound)?;
    let extension = downloads::extension_for(subscription.kind, &episode.audio_url);
    let destination = downloads::download_path(
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
    if !config::source_network_allowed_in(conn, episode.kind)? {
        let state = DownloadState::Failed {
            message: "this source is disabled".to_owned(),
        };
        on_progress(state.clone());
        return Ok(state);
    }
    let tag_set = episode_tags::EpisodeTagSet {
        title: episode.title.clone(),
        show: subscription.title.clone(),
        artist: subscription
            .author
            .clone()
            .filter(|author| !author.trim().is_empty())
            .unwrap_or_else(|| subscription.title.clone()),
        date: episode_tags::episode_date(episode.published_at, episode.first_seen_at),
    };
    let mut media_category = None;
    let download = downloads::download_atomically(&destination, |temporary| {
        let mut report = |progress: DownloadProgress| {
            state =
                download_state::downloading(&state, progress.received_bytes, progress.total_bytes);
            on_progress(state.clone());
        };
        match subscription.kind {
            PodcastKind::Rss => {
                feed_fetcher.download_with_progress(&episode.audio_url, temporary, &mut report)?;
            }
            PodcastKind::Youtube => {
                let metadata = youtube_fetcher.download_with_metadata_and_progress(
                    &episode.audio_url,
                    temporary,
                    &mut report,
                )?;
                media_category = metadata
                    .categories
                    .into_iter()
                    .next()
                    .filter(|category| !category.is_empty());
            }
        }
        // `POD-17`: tag the temporary, never the published file. Lofty
        // rewrites Ogg and FLAC by truncating first, so a write that fails
        // part-way destroys the `.part` — which is why a failed write fails
        // the whole download here: `download_atomically` then deletes the
        // temporary and the episode stays downloadable, instead of the
        // truncated file being measured and published as a finished
        // episode. A container Reprise cannot tag at all is the opposite
        // case: nothing was written, and the download is published untagged.
        episode_tags::tag_download(temporary, &tag_set, episode_id)
            .map_err(|_| PodcastError::TagWrite)?;
        Ok(())
    });
    match download {
        Ok(bytes) => persist_download(
            conn,
            episode_id,
            &destination,
            bytes,
            media_category.as_deref(),
            on_progress,
        ),
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

fn persist_download(
    conn: &Connection,
    episode_id: i64,
    destination: &Path,
    bytes: u64,
    media_category: Option<&str>,
    on_progress: &mut dyn FnMut(DownloadState),
) -> Result<DownloadState, PipelineError> {
    let Some(destination_path) = destination.to_str() else {
        remove_completed_download(episode_id, destination);
        tracing::warn!(episode_id, "podcast download path is not valid UTF-8");
        let state = DownloadState::Failed {
            message: "podcast download path is not valid UTF-8".to_owned(),
        };
        on_progress(state.clone());
        return Ok(state);
    };
    let persisted = match downloads::persist_completed_with_category_if_active_in(
        conn,
        episode_id,
        destination_path,
        bytes,
        media_category,
    ) {
        Ok(persisted) => persisted,
        Err(error) => {
            remove_completed_download(episode_id, destination);
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
        remove_completed_download(episode_id, destination);
        DownloadState::Failed {
            message: "podcast episode no longer exists".to_owned(),
        }
    };
    on_progress(state.clone());
    Ok(state)
}

pub(super) fn remove_completed_download(episode_id: i64, path: &Path) {
    if let Err(error) = std::fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            // POD-13: never let a local filesystem path reach a normal-level
            // log line — log the episode id, which identifies the row without
            // exposing the on-disk layout.
            tracing::warn!(
                episode_id,
                %error,
                "could not remove unclaimed podcast download"
            );
        }
    }
}
