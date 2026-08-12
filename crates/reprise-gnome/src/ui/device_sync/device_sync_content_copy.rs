//! Pure copy projection for the device page's single playlists target.

use reprise_core::device_sync::device_view::CategoryContentRow;
use reprise_core::device_sync::MusicReading;

use super::device_sync_strings;

pub(super) fn projected_playlist_size_bytes(
    content: &CategoryContentRow,
    reading: &MusicReading,
) -> u64 {
    let MusicReading::Diff(diff) = reading;
    content
        .size_on_device_bytes
        .saturating_add(diff.bytes_to_copy)
        .saturating_sub(diff.bytes_freed)
}

pub(super) fn playlist_result_text(
    content: &CategoryContentRow,
    reading: &MusicReading,
) -> (String, String) {
    let MusicReading::Diff(diff) = reading;
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
    use reprise_core::device_sync::MusicDiff;

    fn content() -> CategoryContentRow {
        CategoryContentRow {
            target_path: "/Music/Reprise".into(),
            target_enabled: true,
            item_count: 8,
            size_on_device_bytes: 3 * 1024 * 1024 * 1024,
        }
    }

    #[test]
    fn mtp_22_playlist_result_projects_the_same_diff_into_count_and_size() {
        let reading = MusicReading::Diff(MusicDiff {
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
