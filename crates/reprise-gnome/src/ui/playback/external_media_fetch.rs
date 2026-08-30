//! YouTube episode fetch-before-playback wiring.

use std::rc::Rc;

use reprise_core::podcasts::download_state::DownloadState;
use reprise_core::podcasts::pipeline::{FeedFetcher, PipelineError, YoutubeFetcher};
use reprise_core::{db::Db, podcasts};

use crate::ui::player_controller::PlayerController;

use super::external_media_state::{fetch_download_outcome_from_store, EpisodeSource, FetchOutcome};

/// `POD-13`: a provider error can echo the request URL, so only the
/// classified reason ever leaves the worker thread.
fn classification_reason(error: PipelineError) -> String {
    match error {
        PipelineError::Provider(provider) => provider.classify().to_owned(),
        other => other.to_string(),
    }
}

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
    pub(super) fn fetch_youtube(self: &Rc<Self>, generation: u64, episode_id: i64) {
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
            let stored = reprise_core::podcasts::store::episode(&controller.conn, episode_id)
                .map_err(|error| {
                    tracing::warn!(%error, episode_id, "could not read downloaded podcast episode");
                    error.to_string()
                });
            // The download stored the episode's category on its way past
            // (`AC-26`); without this the running session would keep the empty
            // one it started with and the Visual tab would only appear on the
            // *next* play.
            if let Ok(Some(episode)) = stored.as_ref() {
                controller.apply_resolved_category(
                    generation,
                    episode_id,
                    episode.media_category.clone(),
                );
            }
            let path = stored.map(|episode| episode.and_then(|episode| episode.downloaded_path));
            match fetch_download_outcome_from_store(result, path) {
                FetchOutcome::Play(path) => {
                    let _ = controller.start_podcast_source(
                        generation,
                        episode_id,
                        EpisodeSource::File(path),
                    );
                }
                FetchOutcome::Fail(message) => controller.fail_podcast(generation, &message),
            }
        });
    }

    /// `AC-26`: one landing point for "a category arrived for this episode".
    ///
    /// Both paths that can learn a category end here — the download that
    /// stores one as a side effect, and the classification spent on it
    /// deliberately — because the follow-up is the half that is easy to get
    /// wrong. The panel has to recompute the Visual tab *and* the spectrum
    /// source has to be switched on; doing only the first gives a tab that
    /// arrives late and then draws flat bars. `update_podcast_media_category`
    /// owns the generation and episode guards, so a stale answer from a
    /// session the user has already left lands here and changes nothing.
    pub(super) fn apply_resolved_category(
        &self,
        generation: u64,
        episode_id: i64,
        category: Option<String>,
    ) {
        let changed = self
            .external
            .borrow_mut()
            .update_podcast_media_category(generation, episode_id, category);
        if !changed {
            return;
        }
        self.notify_external_changed();
        if let Err(error) = self.sync_audio_reactive() {
            tracing::warn!(
                %error,
                episode_id,
                "a resolved category did not reach the spectrum source"
            );
        }
    }

    /// `AC-26`: spends one yt-dlp extraction on the episode's category.
    ///
    /// Only for an episode already on disk — anything still to be downloaded
    /// learns its category from the download itself. Playback never waits on
    /// this: the episode plays now and the Visual tab follows if the answer
    /// earns it.
    pub(super) fn classify_youtube_episode(self: &Rc<Self>, generation: u64, episode_id: i64) {
        let Some(database_path) = self.conn.path() else {
            return;
        };
        let task = crate::ui::one_shot_task::spawn("reprise-youtube-classify", move || {
            let db = Db::open_migrated(Some(&database_path)).map_err(|error| error.to_string())?;
            let config =
                reprise_core::podcasts::config::load(&db).map_err(|error| error.to_string())?;
            let ytdlp = reprise_core::podcasts::ytdlp::YtDlp::discover_with_browser(
                config.ytdlp_path.as_deref(),
                config.youtube_browser,
            );
            podcasts::classify_youtube_episode(&db, &ytdlp, episode_id)
                .map_err(classification_reason)
        });
        let Ok(result) = task else {
            return;
        };
        let weak = Rc::downgrade(self);
        gtk4::glib::spawn_future_local(async move {
            let Ok(outcome) = result.recv().await else {
                return;
            };
            let Some(controller) = weak.upgrade() else {
                return;
            };
            match outcome {
                Ok(category) => {
                    controller.apply_resolved_category(generation, episode_id, category);
                }
                Err(reason) => {
                    // An episode that cannot be classified simply keeps its
                    // unclassified state and stays without bars. Nothing about
                    // playback depends on this answer, so it is a warning and
                    // never a user-facing failure.
                    tracing::warn!(episode_id, reason, "could not classify a YouTube episode");
                }
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

#[cfg(test)]
#[path = "external_media_classify_tests.rs"]
mod classify_tests;
