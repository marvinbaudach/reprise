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
pub const PODCAST_FILTER_SHOW: &str = N_!("Show");
pub const PODCAST_FILTER_SOURCE: &str = N_!("Source");
pub const PODCAST_CLEAR_ALL: &str = N_!("Clear all");
pub const PODCAST_GROUP_FACTS: &str =
    N_!("{episodes} · {unplayed} new · latest {latest} · {downloaded}");
pub const PODCAST_NO_PODCASTS: &str = N_!("No podcasts yet");
pub const PODCAST_NO_PODCASTS_DESCRIPTION: &str = N_!("Add a show to receive its latest episodes.");
pub const YOUTUBE_NO_CHANNELS: &str = N_!("No YouTube channels yet");
pub const YOUTUBE_NO_CHANNELS_DESCRIPTION: &str =
    N_!("Subscribe to a channel to receive its latest videos.");
pub const PODCAST_NO_EPISODES: &str = N_!("No episodes yet");
pub const PODCAST_NO_EPISODES_DESCRIPTION: &str =
    N_!("Refresh subscriptions to check for new episodes.");
pub const PODCAST_REFRESH_NOW: &str = N_!("Refresh now");
pub const PODCAST_REFRESHING: &str = N_!("Refreshing podcasts…");
pub const PODCAST_REFRESH_FAILED: &str = N_!("Refresh failed · showing saved episodes");
pub const PODCAST_DIALOG_TITLE: &str = N_!("Add Podcast");
pub const PODCAST_DIALOG_HINT: &str = N_!("Search or paste an RSS / YouTube URL");
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
    N_!("yt-dlp is not installed — YouTube sources are disabled");
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
pub const PODCAST_REMOVE_EPISODE: &str = N_!("Remove episode");
pub const PODCAST_MORE_OPTIONS: &str = N_!("More episode options");
pub const PODCAST_MORE_SOURCE_OPTIONS: &str = N_!("More source options");
pub const PODCAST_UNSUBSCRIBE: &str = N_!("Unsubscribe");
pub const PODCAST_SYNC_PHONE: &str = N_!("Sync downloaded episodes to phone");
pub const PODCAST_STOP_SYNC_PHONE: &str = N_!("Stop syncing episodes to phone");
pub const PODCAST_UNDO: &str = N_!("Undo");
pub const PODCAST_DELETE_FILES: &str = N_!("Delete files");
pub const PODCAST_PLAY_NEXT_EPISODE: &str = N_!("Play next episode");
pub const PODCAST_PREFERENCES_IMPORT_COUNT: &str = N_!("Import latest N episodes");
pub const PODCAST_PREFERENCES_AUTO_DOWNLOAD: &str =
    N_!("Download new episodes automatically (default for new subscriptions)");
pub const PODCAST_PREFERENCES_CLEANUP: &str = N_!("Downloads cleanup");
pub const PODCAST_CLEANUP_KEEP_ALL: &str = N_!("Keep all");
pub const PODCAST_CLEANUP_DELETE_PLAYED: &str = N_!("Delete played after 7 days");
pub const PODCAST_CLEANUP_KEEP_LAST: &str = N_!("Keep last 5 per show");
pub const PODCAST_YOUTUBE_SOURCES: &str = N_!("YouTube sources");
pub const PODCAST_YTDLP: &str = N_!("yt-dlp");
pub const PODCAST_YTDLP_UPDATE: &str = N_!("Update");
pub const PODCAST_YTDLP_CHECKING: &str = N_!("Checking installed version…");
pub const PODCAST_REFRESH_INTERVAL: &str = N_!("Refresh every N hours");
pub const PODCAST_UPDATED_JUST_NOW: &str = N_!("Updated just now");

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

pub fn podcast_filtered_count(visible: usize, total: usize) -> String {
    formatted(
        N_!("{visible} of {total} episodes"),
        &[
            ("visible", &visible.to_string()),
            ("total", &total.to_string()),
        ],
    )
}

pub fn podcast_show_all_count(count: usize) -> String {
    formatted(
        N_!("Show all {count} episodes"),
        &[("count", &count.to_string())],
    )
}

pub fn podcast_unsubscribe_from(show: &str) -> String {
    formatted(N_!("Unsubscribe from “{show}”"), &[("show", show)])
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn episode_count_uses_singular_and_plural_copy() {
        assert_eq!(podcast_episode_count(1), "1 episode");
        assert_eq!(podcast_episode_count(23), "23 episodes");
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
}
