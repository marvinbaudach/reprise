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
pub const DOCTOR_RESULTS_SO_FAR: &str = N_!("Results Found So Far");
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
pub const DOCTOR_SOURCE_LOCAL: &str = N_!("Local");
pub const DOCTOR_SOURCE_MUSICBRAINZ: &str = N_!("MusicBrainz");
pub const DOCTOR_SOURCE_ACOUSTID: &str = N_!("AcoustID");
pub const DOCTOR_RECORDING_MBID: &str = N_!("Recording MBID");
pub const DOCTOR_UNKNOWN_TRACK: &str = N_!("Unknown Track");
pub const DOCTOR_EMPTY_VALUE: &str = N_!("— empty —");
pub const DOCTOR_SELECT_CHANGE: &str = N_!("Select tag change");
pub const DOCTOR_TRACK_AND_FIELD: &str = N_!("Track + Field");
pub const DOCTOR_CURRENT: &str = N_!("Current");
pub const DOCTOR_PROPOSED: &str = N_!("Proposed");
pub const DOCTOR_SOURCE: &str = N_!("Source");
pub const DOCTOR_LOW_CONFIDENCE: &str = N_!("Low confidence; review before selecting");
pub const DOCTOR_EDIT_TRACK_TAGS: &str = N_!("Edit track tags…");
pub const DOCTOR_NO_CHANGES: &str = N_!("No Changes to Review");
pub const DOCTOR_NO_CHANGES_DESCRIPTION: &str =
    N_!("Return to the results and choose another review filter.");
pub const DOCTOR_ALL_SAFE: &str = N_!("All Safe");
pub const DOCTOR_NONE: &str = N_!("None");
pub const DOCTOR_REVIEW_TITLE: &str = N_!("Review Tag Changes");
pub const DOCTOR_PICK_ONE: &str = N_!("Pick one spelling to materialize its track changes.");
pub const DOCTOR_UPDATING_TAGS: &str = N_!("Updating tags…");
pub const DOCTOR_REVERTING_TAGS: &str = N_!("Reverting tags…");
pub const DOCTOR_PROGRESS: &str = N_!("Library Doctor progress");
pub const DOCTOR_CONTROLS_LOCKED: &str = N_!("Locked while a Library Doctor job is running");
pub const TAG_WRITE_BUSY: &str = N_!("Another tag-writing job is already running");
pub const DOCTOR_REVERT: &str = N_!("Revert");
pub const DOCTOR_DETAILS: &str = N_!("Details");
pub const DOCTOR_STATUS_APPLIED: &str = N_!("Applied");
pub const DOCTOR_STATUS_REVERTED: &str = N_!("Reverted");
pub const DOCTOR_STATUS_REMAINING: &str = N_!("Remaining");
pub const DOCTOR_STATUS_CONFLICT: &str = N_!("Conflict");
pub const DOCTOR_STATUS_STALE: &str = N_!("Stale");
pub const DOCTOR_STATUS_FAILED: &str = N_!("Failed");
pub const DOCTOR_CLEANUP_STATUS: &str = N_!("Cleanup Status");
pub const DOCTOR_REVERT_STATUS: &str = N_!("Revert Status");
pub const DOCTOR_LOCAL_ALWAYS_INCLUDED: &str = N_!("Local fixes always included · no network");
pub const DOCTOR_REVERT_LAST_CLEANUP: &str = N_!("Revert Last Cleanup");
pub const DOCTOR_REVERT_AVAILABLE_DISABLED: &str =
    N_!("Available even when Library Doctor is disabled");
pub const DOCTOR_ENABLE_MODULE: &str = N_!("Enable Library Doctor");
pub const DOCTOR_JOB_PAGE_DESCRIPTION: &str =
    N_!("This job continues in the background. Progress and Cancel stay in the sidebar.");
pub const DOCTOR_JOB_FAILED: &str = N_!("Library Doctor Job Failed");

pub fn doctor_remote_confidence(source: &str, confidence: u8) -> String {
    formatted(
        N_!("{source} · {confidence}%"),
        &[("source", source), ("confidence", &confidence.to_string())],
    )
}

pub fn doctor_low_confidence(source: &str, confidence: u8) -> String {
    formatted(
        N_!("{source} · {confidence}% · low confidence"),
        &[("source", source), ("confidence", &confidence.to_string())],
    )
}

pub fn doctor_review_row_description(
    track: &str,
    field: &str,
    current: &str,
    proposed: &str,
    source: &str,
) -> String {
    formatted(
        N_!("{track}, {field}. Current: {current}. Proposed: {proposed}. Source: {source}."),
        &[
            ("track", track),
            ("field", field),
            ("current", current),
            ("proposed", proposed),
            ("source", source),
        ],
    )
}

pub fn doctor_apply_tracks(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "Apply {count} track",
        "Apply {count} tracks",
        count,
        &[("count", &count_text)],
    )
}

pub fn doctor_apply_summary(changes: usize, files: usize) -> String {
    formatted(
        N_!("{changes} tag changes · {files} files · undo available after"),
        &[
            ("changes", &changes.to_string()),
            ("files", &files.to_string()),
        ],
    )
}

pub fn doctor_candidate(value: &str, count: usize) -> String {
    formatted(
        N_!("{value} ({count})"),
        &[("value", value), ("count", &count.to_string())],
    )
}

pub fn doctor_unresolved_spellings(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} spelling, no clear winner",
        "{count} spellings, no clear winner",
        count,
        &[("count", &count_text)],
    )
}

pub fn doctor_evidence_value(label: &str, value: &str) -> String {
    formatted(
        N_!("{label}: {value}"),
        &[("label", label), ("value", value)],
    )
}

pub fn doctor_duration_ms(duration_ms: u64) -> String {
    formatted(
        N_!("Duration: {duration} ms"),
        &[("duration", &duration_ms.to_string())],
    )
}

pub fn doctor_duration_delta_ms(delta_ms: u64) -> String {
    formatted(
        N_!("Duration difference: {delta} ms"),
        &[("delta", &delta_ms.to_string())],
    )
}

pub fn doctor_track_progress(completed: usize, total: usize) -> String {
    formatted(
        N_!("{completed}/{total} tracks"),
        &[
            ("completed", &completed.to_string()),
            ("total", &total.to_string()),
        ],
    )
}

pub fn doctor_tags_updated(count: usize) -> String {
    formatted(
        N_!("Tags updated · {count} tracks"),
        &[("count", &count.to_string())],
    )
}

pub fn doctor_tags_reverted(count: usize) -> String {
    formatted(
        N_!("Tags reverted · {count} tracks"),
        &[("count", &count.to_string())],
    )
}

pub fn doctor_write_cancelled(updated: usize, cancelled: usize) -> String {
    formatted(
        N_!("{updated} tracks updated · {cancelled} cancelled"),
        &[
            ("updated", &updated.to_string()),
            ("cancelled", &cancelled.to_string()),
        ],
    )
}

pub fn doctor_write_failures(updated: usize, failed: usize) -> String {
    formatted(
        N_!("{updated} updated, {failed} failed"),
        &[
            ("updated", &updated.to_string()),
            ("failed", &failed.to_string()),
        ],
    )
}

pub fn doctor_cleanup_summary(applied: usize, remaining: usize) -> String {
    formatted(
        N_!("{applied} applied · {remaining} remaining"),
        &[
            ("applied", &applied.to_string()),
            ("remaining", &remaining.to_string()),
        ],
    )
}

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
