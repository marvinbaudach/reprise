//! YouTube episode fetch-before-playback wiring.

use std::rc::Rc;

use reprise_core::podcasts::download_state::DownloadState;
use reprise_core::podcasts::pipeline::{FeedFetcher, PipelineError, YoutubeFetcher};
use reprise_core::{db::Db, podcasts};

use crate::ui::player_controller::PlayerController;

use super::external_media_state::{fetch_download_outcome, EpisodeSource, FetchOutcome};

fn download_episode_for_playback(
    db: &Db,
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    download_root: &std::path::Path,
    episode_id: i64,
    on_progress: &mut dyn FnMut(DownloadState),
) -> Result<DownloadState, PipelineError> {
    podcasts::pipeline::download_episode_waiting(
        db,
        feed_fetcher,
        youtube_fetcher,
        download_root,
        episode_id,
        on_progress,
    )
}

impl PlayerController {
    /// Fetches the episode, then plays it from disk.
    ///
    /// A YouTube episode is played from a local file or not at all. Progress
    /// is forwarded to the source rows while the session stays resolving.
    pub(super) fn fetch_youtube(self: &Rc<Self>, generation: u64, episode_id: i64, resume_ms: i64) {
        let Some(database_path) = self.conn.path() else {
            self.fail_podcast(generation, "the active database has no persistent path");
            return;
        };
        let task = crate::ui::one_shot_task::spawn_with_progress(
            "reprise-youtube-fetch",
            move |publish| {
                let db = reprise_core::db::Db::open_migrated(Some(&database_path))
                    .map_err(|error| error.to_string())?;
                let config =
                    reprise_core::podcasts::config::load(&db).map_err(|error| error.to_string())?;
                let ytdlp = reprise_core::podcasts::ytdlp::YtDlp::discover_with_browser(
                    config.ytdlp_path.as_deref(),
                    config.youtube_browser,
                );
                download_episode_for_playback(
                    &db,
                    &podcasts::pipeline::HttpFeedFetcher,
                    &ytdlp,
                    &podcasts::downloads::default_download_root(),
                    episode_id,
                    &mut |state| publish(state),
                )
                .map_err(|error| error.to_string())
            },
        );
        let (progress, result) = match task {
            Ok(pair) => pair,
            Err(error) => {
                self.fail_podcast(generation, &error.to_string());
                return;
            }
        };
        let weak = Rc::downgrade(self);
        gtk4::glib::spawn_future_local(async move {
            while let Ok(state) = progress.recv().await {
                let Some(controller) = weak.upgrade() else {
                    return;
                };
                if !controller.external_generation_matches_podcast(generation) {
                    return;
                }
                controller.update_podcast_fetch_progress(generation, episode_id, &state);
            }
        });
        let weak = Rc::downgrade(self);
        gtk4::glib::spawn_future_local(async move {
            let Ok(result) = result.recv().await else {
                return;
            };
            let Some(controller) = weak.upgrade() else {
                return;
            };
            if !controller.external_generation_matches_podcast(generation) {
                return;
            }
            let path = reprise_core::podcasts::store::episode(&controller.conn, episode_id)
                .ok()
                .flatten()
                .and_then(|episode| episode.downloaded_path);
            match fetch_download_outcome(result, path) {
                FetchOutcome::Play(path) => {
                    let _ = controller.start_podcast_source(
                        generation,
                        episode_id,
                        EpisodeSource::File(path),
                        resume_ms,
                    );
                }
                FetchOutcome::Fail(message) => controller.fail_podcast(generation, &message),
            }
        });
    }

    fn update_podcast_fetch_progress(
        &self,
        generation: u64,
        episode_id: i64,
        state: &DownloadState,
    ) {
        if !self.external_generation_matches_podcast(generation) {
            return;
        }
        let callbacks = self.external.borrow().episode_download_callbacks.clone();
        for callback in callbacks {
            callback(episode_id, state.clone());
        }
    }
}

#[cfg(test)]
#[path = "external_media_fetch_tests.rs"]
mod tests;
