//! Pure patch parsing, navigation, and field identity for the tag editor.
//!
//! E1/E2 (TAG-8) add this file's keyboard-semantics decisions: which field
//! Enter should focus next (or whether it should focus Save instead), which
//! key combinations count as the single shared save shortcut, and which
//! stage of the Esc cascade a keypress belongs to. All three are pure and
//! unit-tested here; the GTK wiring that calls them (event controllers,
//! `grab_focus`, widget discovery) lives in `tag_editor_save.rs`, since GTK
//! focus/keypress delivery itself is barely testable headless (see the
//! `building-gtk4-rust-apps` skill).

use gtk4::gdk;

use crate::ui::strings;

// ── Pure-logic helpers (unchanged from v1, exercised by the tests below) ─────

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("expected a positive whole number")]
pub struct ParseFieldError;

pub(in crate::ui) const RATING_MAX: i32 = 5;

// F0 note (not this package's ownership, touched only to keep the build
// green): orphaned by the `Cell<bool>` dirty-array's removal — every text
// field now lives in `TagEditSession` and is read via `set_pending`/
// `write_batch`, never patched from a raw `(dirty, text)` pair. Left in
// place rather than deleted for Package E's Wave-4 call on whether its
// TAG-8 keyboard work still wants it.
#[allow(dead_code)]
pub(crate) fn string_patch(dirty: bool, text: &str) -> Option<String> {
    dirty.then(|| text.to_string())
}

pub(crate) fn number_patch(
    dirty: bool,
    text: &str,
) -> Result<Option<Option<u32>>, ParseFieldError> {
    if !dirty {
        return Ok(None);
    }
    let text = text.trim();
    if text.is_empty() {
        return Ok(Some(None));
    }
    let value = text.parse::<u32>().map_err(|_| ParseFieldError)?;
    if value == 0 {
        return Err(ParseFieldError);
    }
    Ok(Some(Some(value)))
}

// ── Navigation direction ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigateDirection {
    Previous,
    Next,
}

// ── Field identity for dirty tracking ────────────────────────────────────────

/// Indices into the old `dirty` flags vector (F0 removed it in favor of
/// `TagEditSession` + `TagField`). E1 answers Package F's "still wants
/// index-based field identity?" call for seven of these: they're now the
/// identity space `ENTER_CHAIN_ORDER`/`next_enter_target` reason over below,
/// so those seven lose their `#[allow(dead_code)]` — this file's own
/// non-test code uses them, not just `tag_editor_tests.rs` (Package G's
/// test module for `tag_editor.rs`, which still exercises `field_name`/
/// `FIELD_COUNT`/`string_patch` and therefore keeps those three alive; this
/// package does not own that test file and cannot delete what it depends
/// on).
pub(in crate::ui) const FIELD_TITLE: usize = 0;
pub(in crate::ui) const FIELD_ARTIST: usize = 1;
pub(in crate::ui) const FIELD_ALBUM: usize = 2;
pub(in crate::ui) const FIELD_ALBUM_ARTIST: usize = 3;
pub(in crate::ui) const FIELD_YEAR: usize = 4;
pub(in crate::ui) const FIELD_TRACK_NO: usize = 5;
pub(in crate::ui) const FIELD_GENRE: usize = 6;
#[allow(dead_code)]
pub(in crate::ui) const FIELD_RATING: usize = 7;
#[allow(dead_code)]
pub(in crate::ui) const FIELD_COUNT: usize = 8;

/// Human-readable names for the old pending-change bar, indexed by
/// `FIELD_*` — orphaned by the same F0 removal as the constants above.
#[allow(dead_code)]
pub(in crate::ui) fn field_name(index: usize) -> String {
    use strings::*;
    match index {
        FIELD_TITLE => text(TAG_TITLE),
        FIELD_ARTIST => text(TAG_ARTIST),
        FIELD_ALBUM => text(TAG_ALBUM),
        FIELD_ALBUM_ARTIST => text(TAG_ALBUM_ARTIST),
        FIELD_YEAR => text(TAG_YEAR),
        FIELD_TRACK_NO => text(TAG_TRACK_NUMBER),
        FIELD_GENRE => text(TAG_GENRE),
        FIELD_RATING => text(RATING),
        _ => String::new(),
    }
}

// ── TAG-8: Enter-chain ───────────────────────────────────────────────────────

/// Fixed visual order the Enter chain walks: Title, Artist, Album, Album
/// artist, Genre, Year, Track number — top-to-bottom, left-to-right through
/// the 3a layout's header fields then its two-column grid. Rating is
/// deliberately absent: it is a click-only star control (TAG-2's rule 6),
/// never a Tab/Enter-reachable text field.
pub(in crate::ui) const ENTER_CHAIN_ORDER: [usize; 7] = [
    FIELD_TITLE,
    FIELD_ARTIST,
    FIELD_ALBUM,
    FIELD_ALBUM_ARTIST,
    FIELD_GENRE,
    FIELD_YEAR,
    FIELD_TRACK_NO,
];

/// TAG-8's Enter-chain destination: either the next editable field in fixed
/// order, or the Save button once the chain runs out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum EnterTarget {
    Field(usize),
    SaveButton,
}

/// TAG-3: Title and Track-number are locked read-only in Multi mode, so the
/// Enter chain must never land focus on them there.
fn is_enter_chain_editable(field: usize, is_multi: bool) -> bool {
    !is_multi || (field != FIELD_TITLE && field != FIELD_TRACK_NO)
}

/// TAG-8: from `current` (Enter pressed with the field's own dropdown, if
/// any, already closed — Package D's popover-open Enter is a separate,
/// earlier-consumed case this function never sees), the next editable field
/// in the fixed visual order, skipping read-only fields (TAG-3); once the
/// chain runs out, the Save button — "sichtbar; der nächste ↵ speichert
/// bewusst" (TAG-8), never an immediate save from the field itself.
pub(in crate::ui) fn next_enter_target(current: usize, is_multi: bool) -> EnterTarget {
    let start = ENTER_CHAIN_ORDER
        .iter()
        .position(|&field| field == current)
        .map_or(0, |index| index + 1);
    ENTER_CHAIN_ORDER[start..]
        .iter()
        .copied()
        .find(|&field| is_enter_chain_editable(field, is_multi))
        .map_or(EnterTarget::SaveButton, EnterTarget::Field)
}

// ── TAG-8: shared Ctrl+Enter / Ctrl+S save shortcut ──────────────────────────

/// TAG-8: Ctrl+Enter (documented, Shortcuts overlay) and Ctrl+S (silent
/// alias) are both recognized by this single predicate — the caller wires
/// exactly one key controller against it, so both combinations drive the
/// same save action and therefore share its disabled/"Saving…" state
/// automatically, rather than needing two call sites kept in sync by hand.
pub(in crate::ui) fn is_save_shortcut(key: gdk::Key, modifier: gdk::ModifierType) -> bool {
    modifier.contains(gdk::ModifierType::CONTROL_MASK)
        && matches!(key, gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::s)
}

// ── TAG-8: Esc cascade ───────────────────────────────────────────────────────

/// TAG-8 Esc stage 2: the currently focused field absorbs Escape as a
/// field-revert whenever it has a pending diff (Package D's popover-open
/// Escape, stage 1, is a separate, earlier-consumed case — see
/// `tag_editor_save.rs`'s module doc for the propagation-phase argument for
/// why stage 1 always gets first refusal).
pub(in crate::ui) fn escape_should_revert_field(field_is_armed: bool) -> bool {
    field_is_armed
}

/// TAG-8 Esc stage 3: what the dialog itself does with an Escape that
/// neither the popover nor a field revert absorbed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum DialogEscapeAction {
    /// A save is already in flight (Package F's finding): the batch is
    /// atomic from the user's perspective, so Escape is swallowed with no
    /// effect rather than risking an aborted write.
    Ignore,
    /// Nothing pending: close outright, no prompt needed.
    Close,
    /// Something is pending: ask before discarding it.
    PromptDiscard,
}

/// TAG-8's outermost Esc-cascade decision, once stages 1–2 have had their
/// chance and did not consume the key.
pub(in crate::ui) fn dialog_escape_action(
    save_in_flight: bool,
    pending_track_count: usize,
) -> DialogEscapeAction {
    if save_in_flight {
        DialogEscapeAction::Ignore
    } else if pending_track_count > 0 {
        DialogEscapeAction::PromptDiscard
    } else {
        DialogEscapeAction::Close
    }
}

#[cfg(test)]
mod tag8_tests {
    use super::*;

    #[test]
    fn tag_8_enter_never_saves_from_text_field() {
        // Plain Enter (no modifier) is never the save shortcut...
        assert!(!is_save_shortcut(
            gdk::Key::Return,
            gdk::ModifierType::empty()
        ));
        assert!(!is_save_shortcut(
            gdk::Key::KP_Enter,
            gdk::ModifierType::empty()
        ));
        // ...it only ever resolves to a focus target (the next field, or
        // the Save button to focus, never a fired save) — every field in
        // the chain, in both modes.
        for &field in ENTER_CHAIN_ORDER.iter() {
            for is_multi in [false, true] {
                match next_enter_target(field, is_multi) {
                    EnterTarget::Field(_) | EnterTarget::SaveButton => {}
                }
            }
        }
    }

    #[test]
    fn tag_8_enter_skips_readonly_fields() {
        // Multi mode: Title and Track-number are per-track/read-only
        // (TAG-3) and must never receive focus via the Enter chain.
        assert_eq!(
            next_enter_target(FIELD_TITLE, true),
            EnterTarget::Field(FIELD_ARTIST)
        );
        // Genre -> Year (unaffected), but Year -> Track-number is skipped
        // in Multi mode, landing on Save instead.
        assert_eq!(
            next_enter_target(FIELD_GENRE, true),
            EnterTarget::Field(FIELD_YEAR)
        );
        assert_eq!(next_enter_target(FIELD_YEAR, true), EnterTarget::SaveButton);
        // Single-track mode: nothing is skipped.
        assert_eq!(
            next_enter_target(FIELD_TITLE, false),
            EnterTarget::Field(FIELD_ARTIST)
        );
        assert_eq!(
            next_enter_target(FIELD_YEAR, false),
            EnterTarget::Field(FIELD_TRACK_NO)
        );
    }

    #[test]
    fn tag_8_last_field_enter_focuses_save() {
        // Single-track mode: Track-number is editable and last in the
        // chain.
        assert_eq!(
            next_enter_target(FIELD_TRACK_NO, false),
            EnterTarget::SaveButton
        );
    }

    #[test]
    fn tag_8_ctrl_enter_and_ctrl_s_share_one_action() {
        let ctrl = gdk::ModifierType::CONTROL_MASK;
        assert!(is_save_shortcut(gdk::Key::Return, ctrl));
        assert!(is_save_shortcut(gdk::Key::KP_Enter, ctrl));
        assert!(is_save_shortcut(gdk::Key::s, ctrl));
        // Neither fires without Control held — that's the plain Enter-chain
        // case above, not the save shortcut.
        assert!(!is_save_shortcut(
            gdk::Key::Return,
            gdk::ModifierType::empty()
        ));
        assert!(!is_save_shortcut(gdk::Key::s, gdk::ModifierType::empty()));
    }

    #[test]
    fn tag_8_esc_cascade_dropdown_then_revert_then_discard() {
        // Stage 1 (Package D, autocomplete_entry.rs — not re-tested here):
        // while a suggestion popover is open, its own Capture-phase Escape
        // handler closes only the popover and stops propagation, so this
        // module's Bubble-phase stages never even run for that keypress.

        // Stage 2: an armed field absorbs Escape as a revert...
        assert!(escape_should_revert_field(true));
        // ...an unarmed one lets it fall through to the dialog.
        assert!(!escape_should_revert_field(false));

        // Stage 3: nothing pending anywhere -> close outright,
        assert_eq!(dialog_escape_action(false, 0), DialogEscapeAction::Close);
        // something pending -> ask first,
        assert_eq!(
            dialog_escape_action(false, 3),
            DialogEscapeAction::PromptDiscard
        );
        // and never abort a save already in flight (Package F's finding),
        // regardless of how much is pending.
        assert_eq!(dialog_escape_action(true, 3), DialogEscapeAction::Ignore);
    }
}
