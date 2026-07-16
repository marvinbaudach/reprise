//! Save, discard, keyboard, and navigation lifecycle for the tag editor.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::tag_edit::{TagPatch, TrackEditPatch};

use crate::ui::autocomplete_entry::AutocompleteEntry;
use crate::ui::strings;
use crate::ui::tag_editor_state::*;

#[derive(Clone, Copy)]
pub(in crate::ui) struct SaveWidgets<'a> {
    pub(in crate::ui) dialog: &'a adw::Dialog,
    pub(in crate::ui) save_button: &'a gtk4::Button,
    pub(in crate::ui) cancel_button: &'a gtk4::Button,
    pub(in crate::ui) previous_button: &'a gtk4::Button,
    pub(in crate::ui) next_button: &'a gtk4::Button,
    pub(in crate::ui) title: &'a adw::EntryRow,
    pub(in crate::ui) artist: &'a Rc<AutocompleteEntry>,
    pub(in crate::ui) album: &'a Rc<AutocompleteEntry>,
    pub(in crate::ui) album_artist: &'a Rc<AutocompleteEntry>,
    pub(in crate::ui) genre: &'a Rc<AutocompleteEntry>,
    pub(in crate::ui) year: &'a adw::EntryRow,
    pub(in crate::ui) track_number: &'a adw::EntryRow,
    pub(in crate::ui) rating: &'a Cell<i32>,
    pub(in crate::ui) error_label: &'a gtk4::Label,
}

pub(in crate::ui) fn wire(
    widgets: SaveWidgets<'_>,
    dirty: &[Rc<Cell<bool>>],
    on_apply: impl Fn(TrackEditPatch) + Clone + 'static,
    on_navigate: impl Fn(NavigateDirection) -> bool + 'static,
) {
    let dirty = dirty.to_vec();
    let dialog = widgets.dialog.clone();
    let save_button = widgets.save_button.clone();
    let title = widgets.title.clone();
    let artist = widgets.artist.clone();
    let album = widgets.album.clone();
    let album_artist = widgets.album_artist.clone();
    let genre = widgets.genre.clone();
    let year = widgets.year.clone();
    let track_number = widgets.track_number.clone();
    let rating = widgets.rating.clone();
    let error_label = widgets.error_label.clone();

    let do_save: Rc<dyn Fn()> = {
        let dirty = dirty.clone();
        let dialog = dialog.clone();
        Rc::new(move || {
            let year_patch = number_patch(dirty[FIELD_YEAR].get(), year.text().as_str());
            let track_patch =
                number_patch(dirty[FIELD_TRACK_NO].get(), track_number.text().as_str());
            let (Ok(year_patch), Ok(track_patch)) = (year_patch, track_patch) else {
                year.add_css_class("error");
                track_number.add_css_class("error");
                error_label.set_visible(true);
                tracing::debug!("tag editor rejected an invalid year or track number");
                return;
            };

            let patch = TrackEditPatch {
                tags: TagPatch {
                    title: string_patch(dirty[FIELD_TITLE].get(), title.text().as_str()),
                    artist: string_patch(dirty[FIELD_ARTIST].get(), &artist.text()),
                    album: string_patch(dirty[FIELD_ALBUM].get(), &album.text()),
                    album_artist: string_patch(
                        dirty[FIELD_ALBUM_ARTIST].get(),
                        &album_artist.text(),
                    ),
                    year: year_patch,
                    track_no: track_patch,
                    genre: string_patch(dirty[FIELD_GENRE].get(), &genre.text()),
                },
                rating: dirty[FIELD_RATING].get().then(|| rating.get()),
            };
            on_apply(patch);
            dialog.close();
        })
    };

    {
        let do_save = do_save.clone();
        widgets.save_button.connect_clicked(move |_| do_save());
    }
    wire_save_shortcut(&dialog, &save_button, &do_save);

    let confirm_discard = discard_confirmation(&dialog, &dirty, &do_save);
    {
        let confirm_discard = confirm_discard.clone();
        widgets
            .cancel_button
            .connect_clicked(move |_| confirm_discard());
    }
    wire_escape(&dialog, &dirty, &confirm_discard);
    wire_navigation(widgets.previous_button, widgets.next_button, on_navigate);

    dialog.set_default_widget(Some(&save_button));
    widgets.title.set_activates_default(true);
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
    dirty: &[Rc<Cell<bool>>],
    save: &Rc<dyn Fn()>,
) -> Rc<dyn Fn()> {
    let dirty = dirty.to_vec();
    let dialog = dialog.clone();
    let save = save.clone();
    Rc::new(move || {
        if !dirty.iter().any(|flag| flag.get()) {
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

fn wire_escape(dialog: &adw::Dialog, dirty: &[Rc<Cell<bool>>], confirm_discard: &Rc<dyn Fn()>) {
    let dirty = dirty.to_vec();
    let confirm_discard = confirm_discard.clone();
    let controller = gtk4::EventControllerKey::new();
    controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    controller.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Escape && dirty.iter().any(|flag| flag.get()) {
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
