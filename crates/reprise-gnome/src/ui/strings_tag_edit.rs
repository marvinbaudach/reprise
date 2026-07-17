//! Tag-editor review-footer, save-label, and save-progress/failure copy
//! (TAG-5, FB-3, Tasks F1/F2) — split out of `strings.rs` per this crate's
//! append-only sibling pattern (see `strings_autocomplete.rs`): `strings.rs`
//! itself is already at the 800-line guideline.

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::{formatted, plural};

// --- TAG-5 review footer: summary line + "Review changes" expander ---

pub const TAG_REVIEW_EXPANDER: &str = N_!("Review changes");

const TAG_REVIEW_SUMMARY_FIELD: &str = N_!("{fields} field");
const TAG_REVIEW_SUMMARY_FIELD_PLURAL: &str = N_!("{fields} fields");
const TAG_REVIEW_SUMMARY_TRACKS: &str = N_!("{tracks} track affected");
const TAG_REVIEW_SUMMARY_TRACKS_PLURAL: &str = N_!("{tracks} tracks affected");

/// TAG-5's summary line, e.g. "2 fields · 30 tracks affected" — the same
/// "tracks = real file writes" currency the save label, progress spinner,
/// and toast all share.
pub fn tag_review_summary(fields: usize, tracks_affected: usize) -> String {
    let fields_text = fields.to_string();
    let tracks_text = tracks_affected.to_string();
    let fields_part = plural(
        TAG_REVIEW_SUMMARY_FIELD,
        TAG_REVIEW_SUMMARY_FIELD_PLURAL,
        fields,
        &[("fields", &fields_text)],
    );
    let tracks_part = plural(
        TAG_REVIEW_SUMMARY_TRACKS,
        TAG_REVIEW_SUMMARY_TRACKS_PLURAL,
        tracks_affected,
        &[("tracks", &tracks_text)],
    );
    format!("{fields_part} \u{b7} {tracks_part}")
}

/// TAG-5's zero-effective-changes state: everything scarred but no field
/// actually differs from its original value — replaces the summary line
/// text and doubles as the disabled Save button's tooltip (P-2).
pub const TAG_REVIEW_NO_EFFECTIVE_CHANGES: &str = N_!("No effective changes");

/// P-2's disabled-button reason for the pristine, never-touched state —
/// distinct from [`TAG_REVIEW_NO_EFFECTIVE_CHANGES`] (touched, but net
/// zero) so the tooltip always tells the truth about *why* Save is dead.
pub const TAG_SAVE_NO_CHANGES_YET: &str = N_!("No changes yet");

const TAG_REVIEW_LINE: &str = N_!("{field}: {old} \u{2192} {new} \u{b7} {count} track");
const TAG_REVIEW_LINE_PLURAL: &str = N_!("{field}: {old} \u{2192} {new} \u{b7} {count} tracks");

/// One "Review changes" expander row, e.g. "Artist: Suicide → Suicide
/// Silence · 30 tracks".
pub fn tag_review_line(field: &str, old: &str, new: &str, tracks_affected: usize) -> String {
    let count_text = tracks_affected.to_string();
    plural(
        TAG_REVIEW_LINE,
        TAG_REVIEW_LINE_PLURAL,
        tracks_affected,
        &[
            ("field", field),
            ("old", old),
            ("new", new),
            ("count", &count_text),
        ],
    )
}

const TAG_OLD_VALUE_LINE: &str = N_!("was: {value}");

/// The reserved per-field "was: …" line (TAG-5/P-4): only ever set when the
/// field's effective value differs from its original — the caller decides
/// that, this just formats it.
pub fn tag_old_value_line(value: &str) -> String {
    formatted(TAG_OLD_VALUE_LINE, &[("value", value)])
}

// --- TAG-5 save label: the "scattered pending" SingleNav case ---

const TAG_SAVE_SCATTERED: &str = N_!("Save \u{b7} {count} track");
const TAG_SAVE_SCATTERED_PLURAL: &str = N_!("Save \u{b7} {count} tracks");

/// SingleNav's "pending on more than the current track" save label (TAG-5:
/// "verstreut 'Save · 2 tracks'") — Multi mode's "Save N" reuses the
/// existing `tag_save_count` in `strings.rs` instead.
pub fn tag_save_scattered(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        TAG_SAVE_SCATTERED,
        TAG_SAVE_SCATTERED_PLURAL,
        count,
        &[("count", &count_text)],
    )
}

// --- F2: in-dialog save progress ---

/// "Saving… 12/30" — the save button's own label while a batch write is in
/// flight (F2, P-2's "spinner im Button").
pub fn tag_saving_progress(done: usize, total: usize) -> String {
    let done_text = done.to_string();
    let total_text = total.to_string();
    formatted(
        N_!("Saving\u{2026} {done}/{total}"),
        &[("done", &done_text), ("total", &total_text)],
    )
}

// --- FB-3: failure toast + details dialog ---

const TAG_SAVE_RESULT: &str = N_!("Tags updated \u{b7} {count} track");
const TAG_SAVE_RESULT_PLURAL: &str = N_!("Tags updated \u{b7} {count} tracks");

/// The no-failures toast text (FB-1: action-less, 4 s, replaceable) — same
/// "tracks" currency as the summary/progress/save label.
pub fn tag_save_result_toast(updated: usize) -> String {
    let count_text = updated.to_string();
    plural(
        TAG_SAVE_RESULT,
        TAG_SAVE_RESULT_PLURAL,
        updated,
        &[("count", &count_text)],
    )
}

const TAG_SAVE_RESULT_WITH_FAILURES: &str =
    N_!("Tags updated \u{b7} {updated} track \u{b7} {failed} failed");
const TAG_SAVE_RESULT_WITH_FAILURES_PLURAL: &str =
    N_!("Tags updated \u{b7} {updated} tracks \u{b7} {failed} failed");

/// FB-3's failure toast text (paired with a "Details" action button and a
/// 10 s unverdrängbar timeout the caller sets directly on the `adw::Toast`,
/// since `toasts::show` only covers the plain-message 4 s case).
pub fn tag_save_result_toast_with_failures(updated: usize, failed: usize) -> String {
    let updated_text = updated.to_string();
    let failed_text = failed.to_string();
    plural(
        TAG_SAVE_RESULT_WITH_FAILURES,
        TAG_SAVE_RESULT_WITH_FAILURES_PLURAL,
        updated,
        &[("updated", &updated_text), ("failed", &failed_text)],
    )
}

pub const TAG_SAVE_FAILURE_DETAILS: &str = N_!("Details");
pub const TAG_SAVE_FAILURE_DIALOG_TITLE: &str = N_!("Some tracks could not be updated");
pub const TAG_EDIT_FAILED_TRACKS: &str = N_!("Edit failed tracks\u{2026}");

// ── Moved verbatim from `strings.rs` (Task F follow-up): the dialog's own
// field labels, mixed/per-track annotations and count helpers belong beside
// the rest of the tag-editor copy, and strings.rs was 24 lines over the
// 800-line architecture gate — which check-architecture.sh enforces and
// check-merge-readiness.sh runs.

pub const MULTIPLE_VALUES: &str = N_!("(multiple values)");
pub const TAG_TITLE: &str = N_!("Title");
pub const TAG_ARTIST: &str = N_!("Artist");
pub const TAG_ALBUM: &str = N_!("Album");
pub const TAG_ALBUM_ARTIST: &str = N_!("Album artist");
pub const TAG_YEAR: &str = N_!("Year");
pub const TAG_TRACK_NUMBER: &str = N_!("Track number");
pub const TAG_GENRE: &str = N_!("Genre");
pub const TAG_NUMBER_ERROR: &str = N_!("Year and track number must be positive whole numbers");
pub const TAG_EDIT_DATABASE_UNAVAILABLE: &str =
    N_!("Could not open the library database for tag editing");
pub const TAG_EDIT_WORKER_FAILED: &str = N_!("Could not start the tag-edit worker");
pub const TAG_SAME_ON_ALL: &str = N_!("same on all");

// --- Tag editor dialog ---

pub const TAG_EDIT_TITLE_SINGLE: &str = N_!("Edit Tags");
pub const TAG_EDIT_TITLE_MULTI: &str = N_!("Edit {count} Tracks");
pub const TAG_PER_TRACK: &str = N_!("per track");
pub const TAG_WILL_APPLY: &str = N_!("will be applied to all {count}");
pub const TAG_SAVE: &str = N_!("Save");
pub const TAG_SAVE_COUNT: &str = N_!("Save {count}");
// Pre-F1 pending-bar header copy ("N changes pending"), superseded by the
// review footer's TAG-5 summary line ("2 fields · 30 tracks affected").
// Kept — strings.rs is append-only — rather than deleted.
#[allow(dead_code)]
pub const TAG_PENDING_CHANGES: &str = N_!("{count} change pending");
#[allow(dead_code)]
pub const TAG_PENDING_CHANGES_PLURAL: &str = N_!("{count} changes pending");
pub const TAG_REVERT: &str = N_!("Revert");
pub const TAG_FETCH_MUSICBRAINZ: &str = N_!("Fetch tags from MusicBrainz");
pub const TAG_FETCH_HINT: &str = N_!("runs per track, fills only empty fields");
pub const TAG_FETCH_LOADING: &str = N_!("Searching MusicBrainz…");
pub const TAG_FETCH_NO_RESULTS: &str = N_!("No matching release found");
pub const TAG_FETCH_NETWORK_ERROR: &str = N_!("Network error — check your connection");
pub const TAG_FETCH_FIELDS_FILLED: &str = N_!("Done — empty fields filled from MusicBrainz");
pub const TAG_FETCH_NOTHING_TO_FILL: &str = N_!("Done — all fields already have values");
pub const TAG_UNSAVED_TITLE: &str = N_!("Save changes?");
pub const TAG_UNSAVED_SAVE: &str = N_!("Save");
pub const TAG_UNSAVED_DISCARD: &str = N_!("Discard");
// Retained for a future cover-write feature: v1 (3a layout, Beschluss #1)
// dropped the "Change cover…" affordance from the tag editor entirely.
#[allow(dead_code)]
pub const TAG_CHANGE_COVER: &str = N_!("Change cover\u{2026}");
// 3a layout (TAG-3/Beschluss #2): header subtitle for Multi mode, and the
// tooltip on Title/Track-number once they're locked read-only there.
pub const TAG_SUBTITLE_MULTI: &str =
    N_!("Only changed fields will be written to all selected tracks");
pub const TAG_PER_TRACK_TOOLTIP: &str = N_!("Per-track field — edit tracks individually");

pub fn tag_edit_title_multi(count: usize) -> String {
    let count_text = count.to_string();
    formatted(TAG_EDIT_TITLE_MULTI, &[("count", &count_text)])
}

pub fn tag_save_count(count: usize) -> String {
    let count_text = count.to_string();
    formatted(TAG_SAVE_COUNT, &[("count", &count_text)])
}

#[allow(dead_code)] // Superseded by Task F1's TAG-5 summary line; see TAG_PENDING_CHANGES.
pub fn tag_pending_count(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        TAG_PENDING_CHANGES,
        TAG_PENDING_CHANGES_PLURAL,
        count,
        &[("count", &count_text)],
    )
}

pub fn tag_will_apply(count: usize) -> String {
    let count_text = count.to_string();
    formatted(TAG_WILL_APPLY, &[("count", &count_text)])
}

pub fn tag_autocomplete_track_count(count: i64) -> String {
    let count = usize::try_from(count).unwrap_or(usize::MAX);
    let count_text = count.to_string();
    plural(
        "{count} track",
        "{count} tracks",
        count,
        &[("count", &count_text)],
    )
}

pub fn tag_cover_count(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} cover",
        "{count} covers",
        count,
        &[("count", &count_text)],
    )
}

// --- TAG-4: browse-snapshot header subtitle position (Task G1) ---

const TAG_TRACK_POSITION: &str = N_!("Track {position} of {total}");

/// The "Track 3 of 12" prefix `tag_editor.rs` prepends to the single-track
/// header subtitle once a browse snapshot exists — position/total come from
/// the frozen snapshot (`snapshot_position`), never a live re-sorted view.
pub fn tag_track_position(position: usize, total: usize) -> String {
    let position_text = position.to_string();
    let total_text = total.to_string();
    formatted(
        TAG_TRACK_POSITION,
        &[("position", &position_text), ("total", &total_text)],
    )
}

// --- TAG-1/Beschluss #3: MusicBrainz lookup in Multi mode (Task G2) ---

pub const TAG_FETCH_REQUIRES_UNIFORM: &str = N_!("Requires same artist & album across selection");
pub const TAG_FETCH_HINT_MULTI: &str = N_!("fills only empty fields");
