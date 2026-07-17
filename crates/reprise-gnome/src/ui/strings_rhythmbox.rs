//! Rhythmbox-import copy extracted from the central string catalog.

use super::formatted;

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub const RHYTHMBOX_IMPORT_DESCRIPTION: &str =
    N_!("Import ratings, play counts, date-added and last-played information, playlists, and optionally the column layout");
pub const RHYTHMBOX_IMPORT_RATINGS: &str = N_!("Ratings");
pub const RHYTHMBOX_IMPORT_DATE_ADDED: &str = N_!("Date added");
pub const RHYTHMBOX_PLAY_COUNTS_AND_LAST_PLAYED: &str = N_!("Play counts & last played");
pub const RHYTHMBOX_IMPORT_PLAYLISTS: &str = N_!("Playlists");
pub const RHYTHMBOX_IMPORT_START: &str = N_!("Import");
/// TIP-2b: prescan failure keeps the import button disabled — say why.
pub const RHYTHMBOX_PRESCAN_FAILED: &str =
    N_!("Could not read the Rhythmbox library — import stays disabled");
pub const RHYTHMBOX_LIBRARY_FOUND: &str = N_!("Rhythmbox library found");
pub const RHYTHMBOX_IMPORT_BODY_RICH: &str = N_!(
    "Choose what to copy into Reprise. Rhythmbox and your audio files remain unchanged \u{2014} you can undo the whole operation."
);
pub const RHYTHMBOX_IMPORT_COMPLETE_HEADING: &str = N_!("Import complete");
pub const RHYTHMBOX_IMPORTING: &str = N_!("Importing from Rhythmbox\u{2026}");
pub const RHYTHMBOX_UNDO_IMPORT: &str = N_!("Undo import");
pub const RHYTHMBOX_SKIP_OUTSIDE_LIBRARY: &str = N_!("Files outside your library folder");
pub const RHYTHMBOX_SKIP_MISSING_ON_DISK: &str = N_!("Files no longer on disk");
pub const RHYTHMBOX_SKIP_NON_SONG: &str = N_!("Podcasts & radio streams");
pub const RHYTHMBOX_DONE: &str = N_!("Done");
pub const RHYTHMBOX_CANCEL: &str = N_!("Cancel");

pub fn rhythmbox_entries_matched(matched: usize, total: usize) -> String {
    let matched = matched.to_string();
    let total = total.to_string();
    formatted(
        N_!("{matched} of {total} Rhythmbox entries matched your library"),
        &[("matched", &matched), ("total", &total)],
    )
}

pub fn rhythmbox_entries_skipped(count: usize) -> String {
    let count = count.to_string();
    formatted(N_!("{count} entries skipped"), &[("count", &count)])
}

pub fn rhythmbox_prescan_info(entries: usize, last_used_days: Option<u64>) -> String {
    let entries = entries.to_string();
    match last_used_days {
        Some(days) => {
            let days = days.to_string();
            formatted(
                N_!("{entries} entries \u{00b7} last used {days} days ago"),
                &[("entries", &entries), ("days", &days)],
            )
        }
        None => formatted(N_!("{entries} entries"), &[("entries", &entries)]),
    }
}

pub fn rhythmbox_match_count(matched: usize) -> String {
    let matched = matched.to_string();
    formatted(
        N_!("{matched} match your library"),
        &[("matched", &matched)],
    )
}

pub fn rhythmbox_rated_subtitle(count: usize) -> String {
    let count = count.to_string();
    formatted(N_!("{count} rated tracks found"), &[("count", &count)])
}

pub fn rhythmbox_history_subtitle(count: usize) -> String {
    let count = count.to_string();
    formatted(N_!("{count} tracks with history"), &[("count", &count)])
}

pub fn rhythmbox_date_added_subtitle() -> String {
    super::text(N_!("Original \u{201c}added to library\u{201d} timeline"))
}

pub fn rhythmbox_playlists_subtitle(playlists: usize, tracks: usize) -> String {
    let playlists = playlists.to_string();
    let tracks = tracks.to_string();
    formatted(
        N_!("{playlists} playlists \u{00b7} {tracks} tracks"),
        &[("playlists", &playlists), ("tracks", &tracks)],
    )
}

pub fn rhythmbox_progress_count(done: usize, total: usize) -> String {
    let done = done.to_string();
    let total = total.to_string();
    formatted(
        N_!("{done} of {total} tracks"),
        &[("done", &done), ("total", &total)],
    )
}

pub fn rhythmbox_result_ratings(count: usize) -> String {
    let count = count.to_string();
    formatted(N_!("{count} imported"), &[("count", &count)])
}

pub fn rhythmbox_result_play_counts(count: usize) -> String {
    let count = count.to_string();
    formatted(N_!("{count} raised"), &[("count", &count)])
}

pub fn rhythmbox_result_dates(dates: usize, last_played: usize) -> String {
    let dates = dates.to_string();
    let last_played = last_played.to_string();
    formatted(
        N_!("{dates} \u{00b7} {last_played} restored"),
        &[("dates", &dates), ("last_played", &last_played)],
    )
}

pub fn rhythmbox_result_playlists(count: usize) -> String {
    let count = count.to_string();
    formatted(N_!("{count} created"), &[("count", &count)])
}

pub fn rhythmbox_skipped_warning(count: usize) -> String {
    let count = count.to_string();
    formatted(
        N_!("{count} Rhythmbox entries point to files outside your library folder \u{2014} they will be skipped."),
        &[("count", &count)],
    )
}
