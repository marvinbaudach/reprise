//! Multi-selection track editor for classic file tags and the app-owned
//! rating. Dirty flags are per field: mixed or uniform values are never
//! written unless the user edits that field.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::tag_edit::{EditableTagSummary, MixedValue, TagPatch, TrackEditPatch};

use crate::ui::strings;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("expected a positive whole number")]
pub struct ParseFieldError;

const RATING_MAX: i32 = 5;

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

fn string_row(label: &str, value: &MixedValue<String>) -> adw::EntryRow {
    let title = match value {
        MixedValue::Uniform(_) => label.to_string(),
        MixedValue::Mixed => format!("{label} — {}", &strings::text(strings::MULTIPLE_VALUES)),
    };
    let row = adw::EntryRow::builder().title(title).build();
    if let MixedValue::Uniform(value) = value {
        row.set_text(value);
    }
    row
}

fn number_row(label: &str, value: &MixedValue<Option<u32>>) -> adw::EntryRow {
    let title = match value {
        MixedValue::Mixed => format!("{label} — {}", &strings::text(strings::MULTIPLE_VALUES)),
        MixedValue::Uniform(_) => label.to_string(),
    };
    let row = adw::EntryRow::builder()
        .title(title)
        .input_purpose(gtk4::InputPurpose::Digits)
        .build();
    if let MixedValue::Uniform(Some(value)) = value {
        row.set_text(&value.to_string());
    }
    row
}

fn rating_choice_labels(value: &MixedValue<i32>) -> Vec<String> {
    let mut labels = Vec::with_capacity(7);
    if matches!(value, MixedValue::Mixed) {
        labels.push(strings::text(strings::MULTIPLE_VALUES));
    }
    labels.push("☆ —".into());
    labels.extend((1..=RATING_MAX).map(|rating| format!("★ {rating}")));
    labels
}

fn rating_from_selection(started_mixed: bool, selected: u32) -> Option<i32> {
    let rating = if started_mixed {
        selected.checked_sub(1)?
    } else {
        selected
    };
    i32::try_from(rating)
        .ok()
        .filter(|rating| *rating <= RATING_MAX)
}

fn rating_row(value: &MixedValue<i32>) -> (adw::ComboRow, bool) {
    let started_mixed = matches!(value, MixedValue::Mixed);
    let labels = rating_choice_labels(value);
    let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
    let model = gtk4::StringList::new(&label_refs);
    let row = adw::ComboRow::builder()
        .title(strings::text(strings::RATING))
        .model(&model)
        .build();
    let selected = match value {
        MixedValue::Uniform(rating) => {
            u32::try_from((*rating).clamp(0, RATING_MAX)).expect("clamped rating is non-negative")
        }
        MixedValue::Mixed => 0,
    };
    row.set_selected(selected);
    (row, started_mixed)
}

fn wire_dirty(
    row: &adw::EntryRow,
    dirty: &Rc<Cell<bool>>,
    all_dirty: &[Rc<Cell<bool>>],
    apply: &gtk4::Button,
) {
    let dirty = dirty.clone();
    let all_dirty = all_dirty.to_vec();
    let apply = apply.clone();
    row.connect_changed(move |_| {
        dirty.set(true);
        apply.set_sensitive(all_dirty.iter().any(|flag| flag.get()));
    });
}

fn enable_enter_submit(dialog: &adw::Dialog, apply: &gtk4::Button, rows: &[&adw::EntryRow]) {
    dialog.set_default_widget(Some(apply));
    for row in rows {
        row.set_activates_default(true);
    }
}

fn editor_header(apply: &gtk4::Button) -> adw::HeaderBar {
    let header = adw::HeaderBar::new();
    header.pack_end(apply);
    header.set_title_widget(Some(&adw::WindowTitle::new(
        &strings::text(strings::EDIT_TAGS),
        "",
    )));
    header
}

pub fn present(
    parent: &adw::ApplicationWindow,
    summary: &EditableTagSummary,
    rating_summary: &MixedValue<i32>,
    on_apply: impl Fn(TrackEditPatch) + 'static,
) {
    let title = string_row(&strings::text(strings::TAG_TITLE), &summary.title);
    let artist = string_row(&strings::text(strings::TAG_ARTIST), &summary.artist);
    let album = string_row(&strings::text(strings::TAG_ALBUM), &summary.album);
    let album_artist = string_row(
        &strings::text(strings::TAG_ALBUM_ARTIST),
        &summary.album_artist,
    );
    let year = number_row(&strings::text(strings::TAG_YEAR), &summary.year);
    let track_no = number_row(&strings::text(strings::TAG_TRACK_NUMBER), &summary.track_no);
    let genre = string_row(&strings::text(strings::TAG_GENRE), &summary.genre);
    let (rating, rating_started_mixed) = rating_row(rating_summary);

    let group = adw::PreferencesGroup::new();
    for row in [
        &title,
        &artist,
        &album,
        &album_artist,
        &year,
        &track_no,
        &genre,
    ] {
        group.add(row);
    }
    group.add(&rating);

    let error_label = gtk4::Label::builder()
        .label(strings::text(strings::TAG_NUMBER_ERROR))
        .css_classes(["error"])
        .visible(false)
        .wrap(true)
        .xalign(0.0)
        .build();
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.append(&group);
    content.append(&error_label);
    let scrolled = gtk4::ScrolledWindow::builder()
        .child(&content)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .build();

    let apply = gtk4::Button::with_label(&strings::text(strings::APPLY));
    apply.add_css_class("suggested-action");
    apply.set_sensitive(false);
    let header = editor_header(&apply);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&scrolled));
    let dialog = adw::Dialog::builder()
        .child(&toolbar)
        .content_width(520)
        .content_height(590)
        .build();
    enable_enter_submit(
        &dialog,
        &apply,
        &[
            &title,
            &artist,
            &album,
            &album_artist,
            &year,
            &track_no,
            &genre,
        ],
    );

    let dirty: Vec<Rc<Cell<bool>>> = (0..8).map(|_| Rc::new(Cell::new(false))).collect();
    for (row, flag) in [
        (&title, &dirty[0]),
        (&artist, &dirty[1]),
        (&album, &dirty[2]),
        (&album_artist, &dirty[3]),
        (&year, &dirty[4]),
        (&track_no, &dirty[5]),
        (&genre, &dirty[6]),
    ] {
        wire_dirty(row, flag, &dirty, &apply);
    }
    {
        let rating_dirty = dirty[7].clone();
        let all_dirty = dirty.clone();
        let apply = apply.clone();
        rating.connect_selected_notify(move |_| {
            rating_dirty.set(true);
            apply.set_sensitive(all_dirty.iter().any(|flag| flag.get()));
        });
    }

    {
        let dialog = dialog.clone();
        apply.connect_clicked(move |_| {
            let year_patch = number_patch(dirty[4].get(), year.text().as_str());
            let track_patch = number_patch(dirty[5].get(), track_no.text().as_str());
            let (Ok(year_patch), Ok(track_patch)) = (year_patch, track_patch) else {
                year.add_css_class("error");
                track_no.add_css_class("error");
                error_label.set_visible(true);
                tracing::debug!("tag editor rejected an invalid year or track number");
                return;
            };
            let patch = TrackEditPatch {
                tags: TagPatch {
                    title: string_patch(dirty[0].get(), title.text().as_str()),
                    artist: string_patch(dirty[1].get(), artist.text().as_str()),
                    album: string_patch(dirty[2].get(), album.text().as_str()),
                    album_artist: string_patch(dirty[3].get(), album_artist.text().as_str()),
                    year: year_patch,
                    track_no: track_patch,
                    genre: string_patch(dirty[6].get(), genre.text().as_str()),
                },
                rating: dirty[7]
                    .get()
                    .then(|| rating_from_selection(rating_started_mixed, rating.selected()))
                    .flatten(),
            };
            on_apply(patch);
            dialog.close();
        });
    }
    dialog.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_button_with_label(root: &impl IsA<gtk4::Widget>, label: &str) -> bool {
        let mut child = root.first_child();
        while let Some(widget) = child {
            if widget
                .downcast_ref::<gtk4::Button>()
                .is_some_and(|button| button.label().as_deref() == Some(label))
                || has_button_with_label(&widget, label)
            {
                return true;
            }
            child = widget.next_sibling();
        }
        false
    }

    #[test]
    fn string_patch_writes_only_dirty_fields_and_allows_clear() {
        assert_eq!(string_patch(false, "replacement"), None);
        assert_eq!(
            string_patch(true, "replacement"),
            Some("replacement".into())
        );
        assert_eq!(string_patch(true, ""), Some(String::new()));
    }

    #[test]
    fn number_patch_distinguishes_unchanged_clear_set_and_invalid() {
        assert_eq!(number_patch(false, "bad"), Ok(None));
        assert_eq!(number_patch(true, ""), Ok(Some(None)));
        assert_eq!(number_patch(true, " 42 "), Ok(Some(Some(42))));
        assert!(number_patch(true, "forty-two").is_err());
        assert!(number_patch(true, "0").is_err());
    }

    #[test]
    fn rating_choices_keep_mixed_unrated_and_five_stars_distinct() {
        assert_eq!(
            rating_choice_labels(&MixedValue::Mixed),
            vec![
                "(multiple values)",
                "☆ —",
                "★ 1",
                "★ 2",
                "★ 3",
                "★ 4",
                "★ 5"
            ]
        );
        assert_eq!(rating_from_selection(true, 0), None);
        assert_eq!(rating_from_selection(true, 1), Some(0));
        assert_eq!(rating_from_selection(true, 6), Some(5));
        assert_eq!(rating_from_selection(false, 0), Some(0));
        assert_eq!(rating_from_selection(false, 5), Some(5));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn rating_row_shows_a_mixed_value_without_calling_it_unrated() {
        if gtk4::init().is_err() {
            return;
        }
        let (row, started_mixed) = rating_row(&MixedValue::Mixed);

        assert!(started_mixed);
        assert_eq!(row.title(), "Rating");
        assert_eq!(row.selected(), 0);
        let selected = row
            .selected_item()
            .unwrap()
            .downcast::<gtk4::StringObject>()
            .unwrap();
        assert_eq!(selected.string(), "(multiple values)");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn header_uses_the_window_close_control_instead_of_a_cancel_button() {
        if gtk4::init().is_err() {
            return;
        }
        let apply = gtk4::Button::with_label("Apply");
        let header = editor_header(&apply);

        assert!(header.shows_start_title_buttons());
        assert!(apply.is_ancestor(&header));
        assert!(!has_button_with_label(
            &header,
            &strings::text(strings::CANCEL)
        ));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn enter_activates_the_apply_button_from_every_entry_row() {
        if gtk4::init().is_err() {
            return;
        }
        let apply = gtk4::Button::with_label("Apply");
        let first = adw::EntryRow::new();
        let second = adw::EntryRow::new();
        let group = adw::PreferencesGroup::new();
        group.add(&first);
        group.add(&second);
        let header = adw::HeaderBar::new();
        header.pack_end(&apply);
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&group));
        let dialog = adw::Dialog::builder().child(&toolbar).build();

        enable_enter_submit(&dialog, &apply, &[&first, &second]);

        assert!(first.activates_default());
        assert!(second.activates_default());
        assert_eq!(dialog.default_widget(), Some(apply.upcast()));
    }
}
