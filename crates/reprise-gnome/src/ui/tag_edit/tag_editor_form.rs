//! Tag-editor form composition and single/multi mode labels.

use std::cell::{Cell, RefCell};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::library::tag_edit::{EditableTagSummary, MixedValue};
use reprise_core::queries::autocomplete::AutocompleteColumn;
use rusqlite::Connection;

use crate::ui::autocomplete_entry::AutocompleteEntry;
use crate::ui::strings;
use crate::ui::tag_editor_widgets::*;

#[derive(Clone, Copy)]
pub(in crate::ui) struct EditorMode {
    track_count: NonZeroUsize,
}

impl EditorMode {
    pub(in crate::ui) fn new(track_count: usize) -> Option<Self> {
        NonZeroUsize::new(track_count).map(|track_count| Self { track_count })
    }

    pub(in crate::ui) fn track_count(self) -> usize {
        self.track_count.get()
    }

    pub(in crate::ui) fn is_multi(self) -> bool {
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

pub(in crate::ui) struct TagEditorForm {
    pub(in crate::ui) save_btn: gtk4::Button,
    pub(in crate::ui) cancel_btn: gtk4::Button,
    pub(in crate::ui) dialog: adw::Dialog,
    pub(in crate::ui) title_row: adw::EntryRow,
    pub(in crate::ui) artist_ac: Rc<AutocompleteEntry>,
    pub(in crate::ui) album_ac: Rc<AutocompleteEntry>,
    pub(in crate::ui) album_artist_ac: Rc<AutocompleteEntry>,
    pub(in crate::ui) genre_ac: Rc<AutocompleteEntry>,
    pub(in crate::ui) year_row: adw::EntryRow,
    pub(in crate::ui) track_no_row: adw::EntryRow,
    pub(in crate::ui) artist_annotation: Option<gtk4::Label>,
    pub(in crate::ui) album_annotation: Option<gtk4::Label>,
    pub(in crate::ui) album_artist_annotation: Option<gtk4::Label>,
    pub(in crate::ui) genre_annotation: Option<gtk4::Label>,
    pub(in crate::ui) year_annotation: Option<gtk4::Label>,
    pub(in crate::ui) rating_box: gtk4::Box,
    pub(in crate::ui) rating_value: Rc<Cell<i32>>,
    pub(in crate::ui) error_label: gtk4::Label,
    pub(in crate::ui) pending_bar: gtk4::Box,
    pub(in crate::ui) mb_btn: gtk4::Button,
    pub(in crate::ui) mb_hint: gtk4::Label,
    pub(in crate::ui) prev_btn: gtk4::Button,
    pub(in crate::ui) next_btn: gtk4::Button,
}

impl TagEditorForm {
    pub(in crate::ui) fn build(
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

        // Header subtitle (Beschluss #2): Multi explains the batch-write
        // scope; Single renders "FORMAT · bitrate" from the file extension.
        // The "Track N of M" position prefix is Package G's job (TAG-4
        // browse snapshot) — bitrate isn't threaded into this constructor
        // yet either (see this module's doc comment), so only the format
        // half renders for now; never a fabricated or stale bitrate.
        let subtitle = if is_multi {
            strings::text(strings::TAG_SUBTITLE_MULTI)
        } else {
            let extension = tracks
                .first()
                .and_then(|(_, path)| path.extension())
                .and_then(|ext| ext.to_str());
            format_track_subtitle(extension, None).unwrap_or_default()
        };
        let title_widget = adw::WindowTitle::new(&mode.title(), &subtitle);
        let header = adw::HeaderBar::new();
        header.pack_start(&cancel_btn);
        header.pack_end(&save_btn);
        header.set_title_widget(Some(&title_widget));
        header.set_show_start_title_buttons(false);
        header.set_show_end_title_buttons(false);

        // --- Cover (left) + Title/Artist/Album (right) ---
        let cover_area = build_cover_area(tracks, is_multi);

        let title_row = adw::EntryRow::builder()
            .title(strings::text(strings::TAG_TITLE))
            .build();
        apply_per_track_field(&title_row, is_multi);
        if !is_multi {
            set_entry_from_mixed_string(&title_row, &summary.title);
        }

        let artist_ac = Rc::new(AutocompleteEntry::new(
            &strings::text(strings::TAG_ARTIST),
            AutocompleteColumn::Artist,
            conn.clone(),
        ));
        let artist_annotation =
            init_autocomplete_from_mixed(&artist_ac, &summary.artist, track_count, is_multi);
        if is_multi && matches!(summary.artist, MixedValue::Mixed) {
            attach_type_to_arm(artist_ac.row(), artist_annotation.as_ref(), track_count);
        }

        let album_ac = Rc::new(AutocompleteEntry::new(
            &strings::text(strings::TAG_ALBUM),
            AutocompleteColumn::Album,
            conn.clone(),
        ));
        let album_annotation =
            init_autocomplete_from_mixed(&album_ac, &summary.album, track_count, is_multi);
        if is_multi && matches!(summary.album, MixedValue::Mixed) {
            attach_type_to_arm(album_ac.row(), album_annotation.as_ref(), track_count);
        }

        let (title_col, _title_old_value) = build_field_column(title_row.upcast_ref(), None);
        let (artist_col, _artist_old_value) =
            build_field_column(artist_ac.row().upcast_ref(), None);
        let (album_col, _album_old_value) = build_field_column(album_ac.row().upcast_ref(), None);

        let title_artist_album = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        title_artist_album.set_hexpand(true);
        title_artist_album.append(&title_col);
        title_artist_album.append(&artist_col);
        title_artist_album.append(&album_col);

        let top_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 16);
        top_row.append(&cover_area);
        top_row.append(&title_artist_album);

        // --- 2-column grid: Album artist/Genre, Year/Track number ---
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
            attach_type_to_arm(
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
            attach_type_to_arm(genre_ac.row(), genre_annotation.as_ref(), track_count);
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
            attach_type_to_arm(&year_row, year_annotation.as_ref(), track_count);
        }

        let track_no_row = adw::EntryRow::builder()
            .title(strings::text(strings::TAG_TRACK_NUMBER))
            .input_purpose(gtk4::InputPurpose::Digits)
            .build();
        apply_per_track_field(&track_no_row, is_multi);
        if !is_multi {
            set_entry_from_mixed_number(&track_no_row, &summary.track_no);
        }

        let (album_artist_col, _album_artist_old_value) =
            build_field_column(album_artist_ac.row().upcast_ref(), None);
        let (genre_col, _genre_old_value) = build_field_column(genre_ac.row().upcast_ref(), None);
        let (year_col, _year_old_value) = build_field_column(year_row.upcast_ref(), None);
        let (track_no_col, _track_no_old_value) =
            build_field_column(track_no_row.upcast_ref(), None);

        let grid = gtk4::Grid::builder()
            .column_spacing(16)
            .row_spacing(4)
            .column_homogeneous(true)
            .build();
        grid.attach(&album_artist_col, 0, 0, 1, 1);
        grid.attach(&genre_col, 1, 0, 1, 1);
        grid.attach(&year_col, 0, 1, 1, 1);
        grid.attach(&track_no_col, 1, 1, 1, 1);

        let (rating_box, rating_value) = build_star_rating(rating);
        let (rating_col, _rating_old_value) = build_field_column(
            rating_box.upcast_ref(),
            Some(&strings::text(strings::RATING)),
        );

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

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 14);
        content.set_margin_top(12);
        content.set_margin_bottom(18);
        content.set_margin_start(18);
        content.set_margin_end(18);
        content.append(&top_row);
        content.append(&grid);
        content.append(&rating_col);
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
