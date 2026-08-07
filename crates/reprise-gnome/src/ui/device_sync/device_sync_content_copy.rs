//! Pure copy projection for the device page's one-row-per-source plan.

use reprise_core::device_sync::device_view::CategoryContentRow;
use reprise_core::device_sync::{
    summarize_playlist_selection, CategoryReading, PodcastSelectionSummary, SyncTargetKind,
    YoutubeSelectionSummary,
};

use super::device_sync_strings;

#[cfg(test)]
pub(super) fn category_rule_text(
    kind: SyncTargetKind,
    playlists: &[reprise_core::device_sync::SyncPlaylistRow],
    unique_track_count: usize,
    youtube: YoutubeSelectionSummary,
    podcasts: PodcastSelectionSummary,
    keep_smart_updated: bool,
    cap_bytes: Option<u64>,
) -> String {
    let cap = device_sync_strings::cap_text(cap_bytes);
    format!(
        "{} · {cap}",
        category_rule_prefix(
            kind,
            playlists,
            unique_track_count,
            youtube,
            podcasts,
            keep_smart_updated,
        )
    )
}

pub(super) fn category_rule_prefix(
    kind: SyncTargetKind,
    playlists: &[reprise_core::device_sync::SyncPlaylistRow],
    unique_track_count: usize,
    youtube: YoutubeSelectionSummary,
    podcasts: PodcastSelectionSummary,
    keep_smart_updated: bool,
) -> String {
    let parts = match kind {
        SyncTargetKind::Playlists => {
            let summary = summarize_playlist_selection(playlists, unique_track_count);
            let count =
                device_sync_strings::selected_playlists(summary.selected, summary.available_total);
            let smart = device_sync_strings::text(if keep_smart_updated {
                device_sync_strings::SMART_LISTS_UPDATED
            } else {
                device_sync_strings::SMART_LISTS_FROZEN
            });
            vec![count, smart]
        }
        SyncTargetKind::YoutubeAudio => {
            let count = device_sync_strings::selected_channels(youtube.channels_selected);
            let policy = if matches!(youtube.latest_per_channel, 0 | usize::MAX) {
                device_sync_strings::text(device_sync_strings::ALL_EPISODES)
            } else {
                device_sync_strings::latest_each(youtube.latest_per_channel)
            };
            vec![count, policy]
        }
        SyncTargetKind::PodcastEpisodes => vec![
            device_sync_strings::selected_shows(podcasts.shows_selected, podcasts.shows_total),
            device_sync_strings::text(device_sync_strings::UNPLAYED_ONLY),
            device_sync_strings::text(device_sync_strings::PLAYED_ARE_REMOVED),
        ],
    };
    parts.join(" · ")
}

/// The category's projected result after the next synchronization. Counts
/// and bytes are derived from the same `CategoryReading` that used to own a
/// separate row, so combining the cards does not create a second diff.
pub(super) fn category_result_text(
    kind: SyncTargetKind,
    content: &CategoryContentRow,
    reading: &CategoryReading,
) -> (String, String) {
    let CategoryReading::Diff(diff) = reading else {
        return (
            device_sync_strings::category_reading_text(reading),
            device_sync_strings::file_size(content.size_on_device_bytes),
        );
    };

    let count = content
        .item_count
        .saturating_add(diff.files_to_copy)
        .saturating_sub(diff.files_to_remove);
    let size = content
        .size_on_device_bytes
        .saturating_add(diff.bytes_to_copy)
        .saturating_sub(diff.bytes_freed);
    let title = device_sync_strings::category_item_count(kind, count);
    let mut detail = device_sync_strings::file_size(size);
    if kind == SyncTargetKind::YoutubeAudio && diff.files_waiting_for_download > 0 {
        detail.push_str(" · ");
        detail.push_str(&device_sync_strings::to_download(
            diff.files_waiting_for_download,
        ));
    }
    (title, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::device_sync::CategoryDiff;

    #[test]
    fn category_rule_sentence_combines_selection_policy_and_cap() {
        assert_eq!(
            category_rule_text(
                SyncTargetKind::Playlists,
                &[],
                0,
                YoutubeSelectionSummary::default(),
                PodcastSelectionSummary::default(),
                true,
                None,
            ),
            "0 of 0 playlists · smart lists kept up to date · no size limit"
        );
        assert_eq!(
            category_rule_text(
                SyncTargetKind::YoutubeAudio,
                &[],
                0,
                YoutubeSelectionSummary {
                    channels_selected: 2,
                    channels_total: 4,
                    latest_per_channel: 10,
                },
                PodcastSelectionSummary::default(),
                true,
                Some(8 * 1024 * 1024 * 1024),
            ),
            "2 channels · latest 10 each · max 8.0 GiB"
        );
        assert_eq!(
            category_rule_text(
                SyncTargetKind::PodcastEpisodes,
                &[],
                0,
                YoutubeSelectionSummary::default(),
                PodcastSelectionSummary {
                    shows_selected: 1,
                    shows_total: 3,
                },
                true,
                Some(4 * 1024 * 1024 * 1024),
            ),
            "1 of 3 shows · unplayed only · played are removed · max 4.0 GiB"
        );
    }

    #[test]
    fn unlimited_youtube_rule_never_reads_latest_zero() {
        let text = category_rule_text(
            SyncTargetKind::YoutubeAudio,
            &[],
            0,
            YoutubeSelectionSummary {
                channels_selected: 4,
                channels_total: 4,
                latest_per_channel: 0,
            },
            PodcastSelectionSummary::default(),
            true,
            None,
        );

        assert_eq!(text, "4 channels · all episodes · no size limit");
        assert!(!text.contains("latest 0"));
    }

    #[test]
    fn result_projects_the_same_diff_into_count_size_and_download_copy() {
        let content = CategoryContentRow {
            kind: SyncTargetKind::YoutubeAudio,
            target_path: "/Music/Reprise-YouTube".into(),
            target_enabled: true,
            item_count: 8,
            size_on_device_bytes: 3 * 1024 * 1024 * 1024,
            cap_bytes: Some(8 * 1024 * 1024 * 1024),
        };
        let reading = CategoryReading::Diff(CategoryDiff {
            files_to_copy: 3,
            bytes_to_copy: 512 * 1024 * 1024,
            files_to_remove: 1,
            bytes_freed: 256 * 1024 * 1024,
            files_waiting_for_download: 4,
            playlists_rewritten: 0,
        });

        assert_eq!(
            category_result_text(SyncTargetKind::YoutubeAudio, &content, &reading),
            ("10 episodes".into(), "3.2 GiB · 4 to download".into())
        );
    }
}
