#![allow(dead_code)]

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::{formatted, plural, text};
pub const INFORMATION: &str = N_!("Information");
/// Tooltip of the headerbar Now Playing panel toggle (TIP-1b).
pub const INFO_PANEL_TOGGLE: &str = N_!("Toggle Now Playing panel");
pub const NOW_PLAYING_NOTHING: &str = N_!("Nothing playing");
pub const ARTIST_NEWS: &str = N_!("Artist & Album News");
pub const ARTIST_NEWS_DESCRIPTION: &str =
    N_!("Show upcoming and newly released albums from MusicBrainz (network; off by default)");
pub const ARTIST_NEWS_PRIVACY: &str =
    N_!("When enabled, selected artist names are sent to MusicBrainz. Reprise never sends file paths or listening history.");
pub const NEWS_DISABLED_TITLE: &str = N_!("Artist News is Off");
pub const NEWS_SELECT_TRACK: &str = N_!("Select a track to see artist and album news.");
pub const NEWS_MULTIPLE_SELECTION: &str =
    N_!("Artist News is paused while multiple tracks are selected.");
pub const NEWS_NO_ARTIST: &str = N_!("This track has no artist information.");
pub const NEWS_LOADING: &str = N_!("Checking MusicBrainz for album news…");
pub const NEWS_NONE: &str = N_!("No new or upcoming regular albums found.");
pub const NEWS_ERROR: &str = N_!("Artist News is temporarily unavailable.");
pub const NEWS_UNMATCHED: &str = N_!("Artist could not be matched.");
pub const NEWS_AMBIGUOUS: &str = N_!("Artist could not be matched unambiguously.");
pub const NEWS_UPCOMING: &str = N_!("Upcoming");
pub const NEWS_NEW: &str = N_!("New");
pub const NEWS_REFRESH: &str = N_!("Refresh Artist News");
pub const NEWS_OPEN_MUSICBRAINZ: &str = N_!("Open in MusicBrainz");

pub fn tracks_selected(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} track selected",
        "{count} tracks selected",
        count,
        &[("count", &count_text)],
    )
}

pub fn news_release_meta(primary_type: &str, date: &str) -> String {
    formatted(
        N_!("{type} · {date}"),
        &[("type", primary_type), ("date", date)],
    )
}

pub fn news_updated(timestamp: i64) -> String {
    let date = news_timestamp_date(timestamp);
    formatted(N_!("MusicBrainz · Updated {date}"), &[("date", &date)])
}

pub fn news_cached(timestamp: i64) -> String {
    let date = news_timestamp_date(timestamp);
    formatted(N_!("Cached · Updated {date}"), &[("date", &date)])
}

fn news_timestamp_date(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0).map_or_else(
        || text(N_!("unknown date")),
        |value| {
            value
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string()
        },
    )
}
pub const COVER_DOWNLOAD_CHECKING: &str = N_!("Checking missing album covers…");
pub const COVER_DOWNLOAD_COMPLETE: &str = N_!("Cover check complete");
pub const COVER_DOWNLOAD_FAILED: &str = N_!("Could not check album covers");

pub fn cover_download_progress(
    checked: usize,
    total: usize,
    downloaded: usize,
    unavailable: usize,
) -> String {
    let checked = checked.to_string();
    let total = total.to_string();
    let downloaded = downloaded.to_string();
    let unavailable = unavailable.to_string();
    formatted(
        N_!("{checked} of {total} checked · {downloaded} downloaded · {unavailable} unavailable"),
        &[
            ("checked", &checked),
            ("total", &total),
            ("downloaded", &downloaded),
            ("unavailable", &unavailable),
        ],
    )
}
