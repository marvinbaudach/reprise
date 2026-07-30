//! Tag-editor form composition and single/multi mode labels.
//!
//! F0 (TAG-2): fields are built directly from a [`TagEditSession`] rather
//! than the collapsed `EditableTagSummary` Package C had to work with —
//! `session.mixed_placeholder(field)` carries the per-track distinct values
//! a Mixed field needs to show ("Mixed — Ambient, Post-Rock" / "Mixed — 8
//! different values"), not just the fact that *something* differs. The
//! session is also the target every field's live edits write into
//! (`tag_editor_dirty::wire`), so this module only ever needs it for the
//! dialog's *initial* render.

use std::cell::{Cell, RefCell};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::db::Db;
use reprise_core::library::tag_edit::MixedValue;
use reprise_core::library::tag_edit_session::{TagEditSession, TagField};
use reprise_core::queries::autocomplete::AutocompleteColumn;

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
    /// Everything except the header (cover/fields/rating/error label/review
    /// footer/MusicBrainz box/nav row) — disabled wholesale while a save is
    /// in flight (F2) so no field can be edited mid-write.
    pub(in crate::ui) content: gtk4::Box,
    pub(in crate::ui) title_row: adw::EntryRow,
    pub(in crate::ui) artist_ac: Rc<AutocompleteEntry>,
    pub(in crate::ui) album_ac: Rc<AutocompleteEntry>,
    pub(in crate::ui) album_artist_ac: Rc<AutocompleteEntry>,
    pub(in crate::ui) genre_ac: Rc<AutocompleteEntry>,
    pub(in crate::ui) year_row: adw::EntryRow,
    pub(in crate::ui) track_no_row: adw::EntryRow,
    pub(in crate::ui) rating_box: gtk4::Box,
    pub(in crate::ui) rating_value: Rc<Cell<i32>>,
    pub(in crate::ui) artist_annotation: Option<gtk4::Label>,
    pub(in crate::ui) album_annotation: Option<gtk4::Label>,
    pub(in crate::ui) album_artist_annotation: Option<gtk4::Label>,
    pub(in crate::ui) genre_annotation: Option<gtk4::Label>,
    pub(in crate::ui) year_annotation: Option<gtk4::Label>,
    pub(in crate::ui) rating_annotation: Option<gtk4::Label>,
    pub(in crate::ui) error_label: gtk4::Label,
    /// The reserved "was: …" line under each field (TAG-5, P-4) — built by
    /// `build_field_column` for every field including Rating, populated by
    /// `tag_editor_dirty::wire`'s `update()` from `TagEditSession::
    /// old_value_line`. Empty text keeps the reserved space allocated but
    /// invisible-in-effect (never removed from the layout).
    pub(in crate::ui) old_value_labels: Vec<(TagField, gtk4::Label)>,
    /// Review-footer mount point (F1 builds its summary/expander contents
    /// here on every `tag_editor_dirty::wire`'s `update()`, the same
    /// pattern the pre-F0 pending bar used) — always present, visibility
    /// decided by session state rather than static mode (TAG-5: the
    /// expander also applies in SingleNav once browsing accumulates pending
    /// tracks, Package G).
    pub(in crate::ui) review_box: gtk4::Box,
    pub(in crate::ui) prev_btn: gtk4::Button,
    pub(in crate::ui) next_btn: gtk4::Button,
    /// G1 (TAG-4): exposed so `tag_editor.rs` can prepend the browse
    /// position ("Track 3 of 12") once it knows the snapshot — this module
    /// itself never learns about the snapshot, only format/bitrate (see
    /// `format_track_subtitle`'s doc comment).
    pub(in crate::ui) title_widget: adw::WindowTitle,
}

impl TagEditorForm {
    pub(in crate::ui) fn build(
        mode: EditorMode,
        conn: &Rc<Db>,
        tracks: &[(i64, PathBuf)],
        bitrates: &[Option<u32>],
        session: &Rc<RefCell<TagEditSession>>,
    ) -> Self {
        let is_multi = mode.is_multi();
        let track_count = mode.track_count();
        let save_btn = gtk4::Button::with_label(&strings::text(strings::TAG_SAVE));
        save_btn.add_css_class("suggested-action");
        save_btn.set_sensitive(false);
        let cancel_btn = gtk4::Button::with_label(&strings::text(strings::CANCEL));

        // Header subtitle (Beschluss #2): Multi explains the batch-write
        // scope; Single renders "FORMAT · bitrate" from the file extension
        // and `bitrate_kbps` (F0 threads both all the way from the
        // selection). The "Track N of M" position prefix is Package G's job
        // (TAG-4 browse snapshot) — space stays reserved by leaving it out
        // rather than fabricating it.
        let subtitle = if is_multi {
            strings::text(strings::TAG_SUBTITLE_MULTI)
        } else {
            let extension = tracks
                .first()
                .and_then(|(_, path)| path.extension())
                .and_then(|ext| ext.to_str());
            let bitrate = bitrates.first().copied().flatten();
            format_track_subtitle(extension, bitrate).unwrap_or_default()
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

        let session_ref = session.borrow();
        let current_id = session_ref.current_track_id();

        let title_row = adw::EntryRow::builder()
            .title(strings::text(strings::TAG_TITLE))
            .build();
        apply_per_track_field(&title_row, is_multi);
        if !is_multi {
            set_entry_from_mixed_string(
                &title_row,
                &text_bridge(&session_ref, TagField::Title, current_id),
            );
        }

        let artist_ac = Rc::new(AutocompleteEntry::new(
            &strings::text(strings::TAG_ARTIST),
            AutocompleteColumn::Artist,
            conn.clone(),
        ));
        let artist_annotation = init_field(
            &artist_ac,
            &session_ref,
            TagField::Artist,
            track_count,
            is_multi,
            current_id,
        );

        let album_ac = Rc::new(AutocompleteEntry::new(
            &strings::text(strings::TAG_ALBUM),
            AutocompleteColumn::Album,
            conn.clone(),
        ));
        let album_annotation = init_field(
            &album_ac,
            &session_ref,
            TagField::Album,
            track_count,
            is_multi,
            current_id,
        );

        let (title_col, title_old_value) = build_field_column(title_row.upcast_ref(), None);
        let (artist_col, artist_old_value) = build_field_column(artist_ac.row().upcast_ref(), None);
        let (album_col, album_old_value) = build_field_column(album_ac.row().upcast_ref(), None);

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
        let album_artist_annotation = init_field(
            &album_artist_ac,
            &session_ref,
            TagField::AlbumArtist,
            track_count,
            is_multi,
            current_id,
        );

        let genre_ac = Rc::new(AutocompleteEntry::new(
            &strings::text(strings::TAG_GENRE),
            AutocompleteColumn::Genre,
            conn.clone(),
        ));
        let genre_annotation = init_field(
            &genre_ac,
            &session_ref,
            TagField::Genre,
            track_count,
            is_multi,
            current_id,
        );

        let year_row = adw::EntryRow::builder()
            .title(strings::text(strings::TAG_YEAR))
            .input_purpose(gtk4::InputPurpose::Digits)
            .build();
        let year_number = number_bridge(&session_ref, TagField::Year, current_id);
        set_entry_from_mixed_number(&year_row, &year_number);
        let year_annotation = is_multi
            .then(|| apply_mixed_annotation_number(&year_row, &year_number, track_count))
            .flatten();
        if year_annotation.is_some() {
            attach_type_to_arm(&year_row, year_annotation.as_ref(), track_count);
        }
        apply_mixed_field_presentation(
            &year_row,
            year_annotation.as_ref(),
            mixed_field_presentation(&session_ref, TagField::Year).as_ref(),
        );

        let track_no_row = adw::EntryRow::builder()
            .title(strings::text(strings::TAG_TRACK_NUMBER))
            .input_purpose(gtk4::InputPurpose::Digits)
            .build();
        apply_per_track_field(&track_no_row, is_multi);
        if !is_multi {
            set_entry_from_mixed_number(
                &track_no_row,
                &number_bridge(&session_ref, TagField::TrackNo, current_id),
            );
        }

        let (album_artist_col, album_artist_old_value) =
            build_field_column(album_artist_ac.row().upcast_ref(), None);
        let (genre_col, genre_old_value) = build_field_column(genre_ac.row().upcast_ref(), None);
        let (year_col, year_old_value) = build_field_column(year_row.upcast_ref(), None);
        let (track_no_col, track_no_old_value) =
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

        let rating_presentation = mixed_field_presentation(&session_ref, TagField::Rating);
        let rating_mixed = match rating_presentation {
            Some(_) => MixedValue::Mixed,
            None => {
                let rating = session_ref
                    .effective_display(current_id, TagField::Rating)
                    .and_then(|text| text.parse::<i32>().ok())
                    .unwrap_or(0);
                MixedValue::Uniform(rating)
            }
        };
        let (rating_box, rating_value) = build_star_rating(&rating_mixed);
        let (rating_col, rating_old_value) = build_field_column(rating_box.upcast_ref(), None);
        let rating_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let rating_label = gtk4::Label::builder()
            .label(strings::text(strings::RATING))
            .xalign(0.0)
            .hexpand(true)
            .build();
        rating_label.add_css_class("reprise-tag-field-label");
        rating_header.append(&rating_label);
        let rating_annotation = is_multi.then(|| {
            let text = rating_presentation.as_ref().map_or_else(
                || strings::text(strings::TAG_SAME_ON_ALL),
                |presentation| presentation.annotation.clone(),
            );
            let label = gtk4::Label::new(Some(&text));
            label.add_css_class("reprise-tag-field-annotation");
            rating_header.append(&label);
            label
        });
        rating_col.prepend(&rating_header);

        let error_label = gtk4::Label::builder()
            .label(strings::text(strings::TAG_NUMBER_ERROR))
            .css_classes(["reprise-tag-error"])
            .visible(false)
            .wrap(true)
            .xalign(0.0)
            .build();
        let review_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        review_box.add_css_class("reprise-tag-review");
        review_box.set_visible(false);

        let nav_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        nav_box.add_css_class("reprise-tag-nav");
        nav_box.set_halign(gtk4::Align::Center);
        let prev_btn = gtk4::Button::from_icon_name("go-previous-symbolic");
        prev_btn.set_tooltip_text(Some(&strings::text(strings::PREVIOUS)));
        let next_btn = gtk4::Button::from_icon_name("go-next-symbolic");
        next_btn.set_tooltip_text(Some(&strings::text(strings::NEXT)));
        nav_box.append(&prev_btn);
        nav_box.append(&next_btn);
        // G1 (TAG-4): hidden by default; `tag_editor.rs` reveals both buttons
        // once it knows whether a real >1-track browse snapshot exists (a
        // decision that lives outside this module's construction-time data).
        // Hiding each button rather than `nav_box` itself keeps this a single
        // line — no new field needs exposing on `TagEditorForm` for it.
        prev_btn.set_visible(false);
        next_btn.set_visible(false);

        drop(session_ref);

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 14);
        content.set_margin_top(12);
        content.set_margin_bottom(18);
        content.set_margin_start(18);
        content.set_margin_end(18);
        content.append(&top_row);
        content.append(&grid);
        content.append(&rating_col);
        content.append(&error_label);
        content.append(&review_box);
        if !is_multi {
            content.append(&nav_box);
        }
        // Size to content: propagate the content's natural height so the
        // dialog is as tall as it needs to be (no fixed height leaving empty
        // space below — which the removed MusicBrainz button used to fill),
        // capped so a large multi-edit with the review expander open scrolls
        // instead of growing without bound.
        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&content)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(760)
            .build();
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&scrolled));
        let dialog = adw::Dialog::builder()
            .child(&toolbar)
            .content_width(560)
            .build();
        dialog.add_css_class("reprise-tag-editor");

        let old_value_labels = vec![
            (TagField::Title, title_old_value),
            (TagField::Artist, artist_old_value),
            (TagField::Album, album_old_value),
            (TagField::AlbumArtist, album_artist_old_value),
            (TagField::Genre, genre_old_value),
            (TagField::Year, year_old_value),
            (TagField::TrackNo, track_no_old_value),
            (TagField::Rating, rating_old_value),
        ];

        Self {
            save_btn,
            cancel_btn,
            dialog,
            content,
            title_row,
            artist_ac,
            album_ac,
            album_artist_ac,
            genre_ac,
            year_row,
            track_no_row,
            rating_box,
            rating_value,
            artist_annotation,
            album_annotation,
            album_artist_annotation,
            genre_annotation,
            year_annotation,
            rating_annotation,
            error_label,
            old_value_labels,
            review_box,
            prev_btn,
            next_btn,
            title_widget,
        }
    }
}

/// Builds a `MixedValue<String>` bridge for widgets.rs's existing
/// `set_entry_from_mixed_string`/`init_autocomplete_from_mixed` helpers,
/// which still take the old collapsed type. The session-backed presentation
/// applied afterward supplies the rich in-entry placeholder and counter.
fn text_bridge(session: &TagEditSession, field: TagField, current_id: i64) -> MixedValue<String> {
    match session.mixed_placeholder(field) {
        Some(_) => MixedValue::Mixed,
        // A uniform field shows its own value. `effective_display` renders an
        // empty value as the "empty" sentinel (mixed-placeholder vocabulary),
        // so strip it back to a real blank — otherwise a track with no genre
        // shows the literal word "empty" in the field.
        None => MixedValue::Uniform(crate::ui::tag_editor::display_or_blank(
            session.effective_display(current_id, field),
        )),
    }
}

/// Same bridge as [`text_bridge`], for the two numeric fields (Year/
/// Track-number).
fn number_bridge(
    session: &TagEditSession,
    field: TagField,
    current_id: i64,
) -> MixedValue<Option<u32>> {
    match session.mixed_placeholder(field) {
        Some(_) => MixedValue::Mixed,
        None => {
            let value = session
                .effective_display(current_id, field)
                .and_then(|text| text.parse::<u32>().ok());
            MixedValue::Uniform(value)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ui) struct MixedFieldPresentation {
    pub(in crate::ui) entry_placeholder: String,
    pub(in crate::ui) annotation: String,
}

pub(in crate::ui) fn mixed_field_presentation(
    session: &TagEditSession,
    field: TagField,
) -> Option<MixedFieldPresentation> {
    session
        .mixed_placeholder(field)
        .map(|placeholder| MixedFieldPresentation {
            entry_placeholder: placeholder.label,
            annotation: strings::tag_distinct_value_count(placeholder.distinct_count),
        })
}

/// Initializes an autocomplete field's text/annotation from the session:
/// uniform values render as real text; Mixed fields stay blank (TAG-2: no
/// prefilled value, no click-to-unlock), show their rich label inside the
/// entry, and reserve the annotation for the distinct-value count.
fn init_field(
    ac: &AutocompleteEntry,
    session: &TagEditSession,
    field: TagField,
    track_count: usize,
    is_multi: bool,
    current_id: i64,
) -> Option<gtk4::Label> {
    let bridge = text_bridge(session, field, current_id);
    let annotation = init_autocomplete_from_mixed(ac, &bridge, track_count, is_multi);
    if is_multi && matches!(bridge, MixedValue::Mixed) {
        attach_type_to_arm(ac.row(), annotation.as_ref(), track_count);
    }
    apply_mixed_field_presentation(
        ac.row(),
        annotation.as_ref(),
        mixed_field_presentation(session, field).as_ref(),
    );
    annotation
}

pub(in crate::ui) fn apply_mixed_field_presentation(
    row: &adw::EntryRow,
    annotation: Option<&gtk4::Label>,
    presentation: Option<&MixedFieldPresentation>,
) {
    let Some(presentation) = presentation else {
        set_entry_placeholder(row, None);
        return;
    };
    set_entry_placeholder(row, Some(&presentation.entry_placeholder));
    row.remove_css_class("reprise-tag-field-armed");
    row.add_css_class("reprise-tag-mixed");
    if let Some(label) = annotation {
        label.set_text(&presentation.annotation);
        label.remove_css_class("accent");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::library::tag_edit::EditableTags;
    use reprise_core::library::tag_edit_session::{SessionMode, SessionTrack};
    use std::path::PathBuf;

    fn track(id: i64, genre: &str) -> SessionTrack {
        SessionTrack {
            id,
            path: PathBuf::from(format!("/music/{id}.flac")),
            tags: EditableTags {
                title: format!("Title {id}"),
                artist: "Artist".into(),
                album: "Album".into(),
                album_artist: "Artist".into(),
                year: Some(2020),
                track_no: Some(1),
                genre: genre.into(),
            },
            rating: 0,
        }
    }

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

    #[test]
    fn tag_2_uniform_empty_field_shows_a_real_blank_not_the_empty_sentinel() {
        // Both tracks lack a genre: a uniform (not mixed) empty value. The
        // entry must carry a real blank — never the literal "empty" sentinel,
        // which belongs only to the mixed-placeholder vocabulary ("Mixed —
        // Ambient, empty"). The browse-refresh path already strips it; the
        // initial build must too.
        let session = TagEditSession::new(vec![track(1, ""), track(2, "")], SessionMode::Multi);

        assert_eq!(
            text_bridge(&session, TagField::Genre, 1),
            MixedValue::Uniform(String::new()),
        );
    }

    #[test]
    fn tag_2_mixed_placeholder_sits_in_the_entry() {
        let session = TagEditSession::new(
            vec![track(1, "Ambient"), track(2, "Post-Rock")],
            SessionMode::Multi,
        );

        let presentation = mixed_field_presentation(&session, TagField::Genre).unwrap();

        assert_eq!(presentation.entry_placeholder, "Mixed — Ambient, Post-Rock");
    }

    #[test]
    fn tag_2_counter_annotation_shows_distinct_values() {
        let session = TagEditSession::new(
            vec![track(1, "Ambient"), track(2, "Post-Rock")],
            SessionMode::Multi,
        );

        let presentation = mixed_field_presentation(&session, TagField::Genre).unwrap();

        assert_eq!(presentation.annotation, "2 values");
        assert_eq!(strings::tag_distinct_value_count(1), "1 value");
    }

    #[test]
    fn tag_2_rich_placeholder_lists_values_from_session() {
        let session = TagEditSession::new(
            vec![track(1, "Ambient"), track(2, "Post-Rock")],
            SessionMode::Multi,
        );

        assert_eq!(
            mixed_field_presentation(&session, TagField::Genre)
                .map(|presentation| presentation.entry_placeholder),
            Some("Mixed — Ambient, Post-Rock".to_string()),
        );
    }

    #[test]
    fn tag_2_rich_placeholder_counts_three_or_more_values() {
        let session = TagEditSession::new(
            vec![track(1, "Ambient"), track(2, "Post-Rock"), track(3, "Jazz")],
            SessionMode::Multi,
        );

        assert_eq!(
            mixed_field_presentation(&session, TagField::Genre)
                .map(|presentation| presentation.entry_placeholder),
            Some("Mixed — 3 different values".to_string()),
        );
    }

    #[test]
    fn uniform_field_has_no_rich_placeholder() {
        let session = TagEditSession::new(
            vec![track(1, "Ambient"), track(2, "Ambient")],
            SessionMode::Multi,
        );

        assert_eq!(mixed_field_presentation(&session, TagField::Genre), None);
    }
}
