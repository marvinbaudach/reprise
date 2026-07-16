//! Widget builders and mixed-value presentation helpers for the tag editor.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::gdk;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::cover::{self, ThumbnailSize};
use reprise_core::library::tag_edit::{EditableTagSummary, MixedValue};

use crate::ui::autocomplete_entry::AutocompleteEntry;
use crate::ui::strings;
use crate::ui::tag_editor::{STAR_FILLED, STAR_OUTLINE};
use crate::ui::tag_editor_dirty::UpdateCallback;
use crate::ui::tag_editor_state::{
    FIELD_ALBUM, FIELD_ALBUM_ARTIST, FIELD_ARTIST, FIELD_GENRE, FIELD_YEAR, RATING_MAX,
};

/// Builds the cover art area. For single track, shows a thumbnail. For
/// multi-track, shows a stacked representation with a count badge.
pub(in crate::ui) fn build_cover_area(tracks: &[(i64, PathBuf)], is_multi: bool) -> gtk4::Box {
    let outer = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    outer.set_halign(gtk4::Align::Center);
    outer.set_margin_bottom(8);

    if is_multi {
        // Stacked cover display
        let overlay = gtk4::Overlay::new();
        overlay.add_css_class("reprise-tag-cover-stack");

        let cover = load_cover_picture(tracks.first().map(|(_, p)| p.as_path()));
        cover.set_size_request(180, 180);
        overlay.set_child(Some(&cover));

        // Badge: "N covers"
        let badge = gtk4::Label::new(Some(&strings::tag_cover_count(tracks.len())));
        badge.add_css_class("reprise-tag-cover-badge");
        badge.set_halign(gtk4::Align::End);
        badge.set_valign(gtk4::Align::End);
        badge.set_margin_end(8);
        badge.set_margin_bottom(8);
        overlay.add_overlay(&badge);

        outer.append(&overlay);
    } else {
        // Single cover
        let cover = load_cover_picture(tracks.first().map(|(_, p)| p.as_path()));
        cover.set_size_request(200, 200);
        outer.append(&cover);

        // "Change cover..." link (disabled for v1)
        let change_link = gtk4::Button::with_label(&strings::text(strings::TAG_CHANGE_COVER));
        change_link.add_css_class("flat");
        change_link.add_css_class("reprise-tag-cover-link");
        change_link.set_sensitive(false);
        change_link.set_halign(gtk4::Align::Center);
        outer.append(&change_link);
    }

    outer
}

/// Loads a cover thumbnail for a track path, returning a `gtk4::Picture`
/// wrapped in a frame box. Falls back to a placeholder if no cover is found.
pub(in crate::ui) fn load_cover_picture(track_path: Option<&Path>) -> gtk4::Box {
    let frame = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    frame.add_css_class("reprise-tag-cover");
    frame.set_overflow(gtk4::Overflow::Hidden);

    let picture = gtk4::Picture::new();
    picture.set_content_fit(gtk4::ContentFit::Cover);
    picture.set_can_shrink(true);

    let loaded = track_path
        .and_then(cover::resolve_source)
        .and_then(|source| cover::thumbnail(&source, ThumbnailSize::Grid).ok());

    if let Some(thumb_path) = loaded {
        let texture = gdk::Texture::from_filename(&thumb_path).ok();
        if let Some(texture) = texture {
            picture.set_paintable(Some(&texture));
        }
    }

    frame.append(&picture);
    frame
}

/// Builds the clickable star rating widget. Returns the container box and
/// a shared `Cell` holding the current rating value.
pub(in crate::ui) fn build_star_rating(value: &MixedValue<i32>) -> (gtk4::Box, Rc<Cell<i32>>) {
    let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
    container.add_css_class("reprise-tag-stars");

    let current = match value {
        MixedValue::Uniform(v) => *v,
        MixedValue::Mixed => 0,
    };
    let rating_value = Rc::new(Cell::new(current));

    for i in 1..=RATING_MAX {
        let btn = gtk4::Button::new();
        btn.add_css_class("flat");
        let label = gtk4::Label::new(None);
        btn.set_child(Some(&label));

        if i <= current {
            label.set_label(STAR_FILLED);
            btn.add_css_class("star-filled");
        } else {
            label.set_label(STAR_OUTLINE);
            btn.add_css_class("star-outline");
        }
        container.append(&btn);
    }

    // Add a clear button (click current star again clears)
    update_star_display(&container, current);
    (container, rating_value)
}

/// Updates the visual state of star buttons in a rating box.
pub(in crate::ui) fn update_star_display(container: &gtk4::Box, rating: i32) {
    let mut child = container.first_child();
    let mut idx = 1;
    while let Some(widget) = child {
        if let Some(btn) = widget.downcast_ref::<gtk4::Button>() {
            if let Some(label) = btn.child().and_then(|c| c.downcast::<gtk4::Label>().ok()) {
                if idx <= rating {
                    label.set_label(STAR_FILLED);
                    btn.remove_css_class("star-outline");
                    btn.add_css_class("star-filled");
                } else {
                    label.set_label(STAR_OUTLINE);
                    btn.remove_css_class("star-filled");
                    btn.add_css_class("star-outline");
                }
            }
        }
        child = widget.next_sibling();
        idx += 1;
    }
}

/// Wires click handlers on each star button. Clicking star N sets rating
/// to N; clicking the already-selected star clears to 0.
pub(in crate::ui) fn wire_star_clicks(
    container: &gtk4::Box,
    rating_value: &Rc<Cell<i32>>,
    on_changed: &UpdateCallback,
) {
    let mut child = container.first_child();
    let mut idx: i32 = 1;
    while let Some(widget) = child {
        if let Some(btn) = widget.downcast_ref::<gtk4::Button>() {
            let rating_val = rating_value.clone();
            let container_c = container.clone();
            let on_changed_c = on_changed.clone();
            let star_idx = idx;
            btn.connect_clicked(move |_| {
                let current = rating_val.get();
                let new_rating = if current == star_idx { 0 } else { star_idx };
                rating_val.set(new_rating);
                update_star_display(&container_c, new_rating);
                on_changed_c();
            });
        }
        child = widget.next_sibling();
        idx += 1;
    }
}

/// Sets an `EntryRow` text from a `MixedValue<String>`.
pub(in crate::ui) fn set_entry_from_mixed_string(row: &adw::EntryRow, value: &MixedValue<String>) {
    match value {
        MixedValue::Uniform(text) => row.set_text(text),
        MixedValue::Mixed => {
            // Leave empty; the placeholder/annotation conveys the state
        }
    }
}

/// Sets an `EntryRow` text from a `MixedValue<Option<u32>>`.
pub(in crate::ui) fn set_entry_from_mixed_number(
    row: &adw::EntryRow,
    value: &MixedValue<Option<u32>>,
) {
    match value {
        MixedValue::Uniform(Some(n)) => row.set_text(&n.to_string()),
        MixedValue::Uniform(None) | MixedValue::Mixed => {}
    }
}

/// Initialises an `AutocompleteEntry` from a `MixedValue`, adding
/// mixed-field annotations in multi-track mode. Returns the annotation label
/// when the field starts Mixed (needed for click-to-unlock updates).
pub(in crate::ui) fn init_autocomplete_from_mixed(
    ac: &AutocompleteEntry,
    value: &MixedValue<String>,
    _track_count: usize,
    is_multi: bool,
) -> Option<gtk4::Label> {
    match value {
        MixedValue::Uniform(text) => {
            ac.set_text(text);
            if is_multi {
                add_annotation(ac.row(), &strings::text(strings::TAG_SAME_ON_ALL), false);
            }
            None
        }
        MixedValue::Mixed => {
            if is_multi {
                ac.row().set_editable(false);
                ac.row().add_css_class("reprise-tag-mixed");
                Some(add_annotation(
                    ac.row(),
                    &strings::text(strings::MULTIPLE_VALUES),
                    false,
                ))
            } else {
                None
            }
        }
    }
}

/// Adds a mixed-field annotation for number fields in multi-track mode.
/// Returns the annotation label (needed for click-to-unlock updates).
pub(in crate::ui) fn apply_mixed_annotation_number(
    row: &adw::EntryRow,
    value: &MixedValue<Option<u32>>,
    _track_count: usize,
) -> Option<gtk4::Label> {
    match value {
        MixedValue::Uniform(_) => {
            add_annotation(row, &strings::text(strings::TAG_SAME_ON_ALL), false);
            None
        }
        MixedValue::Mixed => {
            row.set_editable(false);
            row.add_css_class("reprise-tag-mixed");
            Some(add_annotation(
                row,
                &strings::text(strings::MULTIPLE_VALUES),
                false,
            ))
        }
    }
}

/// Adds a small annotation label as a suffix to an `EntryRow` and returns it.
pub(in crate::ui) fn add_annotation(row: &adw::EntryRow, text: &str, accent: bool) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.add_css_class("reprise-tag-field-annotation");
    if accent {
        label.add_css_class("accent");
    }
    row.add_suffix(&label);
    label
}

/// Attaches a click gesture to a Mixed field so the user can unlock it for
/// editing. On first click: makes the entry editable, clears the text,
/// removes the mixed CSS class, and updates the annotation to the
/// "will be applied to all N" copy.
pub(in crate::ui) fn attach_click_to_unlock(
    row: &adw::EntryRow,
    annotation: Option<&gtk4::Label>,
    track_count: usize,
) {
    let row_c = row.clone();
    let annotation_c = annotation.cloned();
    let will_apply = strings::tag_will_apply(track_count);
    let gesture = gtk4::GestureClick::new();
    gesture.connect_released(move |_, _, _, _| {
        if !row_c.is_editable() {
            row_c.set_editable(true);
            row_c.remove_css_class("reprise-tag-mixed");
            row_c.set_text("");
            if let Some(lbl) = &annotation_c {
                lbl.set_text(&will_apply);
                lbl.add_css_class("accent");
            }
            row_c.grab_focus();
        }
    });
    row.add_controller(gesture);
}

/// Returns the original text value for a given field index from a snapshot.
/// Returns `None` for fields that were Mixed (no single value).
pub(in crate::ui) fn field_snapshot_text(
    summary: &EditableTagSummary,
    field_idx: usize,
) -> Option<String> {
    match field_idx {
        FIELD_ARTIST => match &summary.artist {
            MixedValue::Uniform(v) => Some(v.clone()),
            MixedValue::Mixed => None,
        },
        FIELD_ALBUM => match &summary.album {
            MixedValue::Uniform(v) => Some(v.clone()),
            MixedValue::Mixed => None,
        },
        FIELD_ALBUM_ARTIST => match &summary.album_artist {
            MixedValue::Uniform(v) => Some(v.clone()),
            MixedValue::Mixed => None,
        },
        FIELD_GENRE => match &summary.genre {
            MixedValue::Uniform(v) => Some(v.clone()),
            MixedValue::Mixed => None,
        },
        FIELD_YEAR => match &summary.year {
            MixedValue::Uniform(Some(v)) => Some(v.to_string()),
            MixedValue::Uniform(None) | MixedValue::Mixed => None,
        },
        _ => None,
    }
}

/// Returns true if the given field was originally Mixed in the snapshot.
pub(in crate::ui) fn field_snapshot_is_mixed(
    summary: &EditableTagSummary,
    field_idx: usize,
) -> bool {
    match field_idx {
        FIELD_ARTIST => matches!(summary.artist, MixedValue::Mixed),
        FIELD_ALBUM => matches!(summary.album, MixedValue::Mixed),
        FIELD_ALBUM_ARTIST => matches!(summary.album_artist, MixedValue::Mixed),
        FIELD_GENRE => matches!(summary.genre, MixedValue::Mixed),
        FIELD_YEAR => matches!(summary.year, MixedValue::Mixed),
        _ => false,
    }
}

/// Builds a single pending-change item: "Field → Value" with a Revert button.
pub(in crate::ui) fn build_pending_item(
    field_name: &str,
    value: &str,
    on_revert: Box<dyn Fn()>,
) -> gtk4::Box {
    let item = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    item.add_css_class("reprise-tag-pending-item");

    let text = format!("{field_name} \u{2192} {value}");
    let label = gtk4::Label::builder()
        .label(&text)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    item.append(&label);

    let revert_btn = gtk4::Button::with_label(&strings::text(strings::TAG_REVERT));
    revert_btn.add_css_class("flat");
    revert_btn.connect_clicked(move |_| on_revert());
    item.append(&revert_btn);

    item
}
