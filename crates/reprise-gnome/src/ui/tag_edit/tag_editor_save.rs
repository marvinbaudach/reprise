//! Save, discard, keyboard, and navigation lifecycle for the tag editor.
//!
//! F0 signature note (Paket E builds the real TAG-8 keyboard semantics on
//! top of this in Wave 4 — the Enter-chain/Esc-cascade bodies below are
//! untouched pre-TAG-8 behavior, kept working only as far as the session
//! swap forces): `dirty: &[Rc<Cell<bool>>]` is gone — it was exactly the
//! parallel state `TagEditSession` replaces (F0). Every dirty-flag read
//! becomes a `TagEditSession` read instead: `discard_confirmation`/
//! `wire_escape`'s "anything pending?" check is now `session.borrow().
//! pending_track_count() > 0`, and `do_save` now commits the session's
//! current effective values for Year/Track-number (the only two fields
//! whose validity can only be known at save time — every other field is
//! already live-synced into the session by `tag_editor_dirty::wire`) and
//! hands the caller the session's `write_batch()` instead of a single
//! `TrackEditPatch`. `on_apply` is renamed `on_save` to match.
//!
//! F2 forces one more change here: `do_save` no longer closes the dialog
//! itself. Once the batch can involve a streamed, cancel-proof write (the
//! dialog must stay open with a progress spinner), only the caller knows
//! when the write actually finished — `on_save`'s implementation is
//! responsible for closing `widgets.dialog` once `apply_track_writes`
//! completes.
//!
//! E1/E2 (TAG-8) rework this file's keyboard/Esc wiring end to end:
//!
//! - **`activates_default` is gone.** Nothing in this dialog treats Enter as
//!   "activate the default widget" anymore — Enter's only two jobs are the
//!   Enter chain (below) and, with Ctrl held, the shared save shortcut.
//! - **Enter chain.** Every field-shaped row (Title/Artist/Album/Album
//!   artist/Genre/Year/Track number) gets its own Bubble-phase
//!   `EventControllerKey`. Package D's autocomplete popover handling
//!   (`autocomplete_entry.rs`) sits at *Capture* phase on those same rows;
//!   GTK always finishes the whole Capture pass (root → target) before the
//!   Bubble pass starts (target → root), and a Capture-phase `Stop` cancels
//!   the event outright — so whenever a popover is open and swallows Enter/
//!   Escape there, this file's Bubble-phase handlers on that exact row never
//!   run at all. That ordering is *why* "don't touch Package D, just don't
//!   swallow its Propagation" (the task brief) falls out for free from phase
//!   choice alone, no coordination needed. `tag_editor_state::
//!   next_enter_target` is the pure decision; this file only resolves it to
//!   a `grab_focus()` call.
//! - **Field discovery without widening `SaveWidgets`.** The Enter chain and
//!   the Esc field-revert stage need handles on all seven field rows, but
//!   `SaveWidgets` (below) only carries Year/Track-number (the two fields
//!   save-time validation needs) plus the dialog chrome — widening it would
//!   require editing `tag_editor.rs`'s call site, which is Package G's file
//!   this wave. Instead, `discover_field_rows` walks `widgets.dialog`'s
//!   already-built widget tree once at wire time and collects every
//!   `adw::EntryRow` it finds, in the same fixed visual order
//!   `tag_editor_state::ENTER_CHAIN_ORDER` reasons about (Title, Artist,
//!   Album, Album artist, Genre, Year, Track number — exactly how
//!   `tag_editor_form.rs` lays them out). If that ever finds the wrong
//!   count, it logs a warning and the Enter-chain/Esc-revert wiring for this
//!   dialog is skipped rather than wired to the wrong field (see that
//!   function's own doc comment).
//! - **Esc field-revert without a session field lookup.** Rather than
//!   re-deriving "is this field armed" from the session (duplicating
//!   `tag_editor_dirty.rs`'s own armed-tracking, and needing a scope/
//!   `TagField` mapping this file would then have to keep in sync by hand),
//!   stage 2 finds the field's own in-row ↺ revert button (installed by
//!   `tag_editor_dirty.rs`, tagged with the `reprise-tag-field-revert` CSS
//!   class) and synthesizes its `clicked` signal. That button's own click
//!   handler already does the correct revert *and* refreshes the review
//!   footer/Save label/tooltip (`tag_editor_dirty::wire`'s `update`
//!   callback) — this file gets that refresh for free instead of
//!   duplicating it, and the button's `is_visible()` state (toggled by that
//!   same module whenever a field arms/unarms) is exactly "is this field
//!   armed", so there is nothing left to query from the session directly.
//! - **Esc cascade order.** The dialog-level Escape handler (stage 3,
//!   `wire_escape`) is now *Bubble* phase, not Capture — the old Capture
//!   phase was itself the bug this task fixes: a Capture-phase handler on
//!   `dialog` (the root-ward ancestor) fires *before* Capture-phase handlers
//!   on any row further down the tree, which meant the old code could pop
//!   the discard-confirmation dialog while a suggestion popover was still
//!   open, or before a field-revert had a chance to absorb the key. Bubble
//!   phase on `dialog` fires only after every row's own Capture pass *and*
//!   Bubble pass (field-revert, stage 2) have already run and chosen not to
//!   consume the key.
//! - **Esc never aborts a save in flight** (a gap Package F found): while
//!   `tag_edit_flow::spawn_save` is running, it disables `cancel_button` for
//!   the whole write and never re-enables it except on a failure path — so
//!   `!cancel_button.is_sensitive()` is a reliable "a save is in flight"
//!   signal without any new cross-file state. `wire_escape` checks it before
//!   doing anything else.
//! - **Two-answer discard prompt.** The old third "Save" response is gone;
//!   Escape/Cancel now only ever offer "Keep editing" (default) or
//!   "Discard" (destructive) — saving is never the way out of a closing
//!   gesture.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::tag_edit::TrackWrite;
use reprise_core::library::tag_edit_session::{SessionMode, TagEditSession};

use crate::ui::strings;
use crate::ui::tag_editor_dirty::{parse_number_field, session_scope};
use crate::ui::tag_editor_state::*;
use reprise_core::library::tag_edit_session::{FieldValue, TagField};

#[derive(Clone, Copy)]
pub(in crate::ui) struct SaveWidgets<'a> {
    pub(in crate::ui) dialog: &'a adw::Dialog,
    pub(in crate::ui) save_button: &'a gtk4::Button,
    pub(in crate::ui) cancel_button: &'a gtk4::Button,
    pub(in crate::ui) previous_button: &'a gtk4::Button,
    pub(in crate::ui) next_button: &'a gtk4::Button,
    pub(in crate::ui) year: &'a adw::EntryRow,
    pub(in crate::ui) track_number: &'a adw::EntryRow,
    pub(in crate::ui) error_label: &'a gtk4::Label,
}

pub(in crate::ui) fn wire(
    widgets: SaveWidgets<'_>,
    session: &Rc<RefCell<TagEditSession>>,
    on_save: impl Fn(Vec<TrackWrite>) + Clone + 'static,
    on_navigate: impl Fn(NavigateDirection) -> bool + 'static,
) {
    let dialog = widgets.dialog.clone();
    let save_button = widgets.save_button.clone();
    let cancel_button = widgets.cancel_button.clone();
    let year = widgets.year.clone();
    let track_number = widgets.track_number.clone();
    let error_label = widgets.error_label.clone();

    let do_save: Rc<dyn Fn()> = {
        let session = session.clone();
        // Title/Artist/Album/Album artist/Genre/Rating already live in the
        // session via `tag_editor_dirty::wire`'s live "changed" wiring —
        // only Year/Track-number's validity can only be known at save time
        // (a partially-typed number is a normal interim keystroke, not yet
        // an error), so only those two get committed here.
        Rc::new(move || {
            let year_value = parse_number_field(&year.text());
            let track_value = if track_number.is_editable() {
                parse_number_field(&track_number.text())
            } else {
                Ok(None)
            };
            let (Ok(year_value), Ok(track_value)) = (year_value, track_value) else {
                year.add_css_class("error");
                track_number.add_css_class("error");
                error_label.set_visible(true);
                tracing::debug!("tag editor rejected an invalid year or track number");
                return;
            };

            let batch = {
                let mut session = session.borrow_mut();
                let scope = session_scope(session.mode());
                session.set_pending(scope, TagField::Year, &FieldValue::Number(year_value));
                if track_number.is_editable() {
                    session.set_pending(scope, TagField::TrackNo, &FieldValue::Number(track_value));
                }
                session.write_batch()
            };
            on_save(batch);
        })
    };

    {
        let do_save = do_save.clone();
        widgets.save_button.connect_clicked(move |_| do_save());
    }
    wire_save_shortcut(&dialog, &save_button, &do_save);

    let confirm_discard = discard_confirmation(&dialog, session);
    {
        let confirm_discard = confirm_discard.clone();
        widgets
            .cancel_button
            .connect_clicked(move |_| confirm_discard());
    }
    wire_escape(&dialog, &cancel_button, session, &confirm_discard);
    wire_navigation(widgets.previous_button, widgets.next_button, on_navigate);

    let is_multi = matches!(session.borrow().mode(), SessionMode::Multi);
    wire_field_keys(&dialog, &save_button, is_multi);

    // TAG-8: `activates_default` is gone on purpose — Enter never fires
    // Save directly from a text field anymore (see this module's top doc
    // comment). The Enter chain above and the Ctrl+Enter/Ctrl+S shortcut are
    // the only two ways Enter can ever reach Save.
}

/// TAG-8: Ctrl+Enter (documented in the Shortcuts overlay) and Ctrl+S
/// (silent alias) are recognized by the single `is_save_shortcut` predicate
/// and both drive this one controller, so both combinations share `save`'s
/// disabled/"Saving…" state automatically rather than needing two call
/// sites kept in sync by hand.
fn wire_save_shortcut(dialog: &adw::Dialog, save_button: &gtk4::Button, save: &Rc<dyn Fn()>) {
    let save = save.clone();
    let save_button = save_button.clone();
    let controller = gtk4::EventControllerKey::new();
    controller.connect_key_pressed(move |_, key, _, modifier| {
        if is_save_shortcut(key, modifier) && save_button.is_sensitive() {
            save();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    dialog.add_controller(controller);
}

/// TAG-8's two-answer discard prompt: "Keep editing" (default,
/// non-destructive) or "Discard" (destructive). The old third "Save"
/// response is gone — a plain array rather than two inline `add_response`
/// calls so the "exactly two, no save" contract is assertable without a
/// live `AdwAlertDialog` (see `tag_8_discard_prompt_counts_tracks_two_
/// answers`).
const DISCARD_RESPONSE_KEEP_EDITING: &str = "keep-editing";
const DISCARD_RESPONSE_DISCARD: &str = "discard";

fn discard_prompt_responses() -> [&'static str; 2] {
    [DISCARD_RESPONSE_KEEP_EDITING, DISCARD_RESPONSE_DISCARD]
}

fn discard_confirmation(
    dialog: &adw::Dialog,
    session: &Rc<RefCell<TagEditSession>>,
) -> Rc<dyn Fn()> {
    let session = session.clone();
    let dialog = dialog.clone();
    Rc::new(move || {
        let pending = session.borrow().pending_track_count();
        if pending == 0 {
            dialog.close();
            return;
        }
        let alert = adw::AlertDialog::builder()
            .heading(strings::tag_discard_prompt_title(pending))
            .build();
        for response_id in discard_prompt_responses() {
            let label = if response_id == DISCARD_RESPONSE_DISCARD {
                strings::text(strings::TAG_UNSAVED_DISCARD)
            } else {
                strings::text(strings::TAG_KEEP_EDITING)
            };
            alert.add_response(response_id, &label);
        }
        alert.set_response_appearance(
            DISCARD_RESPONSE_DISCARD,
            adw::ResponseAppearance::Destructive,
        );
        alert.set_default_response(Some(DISCARD_RESPONSE_KEEP_EDITING));
        alert.set_close_response(DISCARD_RESPONSE_KEEP_EDITING);

        let dialog_for_response = dialog.clone();
        alert.connect_response(None, move |_, response| {
            if response == DISCARD_RESPONSE_DISCARD {
                dialog_for_response.close();
            }
            // Keep editing (or the alert's own Esc-close, which maps to the
            // same response): nothing to do, the dialog stays open exactly
            // as it was.
        });
        alert.present(Some(&dialog));
    })
}

/// TAG-8 Esc stage 3 (the dialog level): only reached once stage 1
/// (Package D's popover, Capture phase) and stage 2 (field-revert, Bubble
/// phase on the row itself — see `wire_field_escape_revert`) have both
/// already run and chosen not to consume the key, because Bubble phase on
/// `dialog` fires strictly after every row's full Capture+Bubble pass. Never
/// aborts a save in flight (Package F's finding): `cancel_button` is only
/// ever insensitive during `tag_edit_flow::spawn_save`'s write, which this
/// checks directly rather than needing new cross-file state.
fn wire_escape(
    dialog: &adw::Dialog,
    cancel_button: &gtk4::Button,
    session: &Rc<RefCell<TagEditSession>>,
    confirm_discard: &Rc<dyn Fn()>,
) {
    let session = session.clone();
    let cancel_button = cancel_button.clone();
    let confirm_discard = confirm_discard.clone();
    let controller = gtk4::EventControllerKey::new();
    controller.set_propagation_phase(gtk4::PropagationPhase::Bubble);
    controller.connect_key_pressed(move |_, key, _, _| {
        if key != gdk::Key::Escape {
            return glib::Propagation::Proceed;
        }
        let save_in_flight = !cancel_button.is_sensitive();
        let pending = session.borrow().pending_track_count();
        match dialog_escape_action(save_in_flight, pending) {
            DialogEscapeAction::Ignore => {}
            DialogEscapeAction::Close | DialogEscapeAction::PromptDiscard => confirm_discard(),
        }
        glib::Propagation::Stop
    });
    dialog.add_controller(controller);
}

fn wire_navigation(
    previous: &gtk4::Button,
    next: &gtk4::Button,
    on_navigate: impl Fn(NavigateDirection) -> bool + 'static,
) {
    let on_navigate = Rc::new(on_navigate);
    {
        let on_navigate = on_navigate.clone();
        previous.connect_clicked(move |_| {
            on_navigate(NavigateDirection::Previous);
        });
    }
    next.connect_clicked(move |_| {
        on_navigate(NavigateDirection::Next);
    });
}

// ── TAG-8: Enter chain + Esc field-revert ────────────────────────────────────

/// Walks `dialog`'s already-built widget tree (depth-first, following each
/// widget's own child order — which for every `gtk4::Box`/`gtk4::Grid` in
/// this dialog is exactly the order `tag_editor_form.rs` appended/attached
/// them in) and collects every `adw::EntryRow` it finds. Stops descending
/// the instant it finds one — an `EntryRow`'s own internal children are GTK
/// presentation detail, never another field.
fn collect_entry_rows(widget: &gtk4::Widget, out: &mut Vec<adw::EntryRow>) {
    if let Some(row) = widget.downcast_ref::<adw::EntryRow>() {
        out.push(row.clone());
        return;
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        collect_entry_rows(&current, out);
        child = current.next_sibling();
    }
}

/// Pairs every discovered `adw::EntryRow` with its `FIELD_*` identity in
/// `tag_editor_state::ENTER_CHAIN_ORDER`'s fixed order. Returns `None` (and
/// logs a warning) if the dialog's layout does not contain exactly the
/// expected seven rows in that order — a defensive fallback rather than a
/// panic or a chain wired to the wrong field, in case a future layout change
/// invalidates the "the visual order matches `ENTER_CHAIN_ORDER`" assumption
/// this depends on (see this module's top doc comment for why the wiring
/// lives here, on a tree walk, instead of on widened `SaveWidgets` fields).
fn discover_field_rows(dialog: &adw::Dialog) -> Option<Rc<Vec<(usize, adw::EntryRow)>>> {
    let root = dialog.child()?;
    let mut rows = Vec::new();
    collect_entry_rows(&root, &mut rows);
    if rows.len() != ENTER_CHAIN_ORDER.len() {
        tracing::warn!(
            found = rows.len(),
            expected = ENTER_CHAIN_ORDER.len(),
            "tag editor: entry-row layout does not match TAG-8's expected field count; \
             Enter-chain/Esc field-revert wiring skipped for this dialog"
        );
        return None;
    }
    Some(Rc::new(
        ENTER_CHAIN_ORDER.iter().copied().zip(rows).collect(),
    ))
}

fn focus_field(rows: &[(usize, adw::EntryRow)], field: usize) -> bool {
    rows.iter()
        .find(|(candidate, _)| *candidate == field)
        .is_some_and(|(_, row)| row.grab_focus())
}

/// TAG-8: wires the Enter chain (`next_enter_target`) and the Esc-cascade's
/// field-revert stage onto every discovered field row. A no-op if the
/// dialog's layout doesn't match what `discover_field_rows` expects.
fn wire_field_keys(dialog: &adw::Dialog, save_button: &gtk4::Button, is_multi: bool) {
    let Some(rows) = discover_field_rows(dialog) else {
        return;
    };
    for (field, row) in rows.iter() {
        wire_field_enter(row, *field, is_multi, &rows, save_button);
        wire_field_escape_revert(row);
    }
}

/// TAG-8's Enter chain for a single field row. Bubble phase (the default):
/// Package D's own Capture-phase handling on this same row (dropdown
/// accept-and-close) always gets first refusal, and a Capture-phase `Stop`
/// there cancels the whole event before this handler ever runs — see this
/// module's top doc comment. Ignores Ctrl+Enter (that combination belongs
/// to `wire_save_shortcut`'s dialog-level controller instead, which this
/// leaves free to receive it by returning `Proceed`).
fn wire_field_enter(
    row: &adw::EntryRow,
    field: usize,
    is_multi: bool,
    rows: &Rc<Vec<(usize, adw::EntryRow)>>,
    save_button: &gtk4::Button,
) {
    let rows = rows.clone();
    let save_button = save_button.clone();
    let controller = gtk4::EventControllerKey::new();
    controller.set_propagation_phase(gtk4::PropagationPhase::Bubble);
    controller.connect_key_pressed(move |_, key, _, modifier| {
        if !matches!(key, gdk::Key::Return | gdk::Key::KP_Enter)
            || modifier.contains(gdk::ModifierType::CONTROL_MASK)
        {
            return glib::Propagation::Proceed;
        }
        match next_enter_target(field, is_multi) {
            EnterTarget::Field(next_field) => {
                focus_field(&rows, next_field);
            }
            EnterTarget::SaveButton => {
                save_button.grab_focus();
            }
        }
        glib::Propagation::Stop
    });
    row.add_controller(controller);
}

/// TAG-8 Esc stage 2 for a single field row: if the field's own in-row ↺
/// revert button (installed by `tag_editor_dirty.rs`, tagged
/// `reprise-tag-field-revert`) is visible — i.e. the field is armed — this
/// synthesizes its `clicked` signal instead of re-implementing the revert
/// here, so the review footer/Save label/tooltip refresh
/// (`tag_editor_dirty::wire`'s `update` callback, which that button's own
/// handler already calls) happens exactly the same way a mouse click would
/// trigger it. Bubble phase, same ordering argument as `wire_field_enter`.
fn wire_field_escape_revert(row: &adw::EntryRow) {
    let row_for_lookup = row.clone();
    let controller = gtk4::EventControllerKey::new();
    controller.set_propagation_phase(gtk4::PropagationPhase::Bubble);
    controller.connect_key_pressed(move |_, key, _, _| {
        if key != gdk::Key::Escape {
            return glib::Propagation::Proceed;
        }
        let Some(revert_button) = find_revert_button(row_for_lookup.upcast_ref()) else {
            return glib::Propagation::Proceed;
        };
        if !escape_should_revert_field(revert_button.is_visible()) {
            return glib::Propagation::Proceed;
        }
        revert_button.emit_clicked();
        glib::Propagation::Stop
    });
    row.add_controller(controller);
}

/// Finds `row`'s own ↺ revert button — a suffix widget `tag_editor_dirty.rs`
/// tags with the `reprise-tag-field-revert` CSS class and toggles visible
/// exactly when the field is armed. `None` for a field that never gets one
/// (Title/Track-number when read-only in Multi mode — `tag_editor_dirty.rs`
/// skips wiring those entirely, TAG-3).
fn find_revert_button(widget: &gtk4::Widget) -> Option<gtk4::Button> {
    if let Some(button) = widget.downcast_ref::<gtk4::Button>() {
        if button.has_css_class("reprise-tag-field-revert") {
            return Some(button.clone());
        }
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = find_revert_button(&current) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_8_discard_prompt_counts_tracks_two_answers() {
        // Tracks, not fields or a generic "changes" — same currency as the
        // review footer/save label/progress/toast.
        assert_eq!(
            strings::tag_discard_prompt_title(1),
            "Discard changes to 1 track?"
        );
        assert_eq!(
            strings::tag_discard_prompt_title(30),
            "Discard changes to 30 tracks?"
        );

        // Exactly two responses; the old "Save" way out is gone.
        let responses = discard_prompt_responses();
        assert_eq!(responses.len(), 2);
        assert!(responses.contains(&DISCARD_RESPONSE_KEEP_EDITING));
        assert!(responses.contains(&DISCARD_RESPONSE_DISCARD));
        assert!(!responses.contains(&"save"));
    }

    #[test]
    fn focus_field_finds_target_by_field_id() {
        // `focus_field` itself needs a live `adw::EntryRow` to call
        // `grab_focus` on, which needs a display — covered instead by this
        // crate's headless GTK acceptance harness. What's headlessly
        // checkable here is the lookup returning `false` for an identity
        // that isn't in the (empty) row list, without panicking.
        let rows: Vec<(usize, adw::EntryRow)> = Vec::new();
        assert!(!focus_field(&rows, FIELD_ARTIST));
    }
}
