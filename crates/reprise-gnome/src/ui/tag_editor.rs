//! Multi-selection classic-tag editor. Dirty flags are per field: mixed or
//! uniform values are never written unless the user edits that field.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::tag_edit::{EditableTagSummary, MixedValue, TagPatch};

use crate::ui::strings;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("expected a positive whole number")]
pub struct ParseFieldError;

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
        MixedValue::Mixed => format!("{label} — {}", strings::MULTIPLE_VALUES),
    };
    let row = adw::EntryRow::builder().title(title).build();
    if let MixedValue::Uniform(value) = value {
        row.set_text(value);
    }
    row
}

fn number_row(label: &str, value: &MixedValue<Option<u32>>) -> adw::EntryRow {
    let title = match value {
        MixedValue::Mixed => format!("{label} — {}", strings::MULTIPLE_VALUES),
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

pub fn present(
    parent: &adw::ApplicationWindow,
    summary: &EditableTagSummary,
    on_apply: impl Fn(TagPatch) + 'static,
) {
    let title = string_row(strings::TAG_TITLE, &summary.title);
    let artist = string_row(strings::TAG_ARTIST, &summary.artist);
    let album = string_row(strings::TAG_ALBUM, &summary.album);
    let album_artist = string_row(strings::TAG_ALBUM_ARTIST, &summary.album_artist);
    let year = number_row(strings::TAG_YEAR, &summary.year);
    let track_no = number_row(strings::TAG_TRACK_NUMBER, &summary.track_no);
    let genre = string_row(strings::TAG_GENRE, &summary.genre);

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

    let error_label = gtk4::Label::builder()
        .label(strings::TAG_NUMBER_ERROR)
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

    let cancel = gtk4::Button::with_label(strings::CANCEL);
    let apply = gtk4::Button::with_label(strings::APPLY);
    apply.add_css_class("suggested-action");
    apply.set_sensitive(false);
    let header = adw::HeaderBar::new();
    header.pack_start(&cancel);
    header.pack_end(&apply);
    header.set_title_widget(Some(&adw::WindowTitle::new(strings::EDIT_TAGS, "")));
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&content));
    let dialog = adw::Dialog::builder()
        .child(&toolbar)
        .content_width(520)
        .content_height(590)
        .build();

    let dirty: Vec<Rc<Cell<bool>>> = (0..7).map(|_| Rc::new(Cell::new(false))).collect();
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
        let dialog = dialog.clone();
        cancel.connect_clicked(move |_| {
            dialog.close();
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
                return;
            };
            let patch = TagPatch {
                title: string_patch(dirty[0].get(), title.text().as_str()),
                artist: string_patch(dirty[1].get(), artist.text().as_str()),
                album: string_patch(dirty[2].get(), album.text().as_str()),
                album_artist: string_patch(dirty[3].get(), album_artist.text().as_str()),
                year: year_patch,
                track_no: track_patch,
                genre: string_patch(dirty[6].get(), genre.text().as_str()),
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
}
