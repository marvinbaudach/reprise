//! Block H-D: read-only YouTube/podcast channel detail over MCP — the
//! episode window, the Shorts filter, and per-row/aggregate download
//! figures. Shares `reprise_core::podcasts::channel_window` with GTK's
//! `youtube_channel_detail.rs` so both surfaces window, filter and total the
//! exact same way instead of drifting apart (`POD-10`/`POD-11`).

use std::collections::BTreeMap;
use std::path::Path;

use reprise_core::podcasts::channel_window;
use rmcp::schemars;
use serde::{Deserialize, Serialize};

use crate::data::{self, DataError};
use crate::source_data::{download_state_dto, local_availability_name, DownloadStateDto};

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct GetChannelDetailParams {
    /// Subscription id from `reprise://podcasts` or `music_search_sources`.
    pub subscription_id: i64,
    /// Whether Shorts (YouTube entries at or under 180s) are included in
    /// the window. Defaults to `false`, matching the GTK channel detail's
    /// default (`POD-10`) unless overridden by the Online sources "Hide
    /// Shorts" preference — this tool does not read that preference and
    /// always defaults to hidden, so pass it explicitly to match a specific
    /// user's current setting.
    #[serde(default)]
    pub show_shorts: Option<bool>,
    /// How many episodes to include in the window after the Shorts filter.
    /// Defaults to 10 (`POD-10`'s initial window); pass 40 or higher for
    /// the "Load more" window.
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct ChannelDetailResult {
    pub subscription_id: i64,
    pub kind: &'static str,
    pub title: String,
    /// `POD-11`'s header summary: `shown`/`available` reflect the current
    /// window and Shorts filter; `downloaded_count`/`downloaded_bytes` are
    /// computed over the whole channel regardless of window or filter, so
    /// the total reads as real disk usage.
    pub shown: usize,
    pub available: usize,
    pub downloaded_count: usize,
    pub downloaded_bytes: u64,
    pub episodes: Vec<ChannelEpisodeDto>,
}

#[derive(Debug, Serialize)]
pub struct ChannelEpisodeDto {
    pub id: i64,
    pub title: String,
    pub published_at: Option<i64>,
    pub duration_secs: Option<i64>,
    pub is_short: bool,
    pub download_state: DownloadStateDto,
    pub local_availability: &'static str,
}

pub fn channel_detail(
    path: &Path,
    params: &GetChannelDetailParams,
) -> Result<ChannelDetailResult, DataError> {
    let db = data::open(path)?;
    data::require_read(&db)?;
    if params.subscription_id <= 0 {
        return Err(DataError::InvalidInput(
            "subscription_id must be positive".to_owned(),
        ));
    }
    let subscription = reprise_core::podcasts::store::subscription(&db, params.subscription_id)
        .map_err(DataError::Db)?
        .filter(|row| row.removed_at.is_none())
        .ok_or_else(|| DataError::InvalidInput("podcast subscription does not exist".to_owned()))?;
    let episodes =
        reprise_core::podcasts::query::episodes_for_subscription(&db, params.subscription_id)
            .map_err(DataError::Db)?;

    let show_shorts = params.show_shorts.unwrap_or(false);
    let limit = params.limit.unwrap_or(channel_window::INITIAL_WINDOW);
    let window = channel_window::visible_window(&episodes, show_shorts, limit);
    let available = channel_window::available_count(&episodes, show_shorts);

    let download_states = episodes
        .iter()
        .map(|episode| (episode.id, crate::source_data::download_state_for(episode)))
        .collect::<BTreeMap<_, _>>();
    let summary = channel_window::channel_download_summary(
        window.len(),
        available,
        &episodes,
        &download_states,
    );

    Ok(ChannelDetailResult {
        subscription_id: subscription.id,
        kind: podcast_kind(subscription.kind),
        title: subscription.title,
        shown: summary.shown,
        available: summary.available,
        downloaded_count: summary.downloaded_count,
        downloaded_bytes: summary.downloaded_bytes,
        episodes: window
            .into_iter()
            .map(|episode| ChannelEpisodeDto {
                id: episode.id,
                title: episode.title.clone(),
                published_at: episode.published_at,
                duration_secs: episode.duration_secs,
                is_short: channel_window::is_short(episode),
                download_state: download_state_dto(episode),
                local_availability: local_availability_name(episode),
            })
            .collect(),
    })
}

fn podcast_kind(kind: reprise_core::podcasts::PodcastKind) -> &'static str {
    match kind {
        reprise_core::podcasts::PodcastKind::Rss => "rss",
        reprise_core::podcasts::PodcastKind::Youtube => "youtube",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::podcasts::feed::ParsedEpisode;
    use reprise_core::podcasts::store::{self, NewSubscription};
    use reprise_core::podcasts::PodcastKind;

    fn seeded_channel(path: &Path, episode_count: i64) -> i64 {
        let db = reprise_core::db::Db::open_migrated(Some(path)).unwrap();
        let subscription_id = store::add_or_restore(
            &db,
            &NewSubscription {
                kind: PodcastKind::Youtube,
                feed_url: "https://youtube.test/channel/UC-test".into(),
                title: "Test Channel".into(),
                author: None,
                image_url: None,
                auto_download: false,
            },
            1,
        )
        .unwrap();
        for id in 1..=episode_count {
            store::upsert_episode(
                &db,
                subscription_id,
                &ParsedEpisode {
                    guid: format!("video-{id}"),
                    title: format!("Video {id}"),
                    image_url: None,
                    audio_url: format!("https://youtube.test/watch?v={id}"),
                    page_url: None,
                    published_at: Some(id),
                    duration_secs: Some(if id == episode_count { 60 } else { 600 }),
                },
                id,
            )
            .unwrap();
        }
        subscription_id
    }

    #[test]
    fn channel_detail_hides_shorts_by_default_and_reveals_them_on_request() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reprise.db");
        let subscription_id = seeded_channel(&path, 3);

        let hidden = channel_detail(
            &path,
            &GetChannelDetailParams {
                subscription_id,
                show_shorts: None,
                limit: None,
            },
        )
        .unwrap();
        assert_eq!(hidden.available, 2, "the newest video (id 3) is a Short");
        assert!(hidden.episodes.iter().all(|episode| !episode.is_short));

        let shown = channel_detail(
            &path,
            &GetChannelDetailParams {
                subscription_id,
                show_shorts: Some(true),
                limit: None,
            },
        )
        .unwrap();
        assert_eq!(shown.available, 3);
        assert_ne!(
            hidden.available, shown.available,
            "the Shorts filter must actually change the result"
        );
    }

    #[test]
    fn channel_detail_reports_real_download_sizes_never_invented_for_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reprise.db");
        let subscription_id = seeded_channel(&path, 2);
        let episode_id = {
            let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
            reprise_core::podcasts::query::episodes_for_subscription(&db, subscription_id).unwrap()
                [0]
            .id
        };
        let audio = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(audio.path(), b"twelve-bytes").unwrap();
        {
            let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
            reprise_core::podcasts::downloads::set_downloaded_file(
                &db,
                episode_id,
                audio.path().to_str(),
                Some(12),
            )
            .unwrap();
        }

        let detail = channel_detail(
            &path,
            &GetChannelDetailParams {
                subscription_id,
                show_shorts: Some(true),
                limit: None,
            },
        )
        .unwrap();

        let downloaded = detail
            .episodes
            .iter()
            .find(|episode| episode.id == episode_id)
            .unwrap();
        assert_eq!(downloaded.download_state.state, "downloaded");
        assert_eq!(downloaded.download_state.bytes, Some(12));
        assert_eq!(downloaded.local_availability, "available");
        assert_eq!(detail.downloaded_count, 1);
        assert_eq!(detail.downloaded_bytes, 12);

        let others_not_downloaded = detail
            .episodes
            .iter()
            .filter(|episode| episode.id != episode_id)
            .all(|episode| episode.download_state.bytes.is_none());
        assert!(
            others_not_downloaded,
            "an episode with no file must never carry an invented size"
        );
    }

    #[test]
    fn channel_detail_rejects_an_unknown_subscription() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reprise.db");
        let _db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();

        let error = channel_detail(
            &path,
            &GetChannelDetailParams {
                subscription_id: 999,
                show_shorts: None,
                limit: None,
            },
        )
        .unwrap_err();

        assert!(matches!(error, DataError::InvalidInput(_)));
    }
}
