//! Pure projection of persisted and transient episode download states.

use std::collections::BTreeMap;

use reprise_core::podcasts::download_state::{self, DownloadState};
use reprise_core::podcasts::EpisodeRow;

/// `POD-11`: the YouTube channel detail's header summary data — the
/// currently listed window's size plus how many of the channel's episodes
/// are downloaded and their combined size on disk. `downloaded_count`/
/// `downloaded_bytes` are computed over the whole channel's episode set
/// (not the visible window and not filtered by the Shorts toggle), so the
/// total reads as real disk usage rather than a filtered slice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ChannelDownloadSummary {
    pub(super) shown: usize,
    pub(super) available: usize,
    pub(super) downloaded_count: usize,
    pub(super) downloaded_bytes: u64,
}

pub(super) fn channel_download_summary(
    shown: usize,
    available: usize,
    episodes: &[EpisodeRow],
    download_states: &BTreeMap<i64, DownloadState>,
) -> ChannelDownloadSummary {
    let (downloaded_count, downloaded_bytes) = episodes.iter().fold(
        (0_usize, 0_u64),
        |(count, bytes), episode| match download_states.get(&episode.id) {
            Some(DownloadState::Downloaded { bytes: size }) => {
                (count + 1, bytes.saturating_add(*size))
            }
            _ => (count, bytes),
        },
    );
    ChannelDownloadSummary {
        shown,
        available,
        downloaded_count,
        downloaded_bytes,
    }
}

pub(super) fn refreshed_download_states(
    rows: &[EpisodeRow],
    previous: &BTreeMap<i64, DownloadState>,
) -> BTreeMap<i64, DownloadState> {
    rows.iter()
        .map(|row| {
            let state = match previous.get(&row.id) {
                Some(
                    state @ (DownloadState::Queued
                    | DownloadState::Downloading { .. }
                    | DownloadState::Failed { .. }),
                ) => state.clone(),
                _ => {
                    let metadata = row
                        .downloaded_path
                        .as_deref()
                        .and_then(|path| std::fs::metadata(path).ok())
                        .filter(std::fs::Metadata::is_file);
                    let bytes = row.downloaded_bytes.or_else(|| {
                        metadata
                            .as_ref()
                            .map(|metadata| metadata.len().min(i64::MAX as u64) as i64)
                    });
                    download_state::from_persisted(
                        row.downloaded_path.as_deref(),
                        bytes,
                        metadata.is_some(),
                    )
                }
            };
            (row.id, state)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use reprise_core::podcasts::PodcastKind;

    use super::*;

    fn episode(id: i64) -> EpisodeRow {
        EpisodeRow {
            id,
            subscription_id: 7,
            guid: format!("video-{id}"),
            title: format!("Video {id}"),
            show: "Channel".into(),
            show_image_url: None,
            kind: PodcastKind::Youtube,
            audio_url: format!("https://youtube.test/watch?v={id}"),
            page_url: None,
            published_at: Some(id),
            duration_secs: Some(600),
            downloaded_path: None,
            downloaded_bytes: None,
            played_at: None,
            position_ms: 0,
            first_seen_at: id,
        }
    }

    #[test]
    fn pod_11_channel_summary_counts_downloaded_episodes_and_total_bytes() {
        let episodes = vec![episode(1), episode(2), episode(3)];
        let states = BTreeMap::from([
            (1, DownloadState::Downloaded { bytes: 100 }),
            (2, DownloadState::NotDownloaded),
            (3, DownloadState::Downloaded { bytes: 250 }),
        ]);

        let summary = channel_download_summary(10, 487, &episodes, &states);

        assert_eq!(summary.shown, 10);
        assert_eq!(summary.available, 487);
        assert_eq!(summary.downloaded_count, 2);
        assert_eq!(summary.downloaded_bytes, 350);
    }

    #[test]
    fn pod_11_channel_summary_never_counts_queued_downloading_missing_or_failed_as_downloaded() {
        let episodes = vec![episode(1), episode(2), episode(3), episode(4)];
        let states = BTreeMap::from([
            (1, DownloadState::Queued),
            (
                2,
                DownloadState::Downloading {
                    received_bytes: 40,
                    total_bytes: Some(100),
                },
            ),
            (3, DownloadState::Missing),
            (
                4,
                DownloadState::Failed {
                    message: "boom".into(),
                },
            ),
        ]);

        let summary = channel_download_summary(4, 4, &episodes, &states);

        assert_eq!(summary.downloaded_count, 0);
        assert_eq!(summary.downloaded_bytes, 0);
    }

    #[test]
    fn pod_11_channel_summary_reacts_to_a_download_completing_and_being_deleted() {
        let episodes = vec![episode(1)];
        let mut states = BTreeMap::from([(1, DownloadState::NotDownloaded)]);
        assert_eq!(
            channel_download_summary(1, 1, &episodes, &states).downloaded_count,
            0
        );

        states.insert(1, DownloadState::Downloaded { bytes: 500 });
        let after_download = channel_download_summary(1, 1, &episodes, &states);
        assert_eq!(after_download.downloaded_count, 1);
        assert_eq!(after_download.downloaded_bytes, 500);

        states.insert(1, DownloadState::NotDownloaded);
        let after_delete = channel_download_summary(1, 1, &episodes, &states);
        assert_eq!(after_delete.downloaded_count, 0);
        assert_eq!(after_delete.downloaded_bytes, 0);
    }
}
