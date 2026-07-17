macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::plural;
use super::{formatted, text};

pub const ISSUE_SHOW_ONE_MORE: &str = N_!("Show 1 more");
pub const ISSUE_SHOW_MORE: &str = N_!("Show {count} more");

pub fn issue_show_more(count: u32) -> String {
    let count_text = count.to_string();
    let count = usize::try_from(count).unwrap_or(usize::MAX);
    plural(
        ISSUE_SHOW_ONE_MORE,
        ISSUE_SHOW_MORE,
        count,
        &[("count", &count_text)],
    )
}

pub const MISSING_CLEAR_TITLE: &str = N_!("No missing files ✓");
pub const MISSING_CLEAR_DESCRIPTION: &str = N_!("Your library has no missing file entries.");
pub const MISSING_UNAVAILABLE_ICON: &str = N_!("⏏");
pub const MISSING_UNAVAILABLE_TITLE: &str = N_!("On unavailable drive");
pub const MISSING_UNKNOWN_LOCATION: &str = N_!("unknown location");
pub const MISSING_NOT_MOUNTED: &str = N_!("not mounted");
pub const MISSING_RETURNS_WHEN_MOUNTED: &str =
    N_!("return automatically when the drive is mounted");
pub const MISSING_VERIFY_NEXT_SCAN: &str = N_!("will be verified on next scan");
pub const MISSING_DELETED_ICON: &str = N_!("🗑");
pub const MISSING_DELETED_TITLE: &str = N_!("Deleted from disk");
pub const MISSING_DELETED_META: &str = N_!("folder still exists");
pub const MISSING_REMOVE: &str = N_!("Remove");
pub const MISSING_REMOVE_ALL: &str = N_!("Remove all {count} from library");
pub const MISSING_REMOVE_HEADING: &str = N_!("Remove from library?");
pub const MISSING_REMOVE_BODY: &str = N_!("This removes {count} tracks from the library — their ratings and listening history go with them. Files are never touched.");
pub const MISSING_REMOVED_ONE: &str = N_!("1 removed");
pub const MISSING_REMOVED: &str = N_!("{count} removed");
pub const MISSING_UNDO: &str = N_!("Undo");
pub const MISSING_AUTO_CLEAN_OFF: &str = N_!("Auto-clean: off ▾");
pub const MISSING_AUTO_CLEAN_DAYS: &str = N_!("Auto-clean: {days} days ▾");
pub const MISSING_AUTO_CLEAN_OPTION_OFF: &str = N_!("Off");
pub const MISSING_AUTO_CLEAN_OPTION_30: &str = N_!("30 days");
pub const MISSING_AUTO_CLEAN_OPTION_90: &str = N_!("90 days");
pub const MISSING_AUTO_CLEAN_HEADING: &str = N_!("Enable auto-clean?");
pub const MISSING_AUTO_CLEAN_BODY: &str = N_!("This will remove {count} tracks now (deleted more than {days} days ago) — their ratings and listening history go with them.");
pub const MISSING_AUTO_CLEAN_REMOVE_NOW: &str = N_!("Remove now");
pub const MISSING_AUTO_CLEAN_START_TODAY: &str = N_!("Start counting from today");
pub const MISSING_RELINK_INFO: &str =
    N_!("Reprise automatically reconnects moved tracks when it can identify them.");
pub const MISSING_LAST_RELINKED: &str = N_!("Last scan relinked {count} tracks");
pub const MISSING_AUTO_CLEAN_HINT: &str = N_!("Tracks deleted from disk stay listed until you remove them — enable auto-clean to do this automatically.");
pub const MISSING_FOOTNOTE: &str = N_!("Remove only removes library entries — never files.");
pub const MISSING_TRACKS_ONE: &str = N_!("1 track");
pub const MISSING_TRACKS: &str = N_!("{count} tracks");
pub const MISSING_SINCE: &str = N_!("since {date}");
pub const MISSING_ROW_UNAVAILABLE: &str = N_!("On unavailable drive — returns when mounted");
pub const MISSING_ROW_FILE_SINCE: &str = N_!("File missing since {date}");

pub fn missing_tracks(count: u32) -> String {
    let value = count.to_string();
    plural(
        MISSING_TRACKS_ONE,
        MISSING_TRACKS,
        count as usize,
        &[("count", &value)],
    )
}

pub fn missing_remove_all(count: usize) -> String {
    formatted(MISSING_REMOVE_ALL, &[("count", &count.to_string())])
}

pub fn missing_remove_body(count: usize) -> String {
    formatted(MISSING_REMOVE_BODY, &[("count", &count.to_string())])
}

pub fn missing_removed(count: usize) -> String {
    let value = count.to_string();
    plural(
        MISSING_REMOVED_ONE,
        MISSING_REMOVED,
        count,
        &[("count", &value)],
    )
}

pub fn missing_auto_clean_label(days: u32) -> String {
    formatted(MISSING_AUTO_CLEAN_DAYS, &[("days", &days.to_string())])
}

pub fn missing_auto_clean_body(count: usize, days: u32) -> String {
    formatted(
        MISSING_AUTO_CLEAN_BODY,
        &[("count", &count.to_string()), ("days", &days.to_string())],
    )
}

pub fn missing_last_relinked(count: &str) -> String {
    formatted(MISSING_LAST_RELINKED, &[("count", count)])
}

pub fn missing_since(date: &str) -> String {
    formatted(MISSING_SINCE, &[("date", date)])
}

pub fn missing_row_file_since(date: &str) -> String {
    formatted(MISSING_ROW_FILE_SINCE, &[("date", date)])
}

pub fn issue_text(message: &str) -> String {
    text(message)
}

pub const IMPORT_ISSUE_TAGS_ICON: &str = N_!("✎");
pub const IMPORT_ISSUE_TAGS_TITLE: &str = N_!("Unreadable tags");
pub const IMPORT_ISSUE_TAGS_ROW: &str =
    N_!("Tags unreadable — the file itself can usually still be played");
pub const IMPORT_ISSUE_PERMISSION_ICON: &str = N_!("🔒");
pub const IMPORT_ISSUE_PERMISSION_TITLE: &str = N_!("Permission denied");
pub const IMPORT_ISSUE_PERMISSION_ROW: &str = N_!("Reprise cannot read this file");
pub const IMPORT_ISSUE_FORMAT_ICON: &str = N_!("◇");
pub const IMPORT_ISSUE_FORMAT_TITLE: &str = N_!("Unsupported format");
pub const IMPORT_ISSUE_FORMAT_ROW: &str = N_!("This audio format is not supported");
pub const IMPORT_ISSUE_IO_ICON: &str = N_!("⚠");
pub const IMPORT_ISSUE_IO_TITLE: &str = N_!("Read error");
pub const IMPORT_ISSUE_IO_ROW: &str = N_!("The file could not be read");
pub const IMPORT_ISSUE_UNKNOWN_ICON: &str = N_!("?");
pub const IMPORT_ISSUE_UNKNOWN_TITLE: &str = N_!("Unclassified");
pub const IMPORT_ISSUE_UNKNOWN_ROW: &str = N_!("The error could not be classified");
pub const IMPORT_ISSUE_FILE_ONE: &str = N_!("1 file");
pub const IMPORT_ISSUE_FILES: &str = N_!("{count} files");
pub const IMPORT_ISSUE_SEEN_ONE: &str = N_!("seen in 1 scan");
pub const IMPORT_ISSUE_SEEN: &str = N_!("seen in {count} scans");
pub const IMPORT_ISSUE_HINT_PREFIX: &str = N_!("Imported without metadata");
pub const IMPORT_ISSUE_EDIT_TAGS: &str = N_!("Open in Tag Editor");
pub const IMPORT_ISSUE_SHOW_FILES: &str = N_!("Show in Files");
pub const IMPORT_ISSUE_RETRY_ALL: &str = N_!("Retry all");
pub const IMPORT_ISSUE_DISMISS_ALL: &str = N_!("Dismiss all");
pub const IMPORT_ISSUE_EXPORT: &str = N_!("Export list…");
pub const IMPORT_ISSUE_EXPORT_TITLE: &str = N_!("Export import errors");
pub const IMPORT_ISSUE_RESTORE: &str = N_!("Restore");
pub const IMPORT_ISSUE_DISMISSED: &str = N_!("{count} dismissed · Show");
pub const IMPORT_ISSUE_HIDE_DISMISSED: &str = N_!("Hide dismissed");
pub const IMPORT_ISSUE_DISMISS_FAILED: &str = N_!("Could not dismiss — the file is unavailable");
pub const IMPORT_ISSUE_RETRY_ALL_FAILED: &str = N_!("Could not retry all import errors");
pub const IMPORT_ISSUE_EXPORT_FAILED: &str = N_!("Could not export the import-error list");
pub const IMPORT_ISSUE_FAILED_ONE: &str = N_!("1 failed");
pub const IMPORT_ISSUE_FAILED: &str = N_!("{count} failed");
pub const IMPORT_ISSUE_DETAILS: &str = N_!("Details");

pub fn import_issue_file_count(count: usize) -> String {
    let value = count.to_string();
    plural(
        IMPORT_ISSUE_FILE_ONE,
        IMPORT_ISSUE_FILES,
        count,
        &[("count", &value)],
    )
}

pub fn import_issue_seen(count: i64) -> String {
    let count = usize::try_from(count.max(0)).unwrap_or(usize::MAX);
    let value = count.to_string();
    plural(
        IMPORT_ISSUE_SEEN_ONE,
        IMPORT_ISSUE_SEEN,
        count,
        &[("count", &value)],
    )
}

pub fn import_issue_dismissed(count: u32) -> String {
    formatted(IMPORT_ISSUE_DISMISSED, &[("count", &count.to_string())])
}

pub fn import_issue_failed(count: u32) -> String {
    let value = count.to_string();
    plural(
        IMPORT_ISSUE_FAILED_ONE,
        IMPORT_ISSUE_FAILED,
        count as usize,
        &[("count", &value)],
    )
}
