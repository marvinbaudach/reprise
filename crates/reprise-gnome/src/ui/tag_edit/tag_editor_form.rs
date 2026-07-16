//! Tag-editor form composition and single/multi mode labels.

use std::cell::{Cell, RefCell};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::tag_edit::{EditableTagSummary, MixedValue};
use reprise_core::queries::autocomplete::AutocompleteColumn;
use rusqlite::Connection;

use crate::ui::autocomplete_entry::AutocompleteEntry;
use crate::ui::strings;
use crate::ui::tag_editor_widgets::*;

#[derive(Clone, Copy)]
pub(super) struct EditorMode {
    track_count: NonZeroUsize,
}

impl EditorMode {
    pub(super) fn new(track_count: usize) -> Option<Self> {
        NonZeroUsize::new(track_count).map(|track_count| Self { track_count })
    }

    pub(super) fn track_count(self) -> usize {
        self.track_count.get()
    }

    pub(super) fn is_multi(self) -> bool {
        self.track_count.get() > 1
    }

    fn save_label(self) -> String {
        if self.is_multi() {
            strings::tag_save_count(self.track_count())
        } else {
            strings::text(strings::TAG_SAVE)
        }
    }

    fn title(self) -> String {
        if self.is_multi() {
            strings::tag_edit_title_multi(self.track_count())
        } else {
            strings::text(strings::TAG_EDIT_TITLE_SINGLE)
        }
    }
}

pub(super) struct TagEditorForm {
    pub(super) save_btn: gtk4::Button,
    pub(super) cancel_btn: gtk4::Button,
    pub(super) dialog: adw::Dialog,
    pub(super) title_row: adw::EntryRow,
    pub(super) artist_ac: Rc<AutocompleteEntry>,
    pub(super) album_ac: Rc<AutocompleteEntry>,
    pub(super) album_artist_ac: Rc<AutocompleteEntry>,
    pub(super) genre_ac: Rc<AutocompleteEntry>,
    pub(super) year_row: adw::EntryRow,
    pub(super) track_no_row: adw::EntryRow,
    pub(super) artist_annotation: Option<gtk4::Label>,
    pub(super) album_annotation: Option<gtk4::Label>,
    pub(super) album_artist_annotation: Option<gtk4::Label>,
    pub(super) genre_annotation: Option<gtk4::Label>,
    pub(super) year_annotation: Option<gtk4::Label>,
    pub(super) rating_box: gtk4::Box,
    pub(super) rating_value: Rc<Cell<i32>>,
    pub(super) error_label: gtk4::Label,
    pub(super) pending_bar: gtk4::Box,
    pub(super) mb_btn: gtk4::Button,
    pub(super) mb_hint: gtk4::Label,
    pub(super) prev_btn: gtk4::Button,
    pub(super) next_btn: gtk4::Button,
}

impl TagEditorForm {
    pub(super) fn build(
        mode: EditorMode,
        conn: &Rc<RefCell<Connection>>,
        tracks: &[(i64, PathBuf)],
        summary: &EditableTagSummary,
        rating: &MixedValue<i32>,
    ) -> Self {
        let is_multi = mode.is_multi();
        let track_count = mode.track_count();
        let save_btn = gtk4::Button::with_label(&mode.save_label());
        save_btn.add_css_class("suggested-action");
        save_btn.set_sensitive(false);
        let cancel_btn = gtk4::Button::with_label(&strings::text(strings::CANCEL));

        let title_widget = adw::WindowTitle::new(&mode.title(), "");
        let header = adw::HeaderBar::new();
        header.pack_start(&cancel_btn);
        header.pack_end(&save_btn);
        header.set_title_widget(Some(&title_widget));
        header.set_show_start_title_buttons(false);
        header.set_show_end_title_buttons(false);

        let cover_area = build_cover_area(tracks, is_multi);
        let title_row = adw::EntryRow::builder()
            .title(strings::text(strings::TAG_TITLE))
            .build();
        if is_multi {
            title_row.set_editable(false);
            title_row.add_css_class("reprise-tag-mixed");
            add_annotation(&title_row, &strings::text(strings::TAG_PER_TRACK), false);
        }
        set_entry_from_mixed_string(&title_row, &summary.title);

        let artist_ac = Rc::new(AutocompleteEntry::new(
            &strings::text(strings::TAG_ARTIST),
            AutocompleteColumn::Artist,
            conn.clone(),
        ));
        let artist_annotation =
            init_autocomplete_from_mixed(&artist_ac, &summary.artist, track_count, is_multi);
        if is_multi && matches!(summary.artist, MixedValue::Mixed) {
            attach_click_to_unlock(artist_ac.row(), artist_annotation.as_ref(), track_count);
        }

        let album_ac = Rc::new(AutocompleteEntry::new(
            &strings::text(strings::TAG_ALBUM),
            AutocompleteColumn::Album,
            conn.clone(),
        ));
        let album_annotation =
            init_autocomplete_from_mixed(&album_ac, &summary.album, track_count, is_multi);
        if is_multi && matches!(summary.album, MixedValue::Mixed) {
            attach_click_to_unlock(album_ac.row(), album_annotation.as_ref(), track_count);
        }

        let album_artist_ac = Rc::new(AutocompleteEntry::new(
            &strings::text(strings::TAG_ALBUM_ARTIST),
            AutocompleteColumn::AlbumArtist,
            conn.clone(),
        ));
        let album_artist_annotation = init_autocomplete_from_mixed(
            &album_artist_ac,
            &summary.album_artist,
            track_count,
            is_multi,
        );
        if is_multi && matches!(summary.album_artist, MixedValue::Mixed) {
            attach_click_to_unlock(
                album_artist_ac.row(),
                album_artist_annotation.as_ref(),
                track_count,
            );
        }

        let genre_ac = Rc::new(AutocompleteEntry::new(
            &strings::text(strings::TAG_GENRE),
            AutocompleteColumn::Genre,
            conn.clone(),
        ));
        let genre_annotation =
            init_autocomplete_from_mixed(&genre_ac, &summary.genre, track_count, is_multi);
        if is_multi && matches!(summary.genre, MixedValue::Mixed) {
            attach_click_to_unlock(genre_ac.row(), genre_annotation.as_ref(), track_count);
        }

        let year_row = adw::EntryRow::builder()
            .title(strings::text(strings::TAG_YEAR))
            .input_purpose(gtk4::InputPurpose::Digits)
            .build();
        set_entry_from_mixed_number(&year_row, &summary.year);
        let year_annotation = is_multi
            .then(|| apply_mixed_annotation_number(&year_row, &summary.year, track_count))
            .flatten();
        if is_multi && matches!(summary.year, MixedValue::Mixed) {
            attach_click_to_unlock(&year_row, year_annotation.as_ref(), track_count);
        }

        let track_no_row = adw::EntryRow::builder()
            .title(strings::text(strings::TAG_TRACK_NUMBER))
            .input_purpose(gtk4::InputPurpose::Digits)
            .build();
        if is_multi {
            track_no_row.set_editable(false);
            track_no_row.add_css_class("reprise-tag-mixed");
            add_annotation(&track_no_row, &strings::text(strings::TAG_PER_TRACK), false);
        }
        set_entry_from_mixed_number(&track_no_row, &summary.track_no);

        let (rating_box, rating_value) = build_star_rating(rating);
        let group = adw::PreferencesGroup::new();
        group.add(&title_row);
        group.add(artist_ac.row());
        group.add(album_ac.row());
        group.add(album_artist_ac.row());
        group.add(genre_ac.row());
        group.add(&year_row);
        group.add(&track_no_row);
        let rating_row = adw::ActionRow::builder()
            .title(strings::text(strings::RATING))
            .build();
        rating_row.add_suffix(&rating_box);
        group.add(&rating_row);

        let error_label = gtk4::Label::builder()
            .label(strings::text(strings::TAG_NUMBER_ERROR))
            .css_classes(["reprise-tag-error"])
            .visible(false)
            .wrap(true)
            .xalign(0.0)
            .build();
        let pending_bar = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        pending_bar.add_css_class("reprise-tag-pending");
        pending_bar.set_visible(false);

        let mb_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        mb_box.add_css_class("reprise-tag-mb");
        let mb_btn = gtk4::Button::with_label(&strings::text(strings::TAG_FETCH_MUSICBRAINZ));
        mb_btn.set_sensitive(false);
        mb_box.append(&mb_btn);
        let mb_hint = gtk4::Label::new(Some(&strings::text(strings::TAG_FETCH_HINT)));
        mb_hint.add_css_class("reprise-tag-mb-hint");
        mb_hint.set_xalign(0.0);
        mb_box.append(&mb_hint);

        let nav_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        nav_box.add_css_class("reprise-tag-nav");
        nav_box.set_halign(gtk4::Align::Center);
        let prev_btn = gtk4::Button::from_icon_name("go-previous-symbolic");
        prev_btn.set_tooltip_text(Some(&strings::text(strings::PREVIOUS)));
        let next_btn = gtk4::Button::from_icon_name("go-next-symbolic");
        next_btn.set_tooltip_text(Some(&strings::text(strings::NEXT)));
        nav_box.append(&prev_btn);
        nav_box.append(&next_btn);
        nav_box.set_visible(false);

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        content.set_margin_top(12);
        content.set_margin_bottom(18);
        content.set_margin_start(18);
        content.set_margin_end(18);
        content.append(&cover_area);
        content.append(&group);
        content.append(&error_label);
        if is_multi {
            content.append(&pending_bar);
        }
        content.append(&mb_box);
        if !is_multi {
            content.append(&nav_box);
        }
        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&content)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .build();
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&scrolled));
        let dialog = adw::Dialog::builder()
            .child(&toolbar)
            .content_width(560)
            .content_height(700)
            .build();
        dialog.add_css_class("reprise-tag-editor");

        Self {
            save_btn,
            cancel_btn,
            dialog,
            title_row,
            artist_ac,
            album_ac,
            album_artist_ac,
            genre_ac,
            year_row,
            track_no_row,
            artist_annotation,
            album_annotation,
            album_artist_annotation,
            genre_annotation,
            year_annotation,
            rating_box,
            rating_value,
            error_label,
            pending_bar,
            mb_btn,
            mb_hint,
            prev_btn,
            next_btn,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn mode_rejects_empty_and_selects_single_or_multi_copy() {
        assert!(super::EditorMode::new(0).is_none());

        let single = super::EditorMode::new(1).unwrap();
        assert!(!single.is_multi());
        assert_eq!(single.track_count(), 1);

        let multi = super::EditorMode::new(3).unwrap();
        assert!(multi.is_multi());
        assert_eq!(multi.track_count(), 3);
    }
}
