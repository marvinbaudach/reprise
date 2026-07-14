//! Rhythmbox-import copy extracted from the central string catalog.

use super::formatted;

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub const RHYTHMBOX_IMPORT_DESCRIPTION: &str =
    N_!("Import ratings, play counts, date-added and last-played information, playlists, and optionally the column layout");
pub const RHYTHMBOX_IMPORT_DIALOG_BODY: &str = N_!("Choose which information to copy into Reprise. Rhythmbox and your audio files remain unchanged.");
pub const RHYTHMBOX_IMPORT_RATINGS: &str = N_!("Ratings");
pub const RHYTHMBOX_IMPORT_PLAY_COUNTS: &str = N_!("Play counts");
pub const RHYTHMBOX_IMPORT_DATE_ADDED: &str = N_!("Date added");
pub const RHYTHMBOX_IMPORT_LAST_PLAYED: &str = N_!("Last played");
pub const RHYTHMBOX_IMPORT_PLAYLISTS: &str = N_!("Playlists");
pub const RHYTHMBOX_IMPORT_ACTION: &str = N_!("Import…");
pub const RHYTHMBOX_IMPORT_COMPLETE: &str = N_!("Rhythmbox import complete");
pub const RHYTHMBOX_IMPORT_FAILED: &str = N_!("Rhythmbox import failed");
pub const RHYTHMBOX_IMPORT_PARTIAL: &str = N_!("Rhythmbox import completed with warnings");

pub fn rhythmbox_import_summary(
    matched: usize,
    ratings: usize,
    play_counts: usize,
    dates: usize,
    last_played: usize,
    skipped: usize,
) -> String {
    let matched = matched.to_string();
    let ratings = ratings.to_string();
    let play_counts = play_counts.to_string();
    let dates = dates.to_string();
    let last_played = last_played.to_string();
    let skipped = skipped.to_string();
    formatted(
        N_!("Matched {matched} tracks · imported {ratings} ratings · raised {play_counts} play counts · restored {dates} date-added values · restored {last_played} last-played values · skipped {skipped}"),
        &[
            ("matched", &matched),
            ("ratings", &ratings),
            ("play_counts", &play_counts),
            ("dates", &dates),
            ("last_played", &last_played),
            ("skipped", &skipped),
        ],
    )
}

pub fn rhythmbox_import_error(error: &str) -> String {
    formatted(
        N_!("Could not import Rhythmbox data: {error}"),
        &[("error", error)],
    )
}

pub fn rhythmbox_playlist_import_summary(
    playlists: usize,
    tracks: usize,
    skipped: usize,
) -> String {
    let playlists = playlists.to_string();
    let tracks = tracks.to_string();
    let skipped = skipped.to_string();
    formatted(
        N_!("Imported {playlists} playlists with {tracks} tracks · skipped {skipped} unavailable tracks"),
        &[
            ("playlists", &playlists),
            ("tracks", &tracks),
            ("skipped", &skipped),
        ],
    )
}

pub fn rhythmbox_playlist_import_error(error: &str) -> String {
    formatted(
        N_!("Playlists could not be imported: {error}"),
        &[("error", error)],
    )
}
