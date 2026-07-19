macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::{formatted, plural};

pub const LIBRARY_DOCTOR: &str = N_!("Library Doctor");
pub const LIBRARY_DOCTOR_DESCRIPTION: &str =
    N_!("Review local tag cleanup suggestions; optional remote suggestions; contacts MusicBrainz / AcoustID");
pub const LIBRARY_DOCTOR_REMOTE: &str = N_!("MusicBrainz / AcoustID suggestions");
pub const LIBRARY_DOCTOR_REMOTE_DESCRIPTION: &str =
    N_!("Optional network lookup · local fixes are always included · no file paths or private library data");
pub const LIBRARY_DOCTOR_REMOTE_HEADING: &str = N_!("Enable remote tag suggestions?");
pub const LIBRARY_DOCTOR_REMOTE_BODY: &str = N_!(
    "Reprise sends only existing title, artist, album, album artist, MusicBrainz IDs, and duration to MusicBrainz. AcoustID receives only a fingerprint and duration. File paths, filenames, library roots, ratings, listening history, playlists, and device data are never sent."
);
pub const LIBRARY_DOCTOR_REMOTE_ENABLE: &str = N_!("Enable Suggestions");
pub const DOCTOR_SCOPE: &str = N_!("Scope");
pub const DOCTOR_SCOPE_WHOLE_LIBRARY: &str = N_!("Whole Library");
pub const DOCTOR_SCOPE_CURRENT_VIEW: &str = N_!("Current View");
pub const DOCTOR_SCOPE_SELECTION: &str = N_!("Selection");
pub const DOCTOR_SCAN_OPTIONS: &str = N_!("Scan Options");
pub const DOCTOR_RUN_SCAN: &str = N_!("Run Scan Now");
pub const DOCTOR_SCANNING: &str = N_!("Checking tracks…");
pub const DOCTOR_RESULTS: &str = N_!("Results");
pub const DOCTOR_SAFE_FIXES: &str = N_!("Safe · local, preselected");
pub const DOCTOR_SUGGESTIONS: &str = N_!("Suggestions · review");
pub const DOCTOR_UNRESOLVED_GROUPS: &str = N_!("Unresolved Groups");
pub const DOCTOR_TRACKS_CHECKED: &str = N_!("Tracks Checked · skipped");
pub const DOCTOR_CASING_WHITESPACE: &str = N_!("Casing / Whitespace");
pub const DOCTOR_MISSING_ALBUM_ARTIST: &str = N_!("Missing Album Artist");
pub const DOCTOR_GENRE_VARIANTS: &str = N_!("Genre Variants");
pub const DOCTOR_MISSING_WRONG_YEAR: &str = N_!("Missing / Incorrect Year");
pub const DOCTOR_MISSING_RECORDING_MBID: &str = N_!("Missing Recording MBID");
pub const DOCTOR_REVIEW_CHANGES: &str = N_!("Review Changes");
pub const DOCTOR_REVIEW_SAFE: &str = N_!("Review Safe Fixes");
pub const DOCTOR_NO_RESULTS: &str = N_!("No Library Doctor Results Yet");
pub const DOCTOR_NO_RESULTS_DESCRIPTION: &str =
    N_!("Choose a scope and run a read-only scan. No tags are changed here.");
pub const DOCTOR_SCOPE_FALLBACK: &str =
    N_!("That scope is no longer available. Scanning the whole library instead.");
pub const DOCTOR_ACOUSTID_UNAVAILABLE: &str = N_!("AcoustID Unavailable");
pub const DOCTOR_ACOUSTID_UNAVAILABLE_DESCRIPTION: &str =
    N_!("Local checks and MusicBrainz suggestions remain available.");

pub fn doctor_change_count(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} change",
        "{count} changes",
        count,
        &[("count", &count_text)],
    )
}

pub fn doctor_group_count(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} group",
        "{count} groups",
        count,
        &[("count", &count_text)],
    )
}

pub fn doctor_problem_counts(safe: usize, review: usize) -> String {
    formatted(
        N_!("{safe} safe · {review} review"),
        &[("safe", &safe.to_string()), ("review", &review.to_string())],
    )
}

pub fn doctor_checked_counts(checked: usize, skipped: usize) -> String {
    formatted(
        N_!("{checked} checked · {skipped} skipped"),
        &[
            ("checked", &checked.to_string()),
            ("skipped", &skipped.to_string()),
        ],
    )
}

pub fn doctor_review_changes(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "Review {count} change",
        "Review {count} changes",
        count,
        &[("count", &count_text)],
    )
}

pub fn doctor_review_safe_fixes(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "Review {count} safe fix",
        "Review {count} safe fixes",
        count,
        &[("count", &count_text)],
    )
}
