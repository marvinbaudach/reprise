//! Pure copy projection for the device page's single playlists target.

use reprise_core::device_sync::device_view::CategoryContentRow;
use reprise_core::device_sync::{summarize_playlist_selection, CategoryReading};

use super::device_sync_strings;

pub(super) fn playlist_rule_text(
    playlists: &[reprise_core::device_sync::SyncPlaylistRow],
    unique_track_count: usize,
    keep_smart_updated: bool,
) -> String {
    let summary = summarize_playlist_selection(playlists, unique_track_count);
    let count = device_sync_strings::selected_playlists(summary.selected, summary.available_total);
    let smart = device_sync_strings::text(if keep_smart_updated {
        device_sync_strings::SMART_LISTS_UPDATED
    } else {
        device_sync_strings::SMART_LISTS_FROZEN
    });
    format!("{count} · {smart}")
}

pub(super) fn projected_playlist_size_bytes(
    content: &CategoryContentRow,
    reading: &CategoryReading,
) -> u64 {
    let CategoryReading::Diff(diff) = reading;
    content
        .size_on_device_bytes
        .saturating_add(diff.bytes_to_copy)
        .saturating_sub(diff.bytes_freed)
}

pub(super) fn playlist_result_text(
    content: &CategoryContentRow,
    reading: &CategoryReading,
) -> (String, String) {
    let CategoryReading::Diff(diff) = reading;
    let count = content
        .item_count
        .saturating_add(diff.files_to_copy)
        .saturating_sub(diff.files_to_remove);
    (
        device_sync_strings::music_track_count(count),
        device_sync_strings::file_size(projected_playlist_size_bytes(content, reading)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::device_sync::CategoryDiff;

    fn content() -> CategoryContentRow {
        CategoryContentRow {
            target_path: "/Music/Reprise".into(),
            target_enabled: true,
            item_count: 8,
            size_on_device_bytes: 3 * 1024 * 1024 * 1024,
        }
    }

    #[test]
    fn mtp_51_playlist_rule_has_one_selection_summary_and_no_cap() {
        let text = playlist_rule_text(&[], 0, true);
        assert_eq!(text, "0 of 0 playlists · smart lists kept up to date");
        assert!(!text.contains("size"));
        assert!(!text.contains("limit"));
    }

    #[test]
    fn mtp_22_playlist_result_projects_the_same_diff_into_count_and_size() {
        let reading = CategoryReading::Diff(CategoryDiff {
            files_to_copy: 3,
            bytes_to_copy: 512 * 1024 * 1024,
            files_to_remove: 1,
            bytes_freed: 256 * 1024 * 1024,
            playlists_rewritten: 0,
        });
        assert_eq!(
            playlist_result_text(&content(), &reading),
            ("10 tracks".into(), "3.2 GiB".into())
        );
        assert_eq!(
            projected_playlist_size_bytes(&content(), &reading),
            3_489_660_928
        );
    }
}
