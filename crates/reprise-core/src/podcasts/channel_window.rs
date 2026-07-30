//! Channel-detail windowing, Shorts filtering and download summary
//! (`POD-10`/`POD-11`) — the pure projections behind the YouTube channel
//! detail surface.
//!
//! Moved out of `reprise-gnome` (Block H, MCP parity): the GTK channel
//! detail widget and the `music_get_channel_detail` MCP tool must show the
//! exact same window, Shorts filter and per-row/aggregate download figures,
//! so both call these functions instead of each carrying their own copy.
//! The GTK side keeps its own `YoutubeChannelState` for per-channel session
//! state (which limit/override is active for which subscription) but
//! delegates every actual computation here.

use std::collections::BTreeMap;

use super::download_state::DownloadState;
use super::EpisodeRow;

/// `POD-10`: a freshly opened channel starts with its ten most recent
/// long-form entries.
pub const INITIAL_WINDOW: usize = 10;
/// `POD-10`: "Load more" extends the window to this many entries in one step.
pub const EXTENDED_WINDOW: usize = 40;
/// `POD-10`: an entry at or under this duration counts as a YouTube Short.
pub const SHORT_MAX_SECONDS: i64 = 180;

/// `POD-10`: whether `episode` is a Short by duration. YouTube RSS/yt-dlp
/// entries without a known duration are never classified as Shorts — an
/// unknown length must not silently hide an episode.
#[must_use]
pub fn is_short(episode: &EpisodeRow) -> bool {
    episode
        .duration_secs
        .is_some_and(|seconds| (0..=SHORT_MAX_SECONDS).contains(&seconds))
}

/// `POD-10`: the currently visible window — episodes already come newest
/// first (`POD-9`), so this only applies the Shorts filter and the take
/// limit.
#[must_use]
pub fn visible_window(
    episodes: &[EpisodeRow],
    show_shorts: bool,
    limit: usize,
) -> Vec<&EpisodeRow> {
    episodes
        .iter()
        .filter(|episode| show_shorts || !is_short(episode))
        .take(limit)
        .collect()
}

/// `POD-10`: how many episodes are available under the current Shorts
/// filter, independent of the window limit — the header's "N of M" and
/// whether "Load more" has anything left to reveal both read this.
#[must_use]
pub fn available_count(episodes: &[EpisodeRow], show_shorts: bool) -> usize {
    episodes
        .iter()
        .filter(|episode| show_shorts || !is_short(episode))
        .count()
}

/// `POD-14`: true exactly when the channel's visible window is empty *only*
/// because Shorts are hidden — the channel has entries, but every one of
/// them is a Short. Pure and displayless so the GTK detail view and a future
/// MCP caller decide it the same way, matching POD-10/POD-11's split between
/// core decision and display. Does not fire when the channel truly has no
/// entries at all (that is a different, uncovered gap — see POD-14's doc
/// comment in `docs/ux-rules.md`) or when Shorts are already shown.
#[must_use]
pub fn shorts_only_hidden(episodes: &[EpisodeRow], show_shorts: bool) -> bool {
    !show_shorts && !episodes.is_empty() && available_count(episodes, false) == 0
}

/// `POD-11`: the channel detail header summary — the currently listed
/// window's size plus how many of the channel's episodes are downloaded and
/// their combined size on disk. `downloaded_count`/`downloaded_bytes` are
/// computed over the whole channel's episode set (not the visible window
/// and not filtered by the Shorts toggle), so the total reads as real disk
/// usage rather than a filtered slice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChannelDownloadSummary {
    pub shown: usize,
    pub available: usize,
    pub downloaded_count: usize,
    pub downloaded_bytes: u64,
}

#[must_use]
pub fn channel_download_summary(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::podcasts::PodcastKind;

    fn episode(id: i64, duration_secs: Option<i64>) -> EpisodeRow {
        EpisodeRow {
            id,
            subscription_id: 7,
            guid: format!("video-{id}"),
            title: format!("Video {id}"),
            show: "Channel".into(),
            show_image_url: None,
            image_url: None,
            kind: PodcastKind::Youtube,
            audio_url: format!("https://youtube.test/watch?v={id}"),
            page_url: None,
            published_at: Some(id),
            duration_secs,
            downloaded_path: None,
            downloaded_bytes: None,
            played_at: None,
            position_ms: 0,
            first_seen_at: id,
            is_new: false,
        }
    }

    #[test]
    fn pod_10_visible_window_hides_shorts_by_default_and_reveals_them_on_request() {
        let episodes = vec![episode(2, Some(600)), episode(1, Some(60))];

        assert_eq!(
            visible_window(&episodes, false, INITIAL_WINDOW).len(),
            1,
            "the Short must be filtered out by default"
        );
        assert_eq!(
            visible_window(&episodes, true, INITIAL_WINDOW).len(),
            2,
            "showing Shorts must change the result"
        );
    }

    #[test]
    fn pod_10_visible_window_respects_the_take_limit_independent_of_available_count() {
        let episodes = (1..=25)
            .rev()
            .map(|id| episode(id, Some(600)))
            .collect::<Vec<_>>();

        assert_eq!(visible_window(&episodes, true, INITIAL_WINDOW).len(), 10);
        assert_eq!(visible_window(&episodes, true, EXTENDED_WINDOW).len(), 25);
        assert_eq!(available_count(&episodes, true), 25);
    }

    #[test]
    fn pod_10_available_count_reacts_to_the_shorts_filter_independent_of_the_window() {
        let episodes = vec![episode(2, Some(600)), episode(1, Some(60))];

        assert_eq!(available_count(&episodes, false), 1);
        assert_eq!(available_count(&episodes, true), 2);
    }

    #[test]
    fn pod_10_an_unknown_duration_is_never_classified_as_a_short() {
        assert!(!is_short(&episode(1, None)));
    }

    #[test]
    fn pod_14_shorts_only_hidden_fires_precisely_when_every_entry_is_a_hidden_short() {
        let all_shorts = vec![episode(2, Some(60)), episode(1, Some(30))];
        let mixed = vec![episode(2, Some(600)), episode(1, Some(60))];

        // Would go red if the feature were deleted (returning `false`
        // unconditionally): a channel of nothing but Shorts, hidden, must
        // read as shorts-only.
        assert!(shorts_only_hidden(&all_shorts, false));
        // A long-form entry survives the filter, so this is an ordinary
        // (non-empty) window, not the shorts-only case.
        assert!(!shorts_only_hidden(&mixed, false));
        // Revealing Shorts empties the condition even for an all-Shorts
        // channel — there is no longer anything hidden.
        assert!(!shorts_only_hidden(&all_shorts, true));
        // A channel with no entries at all is a different, uncovered gap —
        // not shorts-only.
        assert!(!shorts_only_hidden(&[], false));
    }

    #[test]
    fn pod_11_channel_summary_counts_only_downloaded_episodes_and_their_bytes() {
        let episodes = (1..=3).map(episode_with_600).collect::<Vec<_>>();
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
        let episodes = (1..=4).map(episode_with_600).collect::<Vec<_>>();
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

    fn episode_with_600(id: i64) -> EpisodeRow {
        episode(id, Some(600))
    }
}
