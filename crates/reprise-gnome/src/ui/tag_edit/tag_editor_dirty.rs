//! Dirty-field tracking and the multi-edit pending/revert projection.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::library::tag_edit::{EditableTagSummary, MixedValue};

use crate::ui::strings;
use crate::ui::tag_editor::STAR_FILLED;
use crate::ui::tag_editor_form::{EditorMode, TagEditorForm};
use crate::ui::tag_editor_state::*;
use crate::ui::tag_editor_widgets::*;

pub(in crate::ui) type UpdateCallback = Rc<dyn Fn()>;
type UpdateCallbackSlot = Rc<RefCell<Option<UpdateCallback>>>;

struct FieldSnapshot {
    summary: EditableTagSummary,
    rating: MixedValue<i32>,
}

pub(in crate::ui) struct DirtyState {
    pub(in crate::ui) flags: Vec<Rc<Cell<bool>>>,
    pub(in crate::ui) update: UpdateCallback,
}

pub(in crate::ui) fn wire(
    mode: EditorMode,
    form: &TagEditorForm,
    summary: &EditableTagSummary,
    rating: &MixedValue<i32>,
) -> DirtyState {
    let flags: Vec<Rc<Cell<bool>>> = (0..FIELD_COUNT)
        .map(|_| Rc::new(Cell::new(false)))
        .collect();
    let snapshot = Rc::new(FieldSnapshot {
        summary: summary.clone(),
        rating: rating.clone(),
    });
    let update_holder: UpdateCallbackSlot = Rc::new(RefCell::new(None));

    let update: UpdateCallback = {
        let flags = flags.clone();
        let save_button = form.save_btn.clone();
        let pending_bar = form.pending_bar.clone();
        let snapshot = snapshot.clone();
        let update_holder = update_holder.clone();
        let title = form.title_row.clone();
        let artist = form.artist_ac.row().clone();
        let album = form.album_ac.row().clone();
        let album_artist = form.album_artist_ac.row().clone();
        let year = form.year_row.clone();
        let track_number = form.track_no_row.clone();
        let genre = form.genre_ac.row().clone();
        let rating_value = form.rating_value.clone();
        let rating_box = form.rating_box.clone();
        let annotations = [
            None,
            form.artist_annotation.clone(),
            form.album_annotation.clone(),
            form.album_artist_annotation.clone(),
            form.year_annotation.clone(),
            None,
            form.genre_annotation.clone(),
        ];

        Rc::new(move || {
            save_button.set_sensitive(flags.iter().any(|flag| flag.get()));
            if !mode.is_multi() {
                return;
            }
            while let Some(child) = pending_bar.first_child() {
                pending_bar.remove(&child);
            }
            let dirty_count = flags.iter().filter(|flag| flag.get()).count();
            if dirty_count == 0 {
                pending_bar.set_visible(false);
                return;
            }

            let header = gtk4::Label::builder()
                .label(strings::tag_pending_count(dirty_count))
                .xalign(0.0)
                .build();
            header.add_css_class("reprise-tag-pending-header");
            pending_bar.append(&header);

            let rows = [
                title.clone(),
                artist.clone(),
                album.clone(),
                album_artist.clone(),
                year.clone(),
                track_number.clone(),
                genre.clone(),
            ];
            for (index, row) in rows.into_iter().enumerate() {
                if !flags[index].get() {
                    continue;
                }
                let update = update_holder
                    .borrow()
                    .as_ref()
                    .expect("dirty update installed before interaction")
                    .clone();
                let dirty = flags[index].clone();
                let annotation = annotations[index].clone();
                let snapshot = snapshot.clone();
                let row_for_revert = row.clone();
                let revert = Box::new(move || {
                    let original = field_snapshot_text(&snapshot.summary, index);
                    if field_snapshot_is_mixed(&snapshot.summary, index) {
                        row_for_revert.set_editable(false);
                        row_for_revert.add_css_class("reprise-tag-mixed");
                        if let Some(label) = &annotation {
                            label.set_text(&strings::text(strings::MULTIPLE_VALUES));
                            label.remove_css_class("accent");
                        }
                        row_for_revert.set_text("");
                    } else {
                        row_for_revert.set_text(original.as_deref().unwrap_or(""));
                    }
                    dirty.set(false);
                    update();
                });
                pending_bar.append(&build_pending_item(&field_name(index), &row.text(), revert));
            }

            if flags[FIELD_RATING].get() {
                let update = update_holder
                    .borrow()
                    .as_ref()
                    .expect("dirty update installed before interaction")
                    .clone();
                let dirty = flags[FIELD_RATING].clone();
                let value = rating_value.clone();
                let box_for_revert = rating_box.clone();
                let original = snapshot.rating.clone();
                let text = format!("{STAR_FILLED} {}", rating_value.get());
                let revert = Box::new(move || {
                    let original = match &original {
                        MixedValue::Uniform(value) => *value,
                        MixedValue::Mixed => 0,
                    };
                    value.set(original);
                    update_star_display(&box_for_revert, original);
                    dirty.set(false);
                    update();
                });
                pending_bar.append(&build_pending_item(
                    &field_name(FIELD_RATING),
                    &text,
                    revert,
                ));
            }
            pending_bar.set_visible(true);
        })
    };
    *update_holder.borrow_mut() = Some(update.clone());

    wire_entry(&form.title_row, FIELD_TITLE, &flags, &update);
    wire_entry(&form.year_row, FIELD_YEAR, &flags, &update);
    wire_entry(&form.track_no_row, FIELD_TRACK_NO, &flags, &update);
    wire_autocomplete(&form.artist_ac, FIELD_ARTIST, &flags, &update);
    wire_autocomplete(&form.album_ac, FIELD_ALBUM, &flags, &update);
    wire_autocomplete(&form.album_artist_ac, FIELD_ALBUM_ARTIST, &flags, &update);
    wire_autocomplete(&form.genre_ac, FIELD_GENRE, &flags, &update);

    let dirty = flags[FIELD_RATING].clone();
    let rating_update = update.clone();
    let on_rating_changed: UpdateCallback = Rc::new(move || {
        dirty.set(true);
        rating_update();
    });
    wire_star_clicks(&form.rating_box, &form.rating_value, &on_rating_changed);

    DirtyState { flags, update }
}

fn wire_entry(
    row: &adw::EntryRow,
    index: usize,
    flags: &[Rc<Cell<bool>>],
    update: &UpdateCallback,
) {
    let dirty = flags[index].clone();
    let update = update.clone();
    row.connect_changed(move |_| {
        dirty.set(true);
        update();
    });
}

fn wire_autocomplete(
    entry: &crate::ui::autocomplete_entry::AutocompleteEntry,
    index: usize,
    flags: &[Rc<Cell<bool>>],
    update: &UpdateCallback,
) {
    let dirty = flags[index].clone();
    let update = update.clone();
    entry.connect_changed(move || {
        dirty.set(true);
        update();
    });
}
