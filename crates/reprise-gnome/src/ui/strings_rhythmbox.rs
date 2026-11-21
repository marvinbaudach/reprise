//! Rhythmbox-import copy extracted from the central string catalog.

use super::{formatted, plural};

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

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

// The count-bearing lines below pass bare literals to `plural()` on purpose,
// for the reason spelled out in `strings_library_doctor.rs`: wrapped in `N_!`,
// xgettext would emit dead singulars and never the `msgid_plural` ngettext
// looks up. A line with two counts agrees each one through its own lookup.

pub fn rhythmbox_entries_matched(matched: usize, total: usize) -> String {
    let matched = matched.to_string();
    let total_text = total.to_string();
    plural(
        "{matched} of {total} Rhythmbox entry matched your library",
        "{matched} of {total} Rhythmbox entries matched your library",
        total,
        &[("matched", &matched), ("total", &total_text)],
    )
}

pub fn rhythmbox_entries_skipped(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} entry skipped",
        "{count} entries skipped",
        count,
        &[("count", &count_text)],
    )
}

fn rhythmbox_entry_count(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} entry",
        "{count} entries",
        count,
        &[("count", &count_text)],
    )
}

pub fn rhythmbox_prescan_info(entries: usize, last_used_days: Option<u64>) -> String {
    let entry_count = rhythmbox_entry_count(entries);
    let Some(days) = last_used_days else {
        return entry_count;
    };
    let days_text = days.to_string();
    let last_used = plural(
        "last used {count} day ago",
        "last used {count} days ago",
        usize::try_from(days).unwrap_or(usize::MAX),
        &[("count", &days_text)],
    );
    formatted(
        N_!("{entries} \u{00b7} {last_used}"),
        &[("entries", &entry_count), ("last_used", &last_used)],
    )
}

pub fn rhythmbox_match_count(matched: usize) -> String {
    let matched_text = matched.to_string();
    plural(
        "{matched} matches your library",
        "{matched} match your library",
        matched,
        &[("matched", &matched_text)],
    )
}

pub fn rhythmbox_rated_subtitle(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} rated track found",
        "{count} rated tracks found",
        count,
        &[("count", &count_text)],
    )
}

pub fn rhythmbox_history_subtitle(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} track with history",
        "{count} tracks with history",
        count,
        &[("count", &count_text)],
    )
}

pub fn rhythmbox_date_added_subtitle() -> String {
    super::text(N_!("Original \u{201c}added to library\u{201d} timeline"))
}

fn rhythmbox_playlist_count(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} playlist",
        "{count} playlists",
        count,
        &[("count", &count_text)],
    )
}

fn rhythmbox_track_count(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} track",
        "{count} tracks",
        count,
        &[("count", &count_text)],
    )
}

pub fn rhythmbox_playlists_subtitle(playlists: usize, tracks: usize) -> String {
    formatted(
        N_!("{playlists} \u{00b7} {tracks}"),
        &[
            ("playlists", &rhythmbox_playlist_count(playlists)),
            ("tracks", &rhythmbox_track_count(tracks)),
        ],
    )
}

pub fn rhythmbox_progress_count(done: usize, total: usize) -> String {
    let done = done.to_string();
    let total_text = total.to_string();
    plural(
        "{done} of {total} track",
        "{done} of {total} tracks",
        total,
        &[("done", &done), ("total", &total_text)],
    )
}

pub fn rhythmbox_result_ratings(count: usize) -> String {
    let count = count.to_string();
    formatted(N_!("{count} imported"), &[("count", &count)])
}

/// The "Play counts & last played" row owns both numbers: how many play counts
/// were raised and how many last-played dates were restored.
pub fn rhythmbox_result_play_counts(raised: usize, last_played: usize) -> String {
    let raised_text = raised.to_string();
    let raised_count = plural(
        "{count} play count raised",
        "{count} play counts raised",
        raised,
        &[("count", &raised_text)],
    );
    let last_played_text = last_played.to_string();
    let last_played_count = formatted(
        N_!("{count} last played restored"),
        &[("count", &last_played_text)],
    );
    formatted(
        N_!("{raised} \u{00b7} {last_played}"),
        &[
            ("raised", &raised_count),
            ("last_played", &last_played_count),
        ],
    )
}

/// The "Date added" row counts dates and nothing else.
pub fn rhythmbox_result_dates(dates: usize) -> String {
    let count = dates.to_string();
    formatted(N_!("{count} restored"), &[("count", &count)])
}

pub fn rhythmbox_result_playlists(count: usize) -> String {
    let count = count.to_string();
    formatted(N_!("{count} created"), &[("count", &count)])
}

/// The pre-import warning: a headline for everything that will be skipped and
/// one line naming each reason that applies.
pub struct RhythmboxSkipWarning {
    pub title: String,
    pub reasons: String,
}

pub fn rhythmbox_skipped_warning(
    outside_library: usize,
    missing_on_disk: usize,
    non_song: usize,
) -> RhythmboxSkipWarning {
    let total = outside_library + missing_on_disk + non_song;
    let total_text = total.to_string();
    let title = plural(
        "{count} Rhythmbox entry will be skipped",
        "{count} Rhythmbox entries will be skipped",
        total,
        &[("count", &total_text)],
    );
    let mut reasons = Vec::new();
    if outside_library > 0 {
        reasons.push(formatted(
            N_!("{count} outside your library folder"),
            &[("count", &outside_library.to_string())],
        ));
    }
    if missing_on_disk > 0 {
        reasons.push(formatted(
            N_!("{count} no longer on disk"),
            &[("count", &missing_on_disk.to_string())],
        ));
    }
    if non_song > 0 {
        reasons.push(plural(
            "{count} podcast or radio stream",
            "{count} podcasts or radio streams",
            non_song,
            &[("count", &non_song.to_string())],
        ));
    }
    RhythmboxSkipWarning {
        title,
        reasons: reasons.join(" \u{00b7} "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every count the dialog names inflects through the catalog: "1 entries"
    /// and "1 matches" were English grammar baked into one form.
    #[test]
    fn rbx_copy_1_every_count_agrees_with_its_noun() {
        assert_eq!(rhythmbox_entries_skipped(1), "1 entry skipped");
        assert_eq!(rhythmbox_entries_skipped(3), "3 entries skipped");
        assert_eq!(
            rhythmbox_entries_matched(1, 1),
            "1 of 1 Rhythmbox entry matched your library"
        );
        assert_eq!(
            rhythmbox_entries_matched(10, 11),
            "10 of 11 Rhythmbox entries matched your library"
        );
        assert_eq!(rhythmbox_prescan_info(1, None), "1 entry");
        assert_eq!(rhythmbox_prescan_info(11, None), "11 entries");
        assert_eq!(
            rhythmbox_prescan_info(1, Some(1)),
            "1 entry \u{b7} last used 1 day ago"
        );
        assert_eq!(
            rhythmbox_prescan_info(11, Some(3)),
            "11 entries \u{b7} last used 3 days ago"
        );
        assert_eq!(rhythmbox_match_count(1), "1 matches your library");
        assert_eq!(rhythmbox_match_count(10), "10 match your library");
        assert_eq!(rhythmbox_rated_subtitle(1), "1 rated track found");
        assert_eq!(rhythmbox_rated_subtitle(4), "4 rated tracks found");
        assert_eq!(rhythmbox_history_subtitle(1), "1 track with history");
        assert_eq!(rhythmbox_history_subtitle(4), "4 tracks with history");
        assert_eq!(
            rhythmbox_playlists_subtitle(1, 1),
            "1 playlist \u{b7} 1 track"
        );
        assert_eq!(
            rhythmbox_playlists_subtitle(2, 7),
            "2 playlists \u{b7} 7 tracks"
        );
        assert_eq!(rhythmbox_progress_count(1, 1), "1 of 1 track");
        assert_eq!(rhythmbox_progress_count(10, 10), "10 of 10 tracks");
    }

    /// The Date added row reports dates, and only dates: a last-played count
    /// beside it ("10 \u{b7} 6 restored") read as 6 of 10 dates restored.
    #[test]
    fn rbx_copy_2_date_added_row_counts_its_own_dates_only() {
        assert_eq!(rhythmbox_result_dates(10), "10 restored");
    }

    /// Last played belongs to the "Play counts & last played" row, named as
    /// what it is.
    #[test]
    fn rbx_copy_3_play_count_row_carries_the_last_played_count() {
        assert_eq!(
            rhythmbox_result_play_counts(1, 1),
            "1 play count raised \u{b7} 1 last played restored"
        );
        assert_eq!(
            rhythmbox_result_play_counts(4, 6),
            "4 play counts raised \u{b7} 6 last played restored"
        );
    }

    /// A missing file is not "outside your library folder": the pre-import
    /// warning names each skip reason it actually has, and no other.
    #[test]
    fn rbx_copy_4_the_skip_warning_names_only_the_reasons_that_apply() {
        let missing_only = rhythmbox_skipped_warning(0, 1, 0);
        assert_eq!(missing_only.title, "1 Rhythmbox entry will be skipped");
        assert_eq!(missing_only.reasons, "1 no longer on disk");
        assert!(!missing_only.reasons.contains("outside"));

        let everything = rhythmbox_skipped_warning(2, 1, 5);
        assert_eq!(everything.title, "8 Rhythmbox entries will be skipped");
        assert_eq!(
            everything.reasons,
            "2 outside your library folder \u{b7} 1 no longer on disk \u{b7} 5 podcasts or radio streams"
        );

        let one_stream = rhythmbox_skipped_warning(0, 0, 1);
        assert_eq!(one_stream.reasons, "1 podcast or radio stream");
        assert!(rhythmbox_skipped_warning(0, 0, 0).reasons.is_empty());
    }
}
