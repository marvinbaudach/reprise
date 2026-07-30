//! Block H-D/H-E: batch episode mutations over MCP — download, remove, and
//! `wanted_on_device` (`MTP-40`). Every action reuses the exact core
//! facades the GNOME channel detail and device page call:
//! `podcasts::pipeline::download_episode` (the same function the refresh
//! pipeline's auto-download branch calls — Block H's extraction, not a
//! second download path), `podcasts::store::tombstone_episode`/
//! `commit_remove_episode` (`POD-6`), and `podcasts::wanted_on_device::
//! set_wanted_on_device` (`MTP-40`).

use std::path::Path;

use reprise_core::podcasts::pipeline::PipelineError;
use rmcp::schemars;
use serde::{Deserialize, Serialize};

use crate::data::{self, DataError};

const MAX_EPISODE_IDS: usize = 100;

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ManageEpisodesParams {
    /// One of: `download`, `remove`, `want_on_device`.
    pub action: String,
    /// Episode ids from `reprise://podcasts` or `music_get_channel_detail`.
    /// At most 100 per call.
    pub episode_ids: Vec<i64>,
    /// Required for `want_on_device`: whether these episodes should be
    /// marked wanted on the phone (`MTP-40`). Ignored for other actions.
    #[serde(default)]
    pub wanted: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct EpisodeOutcome {
    pub episode_id: i64,
    pub ok: bool,
    /// Present when `ok` is true and the action is `download`: the
    /// resulting state, one of `downloaded`, `failed`. Absent for `remove`
    /// and `want_on_device`, which have no download state of their own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_state: Option<&'static str>,
    /// Present only when `ok` is false — a short, path-free reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ManageEpisodesResult {
    pub action: &'static str,
    pub outcomes: Vec<EpisodeOutcome>,
}

impl ManageEpisodesResult {
    pub fn summary(&self) -> String {
        let ok = self.outcomes.iter().filter(|outcome| outcome.ok).count();
        format!(
            "{} of {} episode(s) {}",
            ok,
            self.outcomes.len(),
            match self.action {
                "download" => "downloaded",
                "remove" => "removed",
                _ => "updated",
            }
        )
    }
}

pub fn manage_episodes(
    path: &Path,
    granted_at_startup: bool,
    params: &ManageEpisodesParams,
) -> Result<ManageEpisodesResult, DataError> {
    let db = data::open(path)?;
    let allowed = crate::capability::sources_manage_effective(&db, granted_at_startup)
        .map_err(DataError::Db)?;
    if !allowed {
        return Err(DataError::CapabilityDenied("sources:manage"));
    }
    if params.episode_ids.is_empty() {
        return Err(DataError::InvalidInput(
            "episode_ids must not be empty".to_owned(),
        ));
    }
    if params.episode_ids.len() > MAX_EPISODE_IDS {
        return Err(DataError::InvalidInput(format!(
            "at most {MAX_EPISODE_IDS} episode_ids per call"
        )));
    }
    let action: &'static str = match params.action.as_str() {
        "download" => "download",
        "remove" => "remove",
        "want_on_device" => "want_on_device",
        other => {
            return Err(DataError::InvalidInput(format!(
                "unknown episode action '{other}'"
            )))
        }
    };
    if action == "want_on_device" && params.wanted.is_none() {
        return Err(DataError::InvalidInput(
            "wanted is required for want_on_device".to_owned(),
        ));
    }

    let download_root = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("podcasts");
    let outcomes = params
        .episode_ids
        .iter()
        .map(|&episode_id| match action {
            "download" => download_one(&db, &download_root, episode_id),
            "remove" => remove_one(&db, episode_id),
            _ => want_on_device_one(&db, episode_id, params.wanted.unwrap_or(false)),
        })
        .collect();

    Ok(ManageEpisodesResult { action, outcomes })
}

fn download_one(
    db: &reprise_core::db::Db,
    download_root: &Path,
    episode_id: i64,
) -> EpisodeOutcome {
    let Ok(Some(episode)) = reprise_core::podcasts::store::episode(db, episode_id) else {
        return EpisodeOutcome {
            episode_id,
            ok: false,
            download_state: None,
            error: Some("episode does not exist".to_owned()),
        };
    };
    // `NET-1a`: a download performs real network (RSS) or subprocess
    // (yt-dlp) work, so it is gated exactly like `refresh`'s auto-download
    // branch — the same `source_network_allowed` call, per the episode's
    // own kind rather than a blanket check.
    let allowed =
        reprise_core::podcasts::config::source_network_allowed(db, episode.kind).unwrap_or(false);
    if !allowed {
        return EpisodeOutcome {
            episode_id,
            ok: false,
            download_state: None,
            error: Some("this source is disabled in Reprise preferences".to_owned()),
        };
    }
    let ytdlp_path = match reprise_core::podcasts::config::load(db) {
        Ok(config) => config.ytdlp_path,
        Err(error) => {
            return EpisodeOutcome {
                episode_id,
                ok: false,
                download_state: None,
                error: Some(error.to_string()),
            }
        }
    };
    let ytdlp = reprise_core::podcasts::ytdlp::YtDlp::discover(ytdlp_path.as_deref());
    let feed_fetcher = reprise_core::podcasts::pipeline::HttpFeedFetcher;
    let result = reprise_core::podcasts::pipeline::download_episode(
        db,
        &feed_fetcher,
        &ytdlp,
        download_root,
        episode_id,
        &mut |_| {},
    );
    outcome_from_download_result(episode_id, result)
}

/// Pure, leak-safe mapping from `download_episode`'s result to the
/// response shape — split out from [`download_one`] so the sanitization
/// rule below is directly unit-testable without a real network/subprocess
/// call.
///
/// A failed download's underlying message can embed the raw provider
/// transport error, which in turn can echo the request URL — and an
/// episode's `audio_url` may carry a private per-subscriber token the same
/// way a feed URL can (`SRC-5`). Never forward it verbatim; a fixed,
/// generic reason matches how `source_actions::podcast_source_error`
/// already sanitizes every other podcast provider failure before it
/// reaches a response.
fn outcome_from_download_result(
    episode_id: i64,
    result: Result<reprise_core::podcasts::download_state::DownloadState, PipelineError>,
) -> EpisodeOutcome {
    use reprise_core::podcasts::download_state::DownloadState;
    match result {
        Ok(DownloadState::Downloaded { .. }) => EpisodeOutcome {
            episode_id,
            ok: true,
            download_state: Some("downloaded"),
            error: None,
        },
        Ok(DownloadState::Failed { message: _ }) => EpisodeOutcome {
            episode_id,
            ok: false,
            download_state: Some("failed"),
            error: Some("episode download failed".to_owned()),
        },
        Ok(_) => EpisodeOutcome {
            episode_id,
            ok: false,
            download_state: None,
            error: Some("download did not reach a terminal state".to_owned()),
        },
        Err(PipelineError::EpisodeNotFound) => EpisodeOutcome {
            episode_id,
            ok: false,
            download_state: None,
            error: Some("episode does not exist".to_owned()),
        },
        Err(error) => EpisodeOutcome {
            episode_id,
            ok: false,
            download_state: None,
            error: Some(error.to_string()),
        },
    }
}

fn remove_one(db: &reprise_core::db::Db, episode_id: i64) -> EpisodeOutcome {
    let Ok(Some(_)) = reprise_core::podcasts::store::episode(db, episode_id) else {
        return EpisodeOutcome {
            episode_id,
            ok: false,
            download_state: None,
            error: Some("episode does not exist".to_owned()),
        };
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64);
    // MCP has no undo toast, so unlike the GUI's ten-second-reversible
    // removal (`POD-6`), this commits immediately — matching how
    // `music_manage_podcasts`'s `remove` action already commits a
    // subscription without a second, MCP-only undo window.
    match reprise_core::podcasts::store::tombstone_episode(db, episode_id, now)
        .and_then(|_| reprise_core::podcasts::store::commit_remove_episode(db, episode_id))
    {
        Ok(_) => EpisodeOutcome {
            episode_id,
            ok: true,
            download_state: None,
            error: None,
        },
        Err(error) => EpisodeOutcome {
            episode_id,
            ok: false,
            download_state: None,
            error: Some(error.to_string()),
        },
    }
}

fn want_on_device_one(db: &reprise_core::db::Db, episode_id: i64, wanted: bool) -> EpisodeOutcome {
    match reprise_core::podcasts::wanted_on_device::set_wanted_on_device(db, episode_id, wanted) {
        Ok(true) => EpisodeOutcome {
            episode_id,
            ok: true,
            download_state: None,
            error: None,
        },
        Ok(false) => EpisodeOutcome {
            episode_id,
            ok: false,
            download_state: None,
            error: Some("episode does not exist".to_owned()),
        },
        Err(error) => EpisodeOutcome {
            episode_id,
            ok: false,
            download_state: None,
            error: Some(error.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::podcasts::download_state::DownloadState;
    use reprise_core::podcasts::feed::ParsedEpisode;
    use reprise_core::podcasts::store::{self, NewSubscription};
    use reprise_core::podcasts::PodcastKind;

    /// Response hygiene: a download failure's underlying message can carry
    /// the raw request URL (which, like a feed URL, may embed a private
    /// per-subscriber token — `SRC-5`), and it must never reach the caller.
    #[test]
    fn a_failed_download_never_forwards_its_raw_provider_message() {
        let leaking_result = Ok(DownloadState::Failed {
            message: "network request failed: https://audio.test/ep?token=secret-abc".into(),
        });

        let outcome = outcome_from_download_result(7, leaking_result);

        assert!(!outcome.ok);
        assert_eq!(outcome.download_state, Some("failed"));
        let error = outcome.error.expect("a failure carries a reason");
        assert!(!error.contains("token"));
        assert!(!error.contains("audio.test"));
        assert_eq!(error, "episode download failed");
    }

    /// The two outcomes must actually read differently — this is the guard
    /// the sanitizing rewrite above must not accidentally collapse.
    #[test]
    fn a_successful_download_reads_differently_from_a_failed_one() {
        let success = outcome_from_download_result(1, Ok(DownloadState::Downloaded { bytes: 5 }));
        let failure = outcome_from_download_result(
            1,
            Ok(DownloadState::Failed {
                message: "boom".into(),
            }),
        );

        assert!(success.ok);
        assert!(!failure.ok);
        assert_ne!(success.download_state, failure.download_state);
    }

    fn seeded_db() -> (tempfile::TempDir, std::path::PathBuf, i64) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reprise.db");
        let episode_id = {
            let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
            reprise_core::library::settings::set_bool(
                &db,
                crate::capability::CAP_SOURCES_MANAGE,
                true,
            )
            .unwrap();
            let subscription_id = store::add_or_restore(
                &db,
                &NewSubscription {
                    kind: PodcastKind::Rss,
                    feed_url: "https://feeds.test/show".into(),
                    title: "Show".into(),
                    author: None,
                    image_url: None,
                    auto_download: false,
                },
                1,
            )
            .unwrap();
            store::upsert_episode(
                &db,
                subscription_id,
                &ParsedEpisode {
                    guid: "ep-1".into(),
                    title: "Episode".into(),
                    image_url: None,
                    audio_url: "https://audio.test/ep.mp3".into(),
                    page_url: None,
                    published_at: Some(1),
                    duration_secs: Some(600),
                },
                1,
            )
            .unwrap()
            .unwrap()
            .episode_id
        };
        (dir, path, episode_id)
    }

    #[test]
    fn manage_episodes_is_denied_when_the_capability_is_not_granted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reprise.db");
        let _db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();

        let error = manage_episodes(
            &path,
            true,
            &ManageEpisodesParams {
                action: "remove".into(),
                episode_ids: vec![1],
                wanted: None,
            },
        )
        .unwrap_err();

        assert!(matches!(
            error,
            DataError::CapabilityDenied("sources:manage")
        ));
    }

    /// `MTP-40`: setting wanted on and off must produce genuinely different
    /// persisted state, not just a label.
    #[test]
    fn want_on_device_round_trips_and_the_two_states_actually_differ() {
        let (_dir, path, episode_id) = seeded_db();

        let set = manage_episodes(
            &path,
            true,
            &ManageEpisodesParams {
                action: "want_on_device".into(),
                episode_ids: vec![episode_id],
                wanted: Some(true),
            },
        )
        .unwrap();
        assert!(set.outcomes[0].ok);
        let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
        assert_eq!(
            reprise_core::podcasts::wanted_on_device::wanted_on_device(&db, episode_id).unwrap(),
            Some(true)
        );
        drop(db);

        let cleared = manage_episodes(
            &path,
            true,
            &ManageEpisodesParams {
                action: "want_on_device".into(),
                episode_ids: vec![episode_id],
                wanted: Some(false),
            },
        )
        .unwrap();
        assert!(cleared.outcomes[0].ok);
        let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
        assert_eq!(
            reprise_core::podcasts::wanted_on_device::wanted_on_device(&db, episode_id).unwrap(),
            Some(false)
        );
    }

    #[test]
    fn want_on_device_requires_the_wanted_field() {
        let (_dir, path, episode_id) = seeded_db();

        let error = manage_episodes(
            &path,
            true,
            &ManageEpisodesParams {
                action: "want_on_device".into(),
                episode_ids: vec![episode_id],
                wanted: None,
            },
        )
        .unwrap_err();

        assert!(matches!(error, DataError::InvalidInput(_)));
    }

    #[test]
    fn remove_commits_immediately_without_an_undo_window() {
        let (_dir, path, episode_id) = seeded_db();

        let result = manage_episodes(
            &path,
            true,
            &ManageEpisodesParams {
                action: "remove".into(),
                episode_ids: vec![episode_id],
                wanted: None,
            },
        )
        .unwrap();

        assert!(result.outcomes[0].ok);
        let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
        assert!(reprise_core::podcasts::store::episode(&db, episode_id)
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_batch_reports_per_id_outcomes_so_one_bad_id_does_not_fail_the_whole_call() {
        let (_dir, path, episode_id) = seeded_db();

        let result = manage_episodes(
            &path,
            true,
            &ManageEpisodesParams {
                action: "remove".into(),
                episode_ids: vec![episode_id, 999_999],
                wanted: None,
            },
        )
        .unwrap();

        assert!(result.outcomes[0].ok);
        assert!(!result.outcomes[1].ok);
        assert!(result.outcomes[1].error.is_some());
    }

    #[test]
    fn empty_episode_ids_is_rejected() {
        let (_dir, path, _episode_id) = seeded_db();

        let error = manage_episodes(
            &path,
            true,
            &ManageEpisodesParams {
                action: "remove".into(),
                episode_ids: Vec::new(),
                wanted: None,
            },
        )
        .unwrap_err();

        assert!(matches!(error, DataError::InvalidInput(_)));
    }
}
