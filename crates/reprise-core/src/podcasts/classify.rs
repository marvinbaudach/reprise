//! Classifying a YouTube episode that nothing has classified yet (`AC-26`).
//!
//! A download learns the category for free — yt-dlp already prints it as part
//! of the extraction the download performs anyway. That leaves one gap, and it
//! is permanent: an episode downloaded before Reprise captured categories has
//! an empty one, and a second download never happens. Those episodes would
//! stay unclassified forever, which under `AC-26` means they would stay
//! without Song Visuals forever.
//!
//! This module is the one place allowed to spend a yt-dlp call on the
//! classification alone. It is deliberately narrow: one episode, only YouTube,
//! only when the stored category is empty, and only ever called from the
//! playback path — never a sweep over a library.

use crate::db::Db;

use super::pipeline::{PipelineError, YoutubeFetcher};
use super::{EpisodeRow, PodcastKind};

/// What one extraction learned about an episode.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EpisodeClassification {
    pub media_category: Option<String>,
    pub duration_secs: Option<i64>,
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
        })
}

/// Whether `episode` is one this module may spend a request on.
///
/// An empty string counts as unclassified: the download path stores
/// `NULLIF(category, '')`, but a row written by an older version can still
/// carry one, and a blank category answers nothing.
pub fn needs_classification(episode: &EpisodeRow) -> bool {
    episode.kind == PodcastKind::Youtube
        && episode
            .media_category
            .as_deref()
            .is_none_or(|category| category.trim().is_empty())
}

/// Classifies `episode_id` and persists what came back.
///
/// `Ok(None)` means "nothing to store": the episode does not need
/// classification, or the extraction knew no category. Only `Ok(Some(_))`
/// reached the database, and only that answer may move a live session.
pub fn classify_youtube_episode(
    db: &Db,
    youtube_fetcher: &dyn YoutubeFetcher,
    episode_id: i64,
) -> Result<Option<String>, PipelineError> {
    let Some(episode) = super::store::episode(db, episode_id)? else {
        return Err(PipelineError::EpisodeNotFound);
    };
    if !needs_classification(&episode) {
        EpisodeClassification::clear_retry(db, episode_id);
        return Ok(None);
    }
    let classification = match youtube_fetcher.classify(&episode.audio_url) {
        Ok(classification) => classification,
        Err(error) => {
            EpisodeClassification::refresh_retry_deadline(db, episode_id, now_unix());
            return Err(error.into());
        }
    };
    let category = classification
        .media_category
        .filter(|category| !category.trim().is_empty());
    let Some(category) = category else {
        EpisodeClassification::refresh_retry_deadline(db, episode_id, now_unix());
        return Ok(None);
    };
    // The duration rides along because the extraction already carries it and
    // `save_youtube_resolution` only fills a duration that is still unknown.
    // Asking for it and then dropping it would be the wasteful half of a call
    // this module is spending anyway.
    if let Err(error) = super::store::save_youtube_resolution(
        db,
        episode_id,
        classification.duration_secs,
        Some(category.as_str()),
    ) {
        EpisodeClassification::refresh_retry_deadline(db, episode_id, now_unix());
        return Err(error.into());
    }
    EpisodeClassification::clear_retry(db, episode_id);
    Ok(Some(category))
}

#[cfg(test)]
mod retry_tests {
    use super::*;

    #[test]
    fn ac_26_an_empty_classification_waits_for_the_shared_retry_deadline() {
        let db = Db::open_in_memory().unwrap();

        assert!(EpisodeClassification::claim_retry(&db, 41, 100));
        assert!(!EpisodeClassification::claim_retry(&db, 41, 100));
        assert!(!EpisodeClassification::retry_due(&db, 41, 101));
        assert!(EpisodeClassification::retry_due(&db, 41, 102));
    }

    #[test]
    fn ac_26_the_frontend_and_worker_connections_share_the_retry() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("classification.db");
        let frontend = Db::open_migrated(Some(&path)).unwrap();
        let worker = Db::open_migrated(Some(&path)).unwrap();

        assert!(EpisodeClassification::claim_retry(&frontend, 42, 200));
        assert!(!EpisodeClassification::retry_due(&worker, 42, 201));
        assert!(EpisodeClassification::retry_due(&worker, 42, 202));
    }
}

#[cfg(test)]
#[path = "classify_tests.rs"]
mod tests;
