#![allow(dead_code)]

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::{formatted, plural};

pub const PODCASTS: &str = N_!("Podcasts");
pub const YOUTUBE: &str = N_!("YouTube");
pub const PODCASTS_DESCRIPTION: &str =
    N_!("Contacts publishers and Apple Podcasts for feeds and search; YouTube sources use yt-dlp");
pub const PODCAST_DATE: &str = N_!("Date");
pub const PODCAST_EPISODE: &str = N_!("Episode");
pub const PODCAST_SHOW: &str = N_!("Show");
pub const PODCAST_LENGTH: &str = N_!("Length");
pub const PODCAST_SOURCE: &str = N_!("Source");
pub const PODCAST_STATUS: &str = N_!("Status");
pub const PODCAST_SOURCE_RSS: &str = N_!("RSS");
pub const PODCAST_SOURCE_YOUTUBE: &str = N_!("YouTube");
pub const PODCAST_STATUS_NEW: &str = N_!("New");
pub const PODCAST_STATUS_RESUME: &str = N_!("Resume");
pub const PODCAST_STATUS_PLAYED: &str = N_!("Played");
pub const PODCAST_TODAY: &str = N_!("Today");
pub const PODCAST_YESTERDAY: &str = N_!("Yesterday");
pub const PODCAST_ADD: &str = N_!("Add podcast");
pub const YOUTUBE_ADD: &str = N_!("Add YouTube channel");
pub const PODCAST_ADD_FILTER: &str = N_!("Add filter");
pub const PODCAST_FILTER_UNPLAYED: &str = N_!("Unplayed");
pub const PODCAST_FILTER_DOWNLOADED: &str = N_!("Downloaded");
pub const PODCAST_FILTER_SHOW: &str = N_!("Show");
pub const PODCAST_FILTER_SOURCE: &str = N_!("Source");
pub const PODCAST_CLEAR_ALL: &str = N_!("Clear all");
pub const PODCAST_GROUP_FACTS: &str =
    N_!("{episodes} · {unplayed} new · latest {latest} · {downloaded}");
/// `SRC-10`: the shared empty-state grammar's copy for Podcasts — title, one
/// paragraph of what lands here and where it comes from, the primary
/// button, and a quiet secondary line. The design's approved secondary text
/// mentions OPML import ("or import an OPML file"); no OPML import path
/// exists in this codebase (see `docs/plans/podcasts-youtube-radio-turn6.md`
/// O-7), so this uses the URL path wording instead, matching YouTube's.
pub const PODCAST_NO_PODCASTS: &str = N_!("No podcasts yet");
pub const PODCAST_NO_PODCASTS_DESCRIPTION: &str = N_!(
    "Search by name or paste a feed URL. New episodes arrive on their own and remember where you stopped listening."
);
pub const PODCAST_NO_PODCASTS_SECONDARY: &str = N_!("or paste a feed URL in the dialog");
pub const YOUTUBE_NO_CHANNELS: &str = N_!("No channels yet");
pub const YOUTUBE_NO_CHANNELS_DESCRIPTION: &str = N_!(
    "Subscribe to a channel and its uploads appear here as audio-only episodes — long mixes, sets, instrumentals. Shorts stay hidden."
);
/// The empty state's primary button — deliberately shorter than the
/// toolbar's `YOUTUBE_ADD` ("Add YouTube channel"): the page's own glyph and
/// title already say YouTube, so the button need not repeat it.
pub const YOUTUBE_NO_CHANNELS_ADD: &str = N_!("Add channel");
pub const YOUTUBE_NO_CHANNELS_SECONDARY: &str = N_!("or paste a channel URL in the dialog");
pub const PODCAST_NO_EPISODES: &str = N_!("No episodes yet");
pub const PODCAST_NO_EPISODES_DESCRIPTION: &str =
    N_!("Refresh subscriptions to check for new episodes.");
/// Block B2: the "Downloaded" filter matching nothing — distinct copy from
/// `PODCAST_NO_EPISODES` so the reason (nothing downloaded, not nothing
/// published) is actually named.
pub const PODCAST_NO_DOWNLOADS: &str = N_!("Nothing downloaded yet");
pub const PODCAST_NO_DOWNLOADS_DESCRIPTION: &str =
    N_!("Episodes you download for offline listening will appear here.");
/// Block B2 / `SRC-10` addendum: a source whose own module is switched off,
/// with nothing subscribed yet. `{source}` is filled with the Podcasts or
/// YouTube page title.
pub const PODCAST_SOURCE_OFF_TITLE: &str = N_!("{source} is turned off");
pub const PODCAST_SOURCE_OFF_DESCRIPTION: &str = N_!(
    "Turn it back on in Online sources to search, subscribe, and fetch new episodes again. Existing subscriptions are kept."
);
pub const PODCAST_ENABLE_IN_PREFERENCES: &str = N_!("Enable in Preferences");
pub const PODCAST_REFRESH_NOW: &str = N_!("Refresh now");
pub const PODCAST_REFRESHING: &str = N_!("Refreshing podcasts…");
pub const PODCAST_REFRESH_FAILED: &str = N_!("Refresh failed · showing saved episodes");
pub const PODCAST_DIALOG_TITLE: &str = N_!("Add Podcast");
pub const PODCAST_DIALOG_HINT: &str = N_!("Search by name or paste a feed URL");
pub const YOUTUBE_DIALOG_TITLE: &str = N_!("Add Channel");
pub const YOUTUBE_DIALOG_HINT: &str = N_!("Search or paste a channel URL");
/// `SRC-6`: a source-foreign URL is refused, never silently handed over.
pub const PODCAST_URL_IS_YOUTUBE: &str = N_!("That is a YouTube channel — add it under YouTube");
pub const YOUTUBE_URL_IS_FEED: &str = N_!("That is an RSS feed — add it under Podcasts");
/// `NET-3` point 4: search needs the network, so it is refused offline with
/// this one-line reason while the URL path stays open. One wording per
/// dialog since the alternative it points to differs (feed vs. channel URL).
pub const PODCAST_SEARCH_NEEDS_NETWORK: &str =
    N_!("Search needs a connection — paste a feed URL instead");
pub const YOUTUBE_SEARCH_NEEDS_NETWORK: &str =
    N_!("Search needs a connection — paste a channel URL instead");
/// `NET-3` point 4: a pasted URL still creates the subscription while
/// offline — this confirms it went through without pretending a preview
/// happened.
pub const PODCAST_ADDED_OFFLINE: &str = N_!("Added — details fill in once you're back online");
pub const PODCAST_SEARCH: &str = N_!("Search");
pub const PODCAST_PREVIEW: &str = N_!("Preview");
pub const PODCAST_SEARCHING: &str = N_!("Searching…");
pub const PODCAST_APPLE_RESULTS: &str = N_!("PODCASTS · APPLE PODCASTS");
pub const PODCAST_YOUTUBE_RESULTS: &str = N_!("YOUTUBE · audio only");
pub const PODCAST_SUBSCRIBE: &str = N_!("Subscribe");
pub const PODCAST_CANCEL: &str = N_!("Cancel");
pub const PODCAST_RSS_DETECTED: &str = N_!("RSS feed detected");
pub const PODCAST_YOUTUBE_DETECTED: &str =
    N_!("YouTube channel detected — videos become episodes · audio only via yt-dlp");
pub const PODCAST_IMPORT_LATEST: &str = N_!("Import the latest episodes");
pub const PODCAST_AUTO_DOWNLOAD: &str = N_!("Download new episodes automatically");
pub const PODCAST_YOUTUBE_FOOTNOTE: &str =
    N_!("YouTube subscriptions are played audio-only via yt-dlp.");
pub const PODCAST_PREVIEW_FAILED: &str = N_!("Could not preview this podcast");
pub const PODCAST_SEARCH_FAILED: &str = N_!("Could not search for podcasts");
pub const PODCAST_SUBSCRIBE_FAILED: &str = N_!("Could not subscribe to this podcast");
pub const PODCAST_ALREADY_SUBSCRIBED: &str = N_!("This source is already subscribed");
pub const PODCAST_YTDLP_MISSING: &str =
    N_!("YouTube component is unavailable — reinstall or repair Reprise");
pub const PODCAST_YTDLP_BLOCKED: &str =
    N_!("YouTube blocked the request — update yt-dlp (Preferences)");
pub const PODCAST_RESOLVING_AUDIO: &str = N_!("Resolving audio…");
pub const PODCAST_PLAY: &str = N_!("Play");
pub const PODCAST_COPY_URL: &str = N_!("Copy episode URL");
pub const PODCAST_MARK_PLAYED: &str = N_!("Mark as played");
pub const PODCAST_MARK_UNPLAYED: &str = N_!("Mark as unplayed");
pub const PODCAST_DOWNLOAD: &str = N_!("Download episode");
pub const PODCAST_DELETE_DOWNLOAD: &str = N_!("Delete download");
pub const PODCAST_NOT_DOWNLOADED: &str = N_!("Not downloaded");
pub const PODCAST_DOWNLOAD_QUEUED: &str = N_!("Queued");
pub const PODCAST_DOWNLOADING: &str = N_!("Downloading");
pub const PODCAST_DOWNLOAD_MISSING: &str = N_!("File missing");
pub const PODCAST_DOWNLOAD_FAILED: &str = N_!("Download failed");
/// `POD-13`: the retry action's tooltip, distinct from `PODCAST_DOWNLOAD` so
/// a failed row's action button reads as "try again", not "download for the
/// first time".
pub const PODCAST_RETRY_DOWNLOAD: &str = N_!("Retry download");
pub const PODCAST_REMOVE_EPISODE: &str = N_!("Remove episode");
pub const PODCAST_MORE_OPTIONS: &str = N_!("More episode options");
pub const PODCAST_MORE_SOURCE_OPTIONS: &str = N_!("More source options");
pub const PODCAST_UNSUBSCRIBE: &str = N_!("Unsubscribe");
pub const PODCAST_SYNC_PHONE: &str = N_!("Sync downloaded episodes to phone");
pub const PODCAST_STOP_SYNC_PHONE: &str = N_!("Stop syncing episodes to phone");
pub const PODCAST_SYNC_DEVICES: &str = N_!("Sync downloaded episodes to devices");
pub const YOUTUBE_LOAD_MORE: &str = N_!("Load more");
pub const YOUTUBE_LOADING_MORE: &str = N_!("Loading more videos…");
pub const YOUTUBE_BACK_TO_CHANNELS: &str = N_!("Back to YouTube channels");
pub const YOUTUBE_HIDE_SHORTS: &str = N_!("Hide Shorts");
/// `POD-14`: every currently available entry on this channel is a Short and
/// Shorts are hidden.
pub const YOUTUBE_SHORTS_ONLY_TITLE: &str = N_!("Only Shorts here");
pub const YOUTUBE_SHORTS_ONLY_DESCRIPTION: &str =
    N_!("Every recent upload from this channel is a Short. Show them anyway?");
pub const YOUTUBE_SHOW_SHORTS_ANYWAY: &str = N_!("Show Shorts anyway");
pub const YOUTUBE_OPEN_CHANNEL: &str = N_!("Open channel");
pub const YOUTUBE_SELECT_EPISODES: &str = N_!("Select episodes");
pub const YOUTUBE_DOWNLOAD_SELECTED: &str = N_!("Download selected");
pub const YOUTUBE_REMOVE_SELECTED: &str = N_!("Remove selected");
pub const PODCAST_UNDO: &str = N_!("Undo");
pub const PODCAST_DELETE_FILES: &str = N_!("Delete files");
pub const PODCAST_PLAY_NEXT_EPISODE: &str = N_!("Play next episode");
pub const PODCAST_EPISODES_PER_SHOW: &str = N_!("Episodes per show");
pub const PODCAST_PREFERENCES_AUTO_DOWNLOAD: &str = N_!("Download new episodes");
pub const PODCAST_PREFERENCES_CLEANUP: &str = N_!("Delete played episodes");
pub const PODCAST_CLEANUP_KEEP_ALL: &str = N_!("Keep all");
pub const PODCAST_CLEANUP_DELETE_PLAYED: &str = N_!("After 7 days");
pub const PODCAST_CLEANUP_KEEP_LAST: &str = N_!("Keep last 5 per show");
pub const YOUTUBE_EPISODES_PER_CHANNEL: &str = N_!("Episodes per channel");
pub const PODCAST_YTDLP: &str = N_!("yt-dlp");
pub const PODCAST_YTDLP_UPDATE: &str = N_!("Update");
pub const PODCAST_YTDLP_CHECKING: &str = N_!("Checking installed version…");
pub const PODCAST_UPDATED_JUST_NOW: &str = N_!("Updated just now");
pub const PODCAST_SUBSCRIBERS: &str = N_!("{count} subscribers");

pub fn podcast_episode_count(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} episode",
        "{count} episodes",
        count,
        &[("count", &count_text)],
    )
}

pub fn podcast_group_facts(
    episodes: &str,
    unplayed: usize,
    latest: &str,
    downloaded: &str,
) -> String {
    let unplayed = unplayed.to_string();
    formatted(
        PODCAST_GROUP_FACTS,
        &[
            ("episodes", episodes),
            ("unplayed", &unplayed),
            ("latest", latest),
            ("downloaded", downloaded),
        ],
    )
}

pub const PODCAST_LIBRARY_SUMMARY: &str = N_!("{shows} · {episodes} · {new} new");

/// `G2` (design 6a): the page-level header line above the grouped list,
/// e.g. "4 shows · 41 episodes · 7 new". `shows` and `episodes` each get
/// their own singular/plural form; `new` is a bare count — the design has
/// no distinct singular wording for it ("1 new", not "1 new one").
pub fn podcast_library_summary(shows: usize, episodes: usize, new: usize) -> String {
    let shows_text = shows.to_string();
    let shows_text = plural(
        "{shows} show",
        "{shows} shows",
        shows,
        &[("shows", &shows_text)],
    );
    formatted(
        PODCAST_LIBRARY_SUMMARY,
        &[
            ("shows", &shows_text),
            ("episodes", &podcast_episode_count(episodes)),
            ("new", &new.to_string()),
        ],
    )
}

pub fn podcast_filtered_count(visible: usize, total: usize) -> String {
    formatted(
        N_!("{visible} of {total} episodes"),
        &[
            ("visible", &visible.to_string()),
            ("total", &total.to_string()),
        ],
    )
}

/// Block B2 / `SRC-10` addendum: fills `{source}` with the page title
/// ("Podcasts" or "YouTube") for the module-off empty state.
pub fn podcast_source_off_title(source: &str) -> String {
    formatted(PODCAST_SOURCE_OFF_TITLE, &[("source", source)])
}

pub fn podcast_unsubscribe_from(show: &str) -> String {
    formatted(N_!("Unsubscribe from “{show}”"), &[("show", show)])
}

pub fn podcast_sync_device(device: &str) -> String {
    formatted(
        N_!("Sync downloaded episodes to “{device}”"),
        &[("device", device)],
    )
}

/// `POD-11`: the YouTube channel detail's header summary — the currently
/// listed window's size, how many of the channel's episodes are downloaded,
/// and their combined size on disk (e.g. "10 of 487 · 3 downloaded ·
/// 1.2 GB").
pub fn youtube_channel_summary(
    shown: usize,
    available: usize,
    downloaded_count: usize,
    downloaded_bytes: u64,
) -> String {
    formatted(
        N_!("{shown} of {available} · {downloaded} downloaded · {size}"),
        &[
            ("shown", &shown.to_string()),
            ("available", &available.to_string()),
            ("downloaded", &downloaded_count.to_string()),
            ("size", &compact_file_size(downloaded_bytes)),
        ],
    )
}

pub fn youtube_selected_count(count: usize) -> String {
    formatted(N_!("{count} selected"), &[("count", &count.to_string())])
}

pub fn podcast_stop_sync_device(device: &str) -> String {
    formatted(
        N_!("Stop syncing episodes to “{device}”"),
        &[("device", device)],
    )
}

pub fn podcast_removed_episode(title: &str) -> String {
    formatted(N_!("Removed “{title}”"), &[("title", title)])
}

pub fn podcast_play_next(title: &str) -> String {
    formatted(N_!("Play next: “{title}”"), &[("title", title)])
}

pub fn podcast_downloads_kept(shows: usize, downloads: usize) -> String {
    let shows_text = shows.to_string();
    let downloads_text = downloads.to_string();
    if shows == 1 {
        return plural(
            "{downloads} download kept",
            "{downloads} downloads kept",
            downloads,
            &[("downloads", &downloads_text)],
        );
    }
    formatted(
        N_!("{shows} shows — {downloads} downloads kept"),
        &[("shows", &shows_text), ("downloads", &downloads_text)],
    )
}

pub fn podcast_import_latest_count(count: usize) -> String {
    formatted(
        N_!("Import the latest {count} episodes"),
        &[("count", &count.to_string())],
    )
}

pub fn podcast_youtube_channel_matches(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} matching video · audio only",
        "{count} matching videos · audio only",
        count,
        &[("count", &count_text)],
    )
}

pub fn podcast_updated_minutes_ago(minutes: i64) -> String {
    formatted(
        N_!("Updated {minutes} min ago"),
        &[("minutes", &minutes.to_string())],
    )
}

/// `SRC-9`: a compact, optional subscriber count for channel discovery rows.
/// Absent counts are omitted entirely — never rendered as zero or "unknown".
pub fn podcast_subscriber_count(followers: u64) -> String {
    let count = compact_count(followers);
    formatted(PODCAST_SUBSCRIBERS, &[("count", &count)])
}

fn compact_count(value: u64) -> String {
    const THOUSAND: u64 = 1_000;
    const MILLION: u64 = 1_000_000;
    match value {
        value if value < THOUSAND => value.to_string(),
        value if value < MILLION => compact_unit(value, THOUSAND, "k"),
        value => compact_unit(value, MILLION, "M"),
    }
}

/// One decimal place, with a trailing `.0` dropped so 62 000 reads `62k`.
fn compact_unit(value: u64, unit: u64, suffix: &str) -> String {
    let whole = value / unit;
    let tenths = (value % unit) * 10 / unit;
    if tenths == 0 {
        format!("{whole}{suffix}")
    } else {
        format!("{whole}.{tenths}{suffix}")
    }
}

/// `POD-11`: compact, truthful byte-size formatting for the YouTube channel
/// detail's per-episode download column and header summary — e.g.
/// "148 MB", "1.2 GB". Below one binary-tier unit the exact byte count is
/// shown; at or above it, one decimal place is used until the scaled value
/// reaches double digits, at which point the decimal is dropped so large
/// sizes stay compact.
pub fn compact_file_size(bytes: u64) -> String {
    const KIB: f64 = 1_024.0;
    const MIB: f64 = KIB * 1_024.0;
    const GIB: f64 = MIB * 1_024.0;
    let value = bytes as f64;
    if value < KIB {
        return format!("{bytes} B");
    }
    let (scaled, unit) = if value < MIB {
        (value / KIB, "KB")
    } else if value < GIB {
        (value / MIB, "MB")
    } else {
        (value / GIB, "GB")
    };
    if scaled < 10.0 {
        format!("{scaled:.1} {unit}")
    } else {
        format!("{:.0} {unit}", scaled.round())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn episode_count_uses_singular_and_plural_copy() {
        assert_eq!(podcast_episode_count(1), "1 episode");
        assert_eq!(podcast_episode_count(23), "23 episodes");
    }

    /// `G2` (design 6a): matches the owner's design example verbatim.
    #[test]
    fn pod_9_library_summary_matches_the_owners_design_example() {
        assert_eq!(
            podcast_library_summary(4, 41, 7),
            "4 shows · 41 episodes · 7 new"
        );
    }

    /// `G2`: singular forms at exactly one, not just the plural path.
    #[test]
    fn pod_9_library_summary_uses_singular_forms_at_one() {
        assert_eq!(
            podcast_library_summary(1, 1, 0),
            "1 show · 1 episode · 0 new"
        );
    }

    #[test]
    fn unsubscribe_download_summary_distinguishes_one_and_many_shows() {
        assert_eq!(podcast_downloads_kept(1, 2), "2 downloads kept");
        assert_eq!(podcast_downloads_kept(3, 12), "3 shows — 12 downloads kept");
    }

    #[test]
    fn youtube_search_count_describes_channel_matches() {
        assert_eq!(
            podcast_youtube_channel_matches(2),
            "2 matching videos · audio only"
        );
    }

    #[test]
    fn compact_file_size_shows_exact_bytes_below_one_kilobyte() {
        assert_eq!(compact_file_size(0), "0 B");
        assert_eq!(compact_file_size(512), "512 B");
        assert_eq!(compact_file_size(1_023), "1023 B");
    }

    #[test]
    fn compact_file_size_switches_tier_at_each_1024_boundary() {
        assert_eq!(compact_file_size(1_024), "1.0 KB");
        assert_eq!(compact_file_size(1_048_575), "1024 KB");
        assert_eq!(compact_file_size(1_048_576), "1.0 MB");
        assert_eq!(compact_file_size(1_073_741_824), "1.0 GB");
    }

    #[test]
    fn compact_file_size_drops_the_decimal_at_ten_and_above() {
        assert_eq!(compact_file_size(9 * 1_048_576), "9.0 MB");
        assert_eq!(compact_file_size(10 * 1_048_576), "10 MB");
    }

    #[test]
    fn compact_file_size_matches_the_owners_design_examples() {
        assert_eq!(compact_file_size(148 * 1_048_576), "148 MB");
        assert_eq!(compact_file_size(1_288_490_189), "1.2 GB");
    }

    #[test]
    fn pod_11_channel_summary_line_matches_the_owners_design_example() {
        assert_eq!(
            youtube_channel_summary(10, 487, 3, 1_288_490_189),
            "10 of 487 · 3 downloaded · 1.2 GB"
        );
    }
}
