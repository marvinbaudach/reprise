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
#[allow(dead_code)] // Wired up by Task F2's save-progress channel.
pub fn tag_saving_progress(done: usize, total: usize) -> String {
    let done_text = done.to_string();
    let total_text = total.to_string();
    formatted(
        N_!("Saving\u{2026} {done}/{total}"),
        &[("done", &done_text), ("total", &total_text)],
    )
}

// --- FB-3: failure toast + details dialog ---

#[allow(dead_code)] // Wired up by Task F2's success toast.
const TAG_SAVE_RESULT: &str = N_!("Tags updated \u{b7} {count} track");
#[allow(dead_code)]
const TAG_SAVE_RESULT_PLURAL: &str = N_!("Tags updated \u{b7} {count} tracks");

/// The no-failures toast text (FB-1: action-less, 4 s, replaceable) — same
/// "tracks" currency as the summary/progress/save label.
#[allow(dead_code)] // Wired up by Task F2's success toast.
pub fn tag_save_result_toast(updated: usize) -> String {
    let count_text = updated.to_string();
    plural(
        TAG_SAVE_RESULT,
        TAG_SAVE_RESULT_PLURAL,
        updated,
        &[("count", &count_text)],
    )
}

#[allow(dead_code)] // Wired up by Task F2's FB-3 failure toast.
const TAG_SAVE_RESULT_WITH_FAILURES: &str =
    N_!("Tags updated \u{b7} {updated} track \u{b7} {failed} failed");
#[allow(dead_code)]
const TAG_SAVE_RESULT_WITH_FAILURES_PLURAL: &str =
    N_!("Tags updated \u{b7} {updated} tracks \u{b7} {failed} failed");

/// FB-3's failure toast text (paired with a "Details" action button and a
/// 10 s unverdrängbar timeout the caller sets directly on the `adw::Toast`,
/// since `toasts::show` only covers the plain-message 4 s case).
#[allow(dead_code)] // Wired up by Task F2's FB-3 failure toast.
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

// Wired up by Task F2's failure-details dialog (`tag_editor_failures.rs`).
#[allow(dead_code)]
pub const TAG_SAVE_FAILURE_DETAILS: &str = N_!("Details");
#[allow(dead_code)]
pub const TAG_SAVE_FAILURE_DIALOG_TITLE: &str = N_!("Some tracks could not be updated");
#[allow(dead_code)]
pub const TAG_EDIT_FAILED_TRACKS: &str = N_!("Edit failed tracks\u{2026}");
