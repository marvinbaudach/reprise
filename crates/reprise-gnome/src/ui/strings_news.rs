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
pub const UP_NEXT: &str = N_!("Up Next");
pub const QUEUE_EMPTY: &str = N_!("Queue is empty");
pub const QUEUE_NEXT_IN_QUEUE: &str = N_!("Next in Queue");
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
pub const NEW_RELEASES: &str = N_!("New Releases");
pub const NEW_RELEASES_DESCRIPTION: &str =
    N_!("Show upcoming and newly released albums · contacts MusicBrainz");
pub const COVER_DOWNLOAD: &str = N_!("Album Covers");
pub const COVER_DOWNLOAD_DESCRIPTION: &str =
    N_!("Download missing album covers · contacts MusicBrainz and coverartarchive.org");
pub const ARTIST_PORTRAITS: &str = N_!("Artist Portraits");
pub const ARTIST_PORTRAITS_DESCRIPTION: &str = N_!("Show artist images · contacts Deezer");
pub const ONLINE_LYRICS: &str = N_!("Online Lyrics");
pub const ONLINE_LYRICS_DESCRIPTION: &str = N_!("Load missing lyrics · contacts LRCLIB");
pub const ENABLE_ALBUM_COVERS: &str = N_!("Enable album cover downloads →");
pub const ENABLE_ARTIST_PORTRAITS: &str = N_!("Enable artist images →");
pub const ENABLE_NEW_RELEASES: &str = N_!("Enable new releases →");
pub const ENABLE_ARTIST_NETWORK_FEATURES: &str =
    N_!("Enable network features for artists (images & new releases) →");
pub const DISMISS: &str = N_!("Dismiss");
pub const NEW_RELEASES_ARTISTS: &str = N_!("Artists");
pub const TOP_ARTISTS_ONLY: &str = N_!("Top artists only");
pub const ALL_ARTISTS: &str = N_!("All artists");
pub const FETCH_NOW: &str = N_!("Fetch now");
pub const FETCH_FAILED_INLINE: &str = N_!("Refresh failed · showing saved releases");
pub const UPDATED_JUST_NOW: &str = N_!("Updated just now");
pub const SEE_ALL_RELEASES: &str = N_!("See all");
pub const HIDE_RELEASE: &str = N_!("Hide");

pub fn tracks_selected(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} track selected",
        "{count} tracks selected",
        count,
        &[("count", &count_text)],
    )
}

pub fn up_next_footer(count: usize, duration: &str) -> String {
    let count_text =
        reprise_core::format::format_thousands(i64::try_from(count).unwrap_or(i64::MAX));
    plural(
        "{count} track · {duration}",
        "{count} tracks · {duration}",
        count,
        &[("count", &count_text), ("duration", duration)],
    )
}

pub fn queue_continuing_from(source: &str) -> String {
    formatted(N_!("Continuing from “{source}”"), &[("source", source)])
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

pub fn new_releases_updated_ago(timestamp: i64, now: i64) -> String {
    let age = now.saturating_sub(timestamp).max(0);
    if age < 60 {
        return text(UPDATED_JUST_NOW);
    }
    if age < 60 * 60 {
        let minutes = age / 60;
        return formatted(
            N_!("Updated {age} min ago"),
            &[("age", &minutes.to_string())],
        );
    }
    if age < 24 * 60 * 60 {
        let hours = age / (60 * 60);
        return formatted(N_!("Updated {age} h ago"), &[("age", &hours.to_string())]);
    }
    let days = age / (24 * 60 * 60);
    formatted(N_!("Updated {age} d ago"), &[("age", &days.to_string())])
}

pub fn new_releases_hidden(count: usize) -> String {
    let count = count.to_string();
    formatted(N_!("{count} hidden · Show"), &[("count", &count)])
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
