macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::{formatted, plural};

// `plural()` arguments are deliberately NOT wrapped in `N_!`.
//
// `N_!` is a no-op at runtime, but xgettext sees it first: with the wrapper it
// extracts each form as its own singular msgid and never emits the
// `msgid_plural` entry that `ngettext` looks up, so at runtime every plural
// string falls back to English no matter how well the catalog is translated.
// Measured against `po/reprise.pot`: the unwrapped `doctor_change_count` had a
// real `msgid_plural`; the wrapped `doctor_apply_changes` next to it had two
// dead singulars. Leave these bare.

pub const LIBRARY_DOCTOR: &str = N_!("Library Doctor");
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
/// Scan facts, one muted line under the result title: what the scan actually
/// ran with, taken from the stored scan and never from the current controls.
pub const DOCTOR_REMOTE_ON: &str = N_!("MusicBrainz on");
pub const DOCTOR_REMOTE_OFF: &str = N_!("MusicBrainz off");
pub const DOCTOR_RUN_SCAN: &str = N_!("Run Scan Now");
pub const DOCTOR_SCANNING: &str = N_!("Checking tracks…");
#[allow(dead_code)] // Wired by PERF-1 after the single-writer string package lands.
pub const DOCTOR_PHASE_LOCAL: &str = N_!("Reading tags…");
#[allow(dead_code)] // Wired by PERF-1 after the single-writer string package lands.
pub const DOCTOR_PHASE_REMOTE: &str = N_!("Checking against MusicBrainz…");
pub const DOCTOR_CASING_WHITESPACE: &str = N_!("Casing / Whitespace");
pub const DOCTOR_MISSING_ALBUM_ARTIST: &str = N_!("Missing Album Artist");
pub const DOCTOR_GENRE_VARIANTS: &str = N_!("Genre Variants");
pub const DOCTOR_MISSING_WRONG_YEAR: &str = N_!("Missing / Incorrect Year");
pub const DOCTOR_MISSING_RECORDING_MBID: &str = N_!("Missing Recording MBID");
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
pub const DOCTOR_CURRENT: &str = N_!("Current");
pub const DOCTOR_PROPOSED: &str = N_!("Proposed");
pub const DOCTOR_SOURCE: &str = N_!("Source");
pub const DOCTOR_LOW_CONFIDENCE: &str = N_!("Low confidence; review before selecting");
pub const DOCTOR_EDIT_TRACK_TAGS: &str = N_!("Edit track tags…");
pub const DOCTOR_NO_CHANGES: &str = N_!("No Changes to Review");
pub const DOCTOR_NO_CHANGES_DESCRIPTION: &str =
    N_!("Return to the results and choose another review filter.");
pub const DOCTOR_NONE: &str = N_!("None");
pub const DOCTOR_REVIEW_TITLE: &str = N_!("Review Tag Changes");
pub const DOCTOR_PICK_ONE: &str = N_!("Pick one spelling to materialize its track changes.");
pub const DOCTOR_UPDATING_TAGS: &str = N_!("Updating tags…");
pub const DOCTOR_REVERTING_TAGS: &str = N_!("Reverting tags…");
pub const DOCTOR_PROGRESS: &str = N_!("Library Doctor progress");
pub const DOCTOR_CONTROLS_LOCKED: &str = N_!("Locked while a Library Doctor job is running");
pub const TAG_WRITE_BUSY: &str = N_!("Another tag-writing job is already running");
pub const DOCTOR_DETAILS: &str = N_!("Details");
pub const DOCTOR_STATUS_APPLIED: &str = N_!("Applied");
pub const DOCTOR_STATUS_REVERTED: &str = N_!("Reverted");
pub const DOCTOR_STATUS_REMAINING: &str = N_!("Remaining");
pub const DOCTOR_STATUS_CONFLICT: &str = N_!("Conflict");
pub const DOCTOR_STATUS_STALE: &str = N_!("Stale");
pub const DOCTOR_STATUS_FAILED: &str = N_!("Failed");
pub const DOCTOR_REVERT_LAST_CLEANUP: &str = N_!("Revert Last Cleanup");
pub const DOCTOR_JOB_FAILED: &str = N_!("Library Doctor Job Failed");
pub const DOCTOR_START_HEADING: &str = N_!("Check your library");
pub const DOCTOR_START_BODY: &str = N_!(
    "Reprise fixes what is unambiguous — stray spaces, casing, missing MusicBrainz IDs — and asks you about the rest. Everything it does can be undone in one step."
);
pub const DOCTOR_CONFLICTS_BODY: &str =
    N_!("Waiting at the end of the review list. Skippable — nothing breaks if you leave them.");
pub const DOCTOR_CONFLICTS_SECTION: &str = N_!("Spelling conflicts");
pub const DOCTOR_CONFLICTS_OPTIONAL: &str = N_!("Optional · nothing happens if you skip these");
pub const DOCTOR_SKIP_ALL: &str = N_!("Skip all");
pub const DOCTOR_SCAN_AGAIN: &str = N_!("Scan again");
pub const DOCTOR_RESULTS_KEPT: &str = N_!("Results are kept until the next scan.");
pub const DOCTOR_NOTHING_TO_FIX: &str = N_!("Nothing to fix");
pub const DOCTOR_UNDO_EVERYTHING: &str = N_!("Undo everything from this scan");
pub const DOCTOR_DONE: &str = N_!("Done");
pub const DOCTOR_UNDO: &str = N_!("Undo");
pub const DOCTOR_ALL: &str = N_!("All");
pub const DOCTOR_TRACK: &str = N_!("Track");
pub const DOCTOR_FIELD: &str = N_!("Field");
pub const DOCTOR_NO_ALBUM: &str = N_!("No album");
pub const DOCTOR_FILTER_CASING: &str = N_!("Casing");
pub const DOCTOR_FILTER_YEAR: &str = N_!("Year");
pub const DOCTOR_FILTER_GENRE: &str = N_!("Genre");
pub const DOCTOR_FILTER_LABEL: &str = N_!("Filter tag changes");
pub const NEW_PLAYLIST_UNTITLED: &str = N_!("Untitled playlist");

// Narrow layout only. The shared column header is what names the values, and
// below the breakpoint it is hidden — so each value carries its own short
// prefix instead. Keep them short: they sit in front of every value.
pub fn doctor_narrow_current(value: &str) -> String {
    formatted(N_!("Now: {value}"), &[("value", value)])
}

pub fn doctor_narrow_proposed(value: &str) -> String {
    formatted(N_!("New: {value}"), &[("value", value)])
}

pub fn doctor_narrow_source(value: &str) -> String {
    formatted(N_!("From: {value}"), &[("value", value)])
}

pub fn doctor_scan_estimate(tracks: usize, minutes: usize) -> String {
    formatted(
        N_!("{tracks} tracks · about {minutes} minutes"),
        &[
            ("tracks", &tracks.to_string()),
            ("minutes", &minutes.to_string()),
        ],
    )
}

#[allow(dead_code)] // Wired by PERF-5 after the single-writer string package lands.
pub fn doctor_scan_estimate_tracks_only(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} track",
        "{count} tracks",
        count,
        &[("count", &count_text)],
    )
}

pub fn doctor_last_scan(when: &str) -> String {
    formatted(N_!("Last scan · {when}"), &[("when", when)])
}

pub fn doctor_last_scan_fixes(count: usize) -> String {
    formatted(
        N_!("{count} fixes applied · still reversible"),
        &[("count", &count.to_string())],
    )
}

/// Past tense: by the time this heading renders, the quiet job has run.
pub fn doctor_already_applied(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} fix already applied",
        "{count} fixes already applied",
        count,
        &[("count", &count_text)],
    )
}

pub fn doctor_spacing_casing_line(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} stray space and casing correction",
        "{count} stray spaces and casing corrections",
        count,
        &[("count", &count_text)],
    )
}

pub fn doctor_mbid_line(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} MusicBrainz ID filled in — no visible change to your tags",
        "{count} MusicBrainz IDs filled in — no visible change to your tags",
        count,
        &[("count", &count_text)],
    )
}

pub fn doctor_needs_review(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} change needs your eye",
        "{count} changes need your eye",
        count,
        &[("count", &count_text)],
    )
}

pub fn doctor_across_albums(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "across {count} album",
        "across {count} albums",
        count,
        &[("count", &count_text)],
    )
}

pub fn doctor_unresolved_spellings(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} spelling conflict, no clear winner",
        "{count} spelling conflicts, no clear winner",
        count,
        &[("count", &count_text)],
    )
}

/// Live counters on the running page. Deliberately future tense: the quiet
/// write only starts once the scan completes, so mid-scan nothing has been
/// written yet and the page may not claim otherwise.
///
/// The two English forms below are identical on purpose — English inflects
/// nothing here, but German does: "1 wird still korrigiert" against
/// "511 werden still korrigiert". The count still has to travel through
/// ngettext or every translation is stuck with one of the two.
pub fn doctor_will_fix_quietly(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} will be fixed quietly",
        "{count} will be fixed quietly",
        count,
        &[("count", &count_text)],
    )
}

pub fn doctor_waiting_for_you(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} waiting for you",
        "{count} waiting for you",
        count,
        &[("count", &count_text)],
    )
}

pub fn doctor_skipped_facts(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} skipped",
        "{count} skipped",
        count,
        &[("count", &count_text)],
    )
}

/// The one muted line under the result title: scope, whether the network was
/// used, and how many tracks were skipped — the skipped clause only when there
/// were any, because no line on this page may read `0`.
pub fn doctor_scan_facts(scope: &str, remote: &str, skipped: Option<usize>) -> String {
    let mut facts = format!("{scope} · {remote}");
    if let Some(skipped) = skipped.filter(|count| *count > 0) {
        facts.push_str(" · ");
        facts.push_str(&doctor_skipped_facts(skipped));
    }
    facts
}

pub fn doctor_apply_changes(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "Apply {count} change",
        "Apply {count} changes",
        count,
        &[("count", &count_text)],
    )
}

pub fn doctor_nothing_to_fix_body(checked: usize) -> String {
    let checked_text = checked.to_string();
    plural(
        "{checked} track checked. Your tags are already consistent with each other.",
        "{checked} tracks checked. Your tags are already consistent with each other.",
        checked,
        &[("checked", &checked_text)],
    )
}

/// Same sentence with the skipped clause, used only when tracks were actually
/// skipped — otherwise the empty state would print a literal `0`.
pub fn doctor_nothing_to_fix_body_skipped(checked: usize, skipped: usize) -> String {
    let checked_text = checked.to_string();
    let skipped_text = skipped.to_string();
    plural(
        "{checked} tracks checked, {skipped} skipped. Your tags are already consistent with each other.",
        "{checked} tracks checked, {skipped} skipped. Your tags are already consistent with each other.",
        skipped,
        &[("checked", &checked_text), ("skipped", &skipped_text)],
    )
}

pub fn doctor_tracks_checked_heading(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} track checked",
        "{count} tracks checked",
        count,
        &[("count", &count_text)],
    )
}

pub fn doctor_includes_quiet_fixes(count: usize) -> String {
    formatted(
        N_!("Includes the {count} quiet fixes. Available until the next scan."),
        &[("count", &count.to_string())],
    )
}

pub fn doctor_tags_fixed(count: usize) -> String {
    formatted(N_!("{count} tags fixed"), &[("count", &count.to_string())])
}

pub fn doctor_all_tracks(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "All {count} track",
        "All {count} tracks",
        count,
        &[("count", &count_text)],
    )
}

pub fn doctor_preselected_hint() -> String {
    N_!("Everything here is preselected. Uncheck what you disagree with.").to_owned()
}

pub fn doctor_changes_and_albums(changes: usize, albums: usize) -> String {
    formatted(
        N_!("{changes} changes · {albums} albums"),
        &[
            ("changes", &changes.to_string()),
            ("albums", &albums.to_string()),
        ],
    )
}

#[allow(dead_code)] // Wired by REV-2 after the single-writer string package lands.
pub fn doctor_filter_scope(shown: usize, total: usize, filter: &str) -> String {
    formatted(
        N_!("{shown} of {total} · filtered by {filter}"),
        &[
            ("shown", &shown.to_string()),
            ("total", &total.to_string()),
            ("filter", filter),
        ],
    )
}

#[allow(dead_code)] // Wired by REV-4 after the single-writer string package lands.
pub fn doctor_change_count_none_selected(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} change · none selected",
        "{count} changes · none selected",
        count,
        &[("count", &count_text)],
    )
}

#[allow(dead_code)] // Wired by REV-3 after the single-writer string package lands.
pub fn doctor_conflicts_intro(count: usize) -> String {
    formatted(
        N_!(
            "Your library spells these {count} names more than one way. Reprise will not guess. Pick one and the matching track changes appear above."
        ),
        &[("count", &count.to_string())],
    )
}

pub fn doctor_conflict_scope(field: &str, tracks: usize) -> String {
    formatted(
        N_!("{field} · {tracks} tracks"),
        &[("field", field), ("tracks", &tracks.to_string())],
    )
}

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

pub fn doctor_change_count(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} change",
        "{count} changes",
        count,
        &[("count", &count_text)],
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

/// One category line inside the review card: the class, then the count with a
/// noun so ngettext has something to agree with.
pub fn doctor_review_category(class: &str, count: usize) -> String {
    formatted(
        N_!("{class} · {count}"),
        &[("class", class), ("count", &doctor_change_count(count))],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_9d_the_filter_scope_line_names_shown_total_and_filter() {
        assert_eq!(DOCTOR_PHASE_LOCAL, "Reading tags…");
        assert_eq!(DOCTOR_PHASE_REMOTE, "Checking against MusicBrainz…");
        assert_eq!(
            doctor_filter_scope(27, 390, "Year"),
            "27 of 390 · filtered by Year"
        );
        assert_eq!(
            doctor_change_count_none_selected(2),
            "2 changes · none selected"
        );
        assert_eq!(doctor_scan_estimate_tracks_only(390), "390 tracks");
        assert_eq!(
            doctor_conflicts_intro(4),
            "Your library spells these 4 names more than one way. Reprise will not guess. Pick one and the matching track changes appear above."
        );
        assert!(
            !include_str!("strings_library_doctor.rs").contains("plural(\n        N_!("),
            "plural forms must remain bare so xgettext emits msgid_plural"
        );
    }
}
