#![allow(dead_code)]

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::{formatted, plural};

pub const RADIO: &str = N_!("Radio");
pub const RADIO_DESCRIPTION: &str =
    N_!("Contacts radio-browser.info for search; each favorite play reports its etiquette click");
pub const RADIO_STATION: &str = N_!("Station");
pub const RADIO_GENRE: &str = N_!("Genre");
pub const RADIO_BITRATE: &str = N_!("Bitrate");
pub const RADIO_COUNTRY: &str = N_!("Country");
pub const RADIO_NOW_PLAYING: &str = N_!("Now playing");
pub const RADIO_ADD: &str = N_!("Add station");
pub const RADIO_ADD_FILTER: &str = N_!("Add filter");
pub const RADIO_FILTER_GENRE: &str = N_!("Genre");
pub const RADIO_FILTER_COUNTRY: &str = N_!("Country");
pub const RADIO_CLEAR_ALL: &str = N_!("Clear all");
/// `SRC-10`: the shared empty-state grammar's copy for Radio — title, one
/// paragraph of what lands here and where it comes from, the primary
/// button. Radio has no secondary line: the body already names the URL
/// path (a stream URL), so a second line repeating it would be redundant.
pub const RADIO_NO_STATIONS: &str = N_!("No stations yet");
pub const RADIO_NO_STATIONS_DESCRIPTION: &str = N_!(
    "Find stations in the open radio-browser directory, or paste a stream URL. Nothing is fetched until you search."
);
pub const RADIO_DIALOG_TITLE: &str = N_!("Add Station");
pub const RADIO_DIALOG_HINT: &str = N_!("Search or paste a stream / M3U / PLS URL");
pub const RADIO_SEARCHING: &str = N_!("Searching…");
pub const RADIO_RESULTS_HEADER: &str = N_!("RADIO-BROWSER.INFO");
pub const RADIO_MATCHES_BY_VOTES: &str = N_!("matches · by votes");
pub const RADIO_ADD_RESULT: &str = N_!("Add");
pub const RADIO_CANCEL: &str = N_!("Cancel");
pub const RADIO_FETCH_METADATA: &str = N_!("Fetch logo & tags from radio-browser");
pub const RADIO_COMMUNITY_FOOTNOTE: &str =
    N_!("Community database — a play sends the etiquette click count to radio-browser.");
pub const RADIO_STREAM_DETECTED: &str = N_!("Radio stream detected");
pub const RADIO_PLAYLIST_DETECTED: &str = N_!("Playlist file detected");
pub const RADIO_PREVIEW_FAILED: &str = N_!("Could not preview this station");
pub const RADIO_SEARCH_FAILED: &str = N_!("Could not search for stations");
pub const RADIO_ADD_FAILED: &str = N_!("Could not add this station");
pub const RADIO_ALREADY_FAVORITE: &str = N_!("This station is already in Radio");
pub const RADIO_PLAY: &str = N_!("Play");
pub const RADIO_STOP: &str = N_!("Stop");
/// `NET-3b`: Radio's offline exception — a live stream cannot be queued, so
/// the Play entry itself becomes the retry affordance while offline.
pub const RADIO_NO_CONNECTION_RETRY: &str = N_!("No connection · Retry");
pub const RADIO_COPY_URL: &str = N_!("Copy stream URL");
pub const RADIO_EDIT: &str = N_!("Edit station…");
pub const RADIO_REMOVE_FAVORITE: &str = N_!("Remove favorite");
pub const RADIO_UNDO: &str = N_!("Undo");
pub const RADIO_RETRY: &str = N_!("Retry");
pub const RADIO_RECONNECTING: &str = N_!("Reconnecting live…");
pub const RADIO_RECONNECT_FAILED: &str = N_!("Could not reconnect to this station");
pub const RADIO_SEARCH_ORDER: &str = N_!("Search order");
pub const RADIO_ORDER_VOTES: &str = N_!("Votes");
pub const RADIO_ORDER_NAME: &str = N_!("Name");
pub const RADIO_ORDER_CLICKS: &str = N_!("Clicks");
pub const RADIO_UNKNOWN_NOW_PLAYING: &str = N_!("—");
pub const RADIO_REPORT_PLAYS: &str = N_!("Report plays to the directory");
/// `RAD-5`: the three one-click radio-browser searches in the Add Station
/// dialog.
pub const RADIO_CHIP_METAL_DE: &str = N_!("Metal in DE");
pub const RADIO_CHIP_TOP_VOTED: &str = N_!("Top voted");
pub const RADIO_CHIP_NEAR_YOU: &str = N_!("Near you");

pub fn radio_station_count(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} station",
        "{count} stations",
        count,
        &[("count", &count_text)],
    )
}

pub fn radio_filtered_count(visible: usize, total: usize) -> String {
    formatted(
        N_!("{visible} of {total} stations"),
        &[
            ("visible", &visible.to_string()),
            ("total", &total.to_string()),
        ],
    )
}

pub fn radio_results_count(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} match · by votes",
        "{count} matches · by votes",
        count,
        &[("count", &count_text)],
    )
}

pub fn radio_remove_named(name: &str) -> String {
    formatted(N_!("Remove “{name}”"), &[("name", name)])
}

pub fn radio_playlist_detected(kind: &str, host: &str) -> String {
    formatted(
        N_!("Playlist file detected ({kind}) — resolved to {host}"),
        &[("kind", kind), ("host", host)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn station_count_uses_singular_and_plural_copy() {
        assert_eq!(radio_station_count(1), "1 station");
        assert_eq!(radio_station_count(12), "12 stations");
    }

    #[test]
    fn result_count_describes_vote_order() {
        assert_eq!(radio_results_count(1), "1 match · by votes");
        assert_eq!(radio_results_count(8), "8 matches · by votes");
    }
}
