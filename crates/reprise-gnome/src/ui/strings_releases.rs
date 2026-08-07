#![allow(dead_code)]

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::formatted;

pub const RELEASES: &str = N_!("Releases");
pub const RELEASES_DATE: &str = N_!("Date");
pub const RELEASES_TITLE: &str = N_!("Title");
pub const RELEASES_ARTIST: &str = N_!("Artist");
pub const RELEASES_TYPE: &str = N_!("Type");
pub const RELEASES_STATUS: &str = N_!("Status");
pub const RELEASES_BUY: &str = N_!("Buy");
pub const RELEASES_BANDCAMP: &str = N_!("Bandcamp");
pub const RELEASES_BUY_ON_BANDCAMP: &str = N_!("Buy on Bandcamp");
pub const RELEASES_ADD_FILTER: &str = N_!("+ Add filter");
pub const RELEASES_FILTER: &str = N_!("FILTER");
pub const RELEASES_CLEAR_ALL: &str = N_!("Clear all ×");
pub const RELEASES_HIDDEN: &str = N_!("Hidden");
pub const RELEASES_ALBUM: &str = N_!("Album");
pub const RELEASES_EP: &str = N_!("EP");
pub const RELEASES_SINGLE: &str = N_!("Single");
pub const RELEASES_WINDOW_ONE_YEAR: &str = N_!("1 year");
pub const RELEASES_WINDOW_FIVE_YEARS: &str = N_!("5 years");
pub const RELEASES_WINDOW_TEN_YEARS: &str = N_!("10 years");
pub const RELEASES_WINDOW_ALL: &str = N_!("All");
pub const RELEASES_IN_LIBRARY: &str = N_!("In library");
pub const RELEASES_UPCOMING: &str = N_!("upcoming");
pub const RELEASES_INCOMPLETE: &str = N_!("Incomplete");
pub const RELEASES_MISSING: &str = N_!("Missing");
pub const RELEASES_NO_DATA_TITLE: &str = N_!("No discography data yet");
pub const RELEASES_EMPTY_TITLE: &str = N_!("No missing releases");
pub const RELEASES_HIDE: &str = N_!("Hide");
pub const RELEASES_COULD_NOT_REFRESH: &str = N_!("Couldn't refresh new releases");
pub const RELEASES_CACHED_FAILURE_DESCRIPTION: &str =
    N_!("Showing saved releases from {time}. Announcement links need a connection.");
pub const RELEASES_EMPTY_FAILURE_DESCRIPTION: &str =
    N_!("There are no saved releases to show. Your library is unaffected.");
pub const RELEASES_SAVED_CACHE_TIME: &str = N_!("an earlier update");
pub const RELEASES_FETCHING: &str = N_!("Fetching releases…");
pub const RELEASES_FETCH_PREPARING: &str = N_!("Preparing release check…");

pub fn release_count_line(shown: usize, total: usize) -> String {
    gap_count_line(&shown.to_string(), total)
}

/// FIL-2: the same line with the shown number accented. The bold goes in as
/// the *argument*, not as a substring search over the rendered sentence — a
/// translation that puts the total first would otherwise bold the wrong
/// number, silently.
pub fn release_count_line_markup(shown: usize, total: usize) -> String {
    gap_count_line(&format!("<b>{shown}</b>"), total)
}

fn gap_count_line(shown: &str, total: usize) -> String {
    formatted(
        N_!("{shown} of {total} gaps"),
        &[("shown", shown), ("total", &total.to_string())],
    )
}

pub fn release_total_line(total: usize) -> String {
    formatted(N_!("{total} gaps"), &[("total", &total.to_string())])
}

pub fn show_all_releases(total: usize) -> String {
    formatted(
        N_!("Show all {total} gaps"),
        &[("total", &total.to_string())],
    )
}

pub fn release_track_count_line(local: i64, total: i64) -> String {
    formatted(
        N_!("{local} of {total} tracks"),
        &[("local", &local.to_string()), ("total", &total.to_string())],
    )
}

pub fn releases_cached_failure_description(time: &str) -> String {
    formatted(RELEASES_CACHED_FAILURE_DESCRIPTION, &[("time", time)])
}

pub fn releases_fetch_progress(checked: usize, total: usize) -> String {
    formatted(
        N_!("Checked {checked} of {total} artists"),
        &[
            ("checked", &checked.to_string()),
            ("total", &total.to_string()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_count_line_formats_shown_and_total() {
        assert_eq!(release_count_line(8, 19), "8 of 19 gaps");
    }

    #[test]
    fn show_all_releases_formats_count() {
        assert_eq!(show_all_releases(19), "Show all 19 gaps");
    }

    #[test]
    fn nr_25_release_counts_name_discography_gaps() {
        assert_eq!(release_count_line(8, 19), "8 of 19 gaps");
        assert_eq!(release_total_line(19), "19 gaps");
    }

    #[test]
    fn release_fetch_progress_formats_both_counts() {
        assert_eq!(releases_fetch_progress(2, 5), "Checked 2 of 5 artists");
    }
}
