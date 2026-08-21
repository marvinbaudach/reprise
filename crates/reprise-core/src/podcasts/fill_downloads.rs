//! Keeping the newest N episodes of every subscription on disk.
//!
//! The mirror image of `downloads::cleanup_candidates`: that one deletes what
//! ranks beyond N, this one fetches what is missing within N. Both read the
//! same `keep_downloaded` and the same `downloads::NEWEST_EPISODE_FIRST`
//! ordering, over deliberately different populations — see that constant's
//! comment.

use std::path::Path;

use rusqlite::Connection;

use super::download_state::DownloadState;
use super::downloads::{self, NEWEST_EPISODE_FIRST};
use super::pipeline::{FeedFetcher, PipelineError, YoutubeFetcher};
use crate::db::Db;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FillSummary {
    pub downloaded: usize,
    pub failed: usize,
}

/// The episodes that ought to be on disk and are not.
///
/// Ranks over *all* live episodes, not only downloaded ones — the opposite of
/// the cleanup, and for the opposite reason: the job here is to find what is
/// missing, so a missing episode must occupy its rank position.
///
/// Played episodes are excluded rather than replaced. Replacing them would pull
/// the (N+1)th episode into the download set while the cleanup still ranks it
/// outside — with `CleanupPolicy::DeletePlayedAfter7Days` the two would then
/// delete and re-fetch the same episode forever.
pub(crate) fn missing_episode_ids_in(
    conn: &Connection,
    default_keep_downloaded: usize,
) -> Result<Vec<i64>, rusqlite::Error> {
    let sql = format!(
        "SELECT id, keep_downloaded, episode_rank, downloaded_path, played_at FROM (
           SELECT e.id, s.keep_downloaded, e.downloaded_path, e.played_at,
                  ROW_NUMBER() OVER (
                    PARTITION BY e.subscription_id
                    ORDER BY {NEWEST_EPISODE_FIRST}
                  ) AS episode_rank
           FROM podcast_episodes e
           JOIN podcast_subscriptions s ON s.id = e.subscription_id
           WHERE s.removed_at IS NULL
             AND e.removed_at IS NULL
         )
         ORDER BY episode_rank, id"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<i64>>(4)?,
        ))
    })?;
    let mut missing = Vec::new();
    for row in rows {
        let (episode_id, keep_override, episode_rank, downloaded_path, played_at) = row?;
        let keep = downloads::resolve_keep_downloaded(default_keep_downloaded, keep_override);
        // `0` is unlimited (`E-9`), never "keep none".
        if keep != 0 && episode_rank > keep as i64 {
            continue;
        }
        if downloaded_path.is_some() || played_at.is_some() {
            continue;
        }
        missing.push(episode_id);
    }
    Ok(missing)
}

/// Downloads everything `missing_episode_ids_in` reports, one episode at a
/// time, reporting each state change through `on_progress`.
///
/// Runs to completion rather than under a per-run cap: a cap would leave the
/// target unreached until the next refresh hours later, and the caller runs
/// this off the refresh precisely so a long run costs nobody anything.
/// Per-episode terminal failures and errors increment `FillSummary::failed`;
/// only setup failures that prevent taking the batch snapshot abort the run.
pub fn fill_downloads(
    db: &Db,
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    download_root: &Path,
    on_progress: &mut dyn FnMut(i64, DownloadState),
) -> Result<FillSummary, PipelineError> {
    let config = super::config::load(db)?;
    let episode_ids = {
        let conn = db.conn();
        missing_episode_ids_in(conn, config.keep_downloaded_default)?
    };
    let mut summary = FillSummary::default();
    for episode_id in episode_ids {
        let mut report = |state: DownloadState| on_progress(episode_id, state);
        let outcome = super::pipeline::download_episode(
            db,
            feed_fetcher,
            youtube_fetcher,
            download_root,
            episode_id,
            &mut report,
        );
        match outcome {
            Ok(DownloadState::Downloaded { .. }) => summary.downloaded += 1,
            Ok(DownloadState::Failed { .. }) => summary.failed += 1,
            // Another caller — the download button, or playback — already has
            // this episode in flight. Not this run's job and not a failure.
            Err(PipelineError::DownloadAlreadyRunning) => {}
            Err(_) => summary.failed += 1,
            Ok(_) => {}
        }
    }
    Ok(summary)
}
