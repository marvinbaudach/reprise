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
pub const RELEASES_IN_LIBRARY: &str = N_!("In library");
pub const RELEASES_UPCOMING: &str = N_!("upcoming");
pub const RELEASES_INCOMPLETE: &str = N_!("Incomplete");
pub const RELEASES_MISSING: &str = N_!("Missing");
pub const RELEASES_NO_DATA_TITLE: &str = N_!("No discography data yet");
pub const RELEASES_EMPTY_TITLE: &str = N_!("No missing albums or EPs");
pub const RELEASES_HIDE: &str = N_!("Hide");

pub fn release_count_line(shown: usize, total: usize) -> String {
    formatted(
        N_!("{shown} of {total} gaps"),
        &[("shown", &shown.to_string()), ("total", &total.to_string())],
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
    fn nr_17_release_counts_name_discography_gaps() {
        assert_eq!(release_count_line(8, 19), "8 of 19 gaps");
        assert_eq!(release_total_line(19), "19 gaps");
    }
}
