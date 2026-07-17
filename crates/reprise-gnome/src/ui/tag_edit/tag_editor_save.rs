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

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::tag_edit::TrackWrite;
use reprise_core::library::tag_edit_session::TagEditSession;

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

    let confirm_discard = discard_confirmation(&dialog, session, &do_save);
    {
        let confirm_discard = confirm_discard.clone();
        widgets
            .cancel_button
            .connect_clicked(move |_| confirm_discard());
    }
    wire_escape(&dialog, session, &confirm_discard);
    wire_navigation(widgets.previous_button, widgets.next_button, on_navigate);

    dialog.set_default_widget(Some(&save_button));
    widgets.year.set_activates_default(true);
    widgets.track_number.set_activates_default(true);
}

fn wire_save_shortcut(dialog: &adw::Dialog, save_button: &gtk4::Button, save: &Rc<dyn Fn()>) {
    let save = save.clone();
    let save_button = save_button.clone();
    let controller = gtk4::EventControllerKey::new();
    controller.connect_key_pressed(move |_, key, _, modifier| {
        if key == gdk::Key::s
            && modifier.contains(gdk::ModifierType::CONTROL_MASK)
            && save_button.is_sensitive()
        {
            save();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    dialog.add_controller(controller);
}

fn discard_confirmation(
    dialog: &adw::Dialog,
    session: &Rc<RefCell<TagEditSession>>,
    save: &Rc<dyn Fn()>,
) -> Rc<dyn Fn()> {
    let session = session.clone();
    let dialog = dialog.clone();
    let save = save.clone();
    Rc::new(move || {
        if session.borrow().pending_track_count() == 0 {
            dialog.close();
            return;
        }
        let alert = adw::AlertDialog::builder()
            .heading(strings::text(strings::TAG_UNSAVED_TITLE))
            .build();
        alert.add_response("cancel", &strings::text(strings::CANCEL));
        alert.add_response("discard", &strings::text(strings::TAG_UNSAVED_DISCARD));
        alert.add_response("save", &strings::text(strings::TAG_UNSAVED_SAVE));
        alert.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        alert.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        alert.set_default_response(Some("save"));
        alert.set_close_response("cancel");

        let dialog_for_response = dialog.clone();
        let save = save.clone();
        alert.connect_response(None, move |_, response| match response {
            "save" => save(),
            "discard" => {
                dialog_for_response.close();
            }
            _ => {}
        });
        alert.present(Some(&dialog));
    })
}

fn wire_escape(
    dialog: &adw::Dialog,
    session: &Rc<RefCell<TagEditSession>>,
    confirm_discard: &Rc<dyn Fn()>,
) {
    let session = session.clone();
    let confirm_discard = confirm_discard.clone();
    let controller = gtk4::EventControllerKey::new();
    controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    controller.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Escape && session.borrow().pending_track_count() > 0 {
            confirm_discard();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
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
