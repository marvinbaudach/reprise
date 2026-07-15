//! Redesigned tag editor dialog matching mockups 2g (single), 3a (multi),
//! 4a (autocomplete). Cover art display, prev/next navigation, mixed-field
//! UX with field annotations, pending-change bar, clickable rating stars,
//! and Ctrl+S save. The dialog receives raw track data and computes the
//! summary internally.
//!
//! The new `present_redesigned()` function coexists with the legacy
//! `present()` until Task 4 switches the flow. Items only reachable from
//! the redesigned path are allowed-dead-code until then.

#[allow(unused_imports)] // Path, PathBuf, cover, etc. used by present_redesigned
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use rusqlite::Connection;

use reprise_core::cover::{self, ThumbnailSize};
use reprise_core::library::tag_edit::{
    summarize, summarize_values, EditableTags, EditableTagSummary, MixedValue, TagPatch,
    TrackEditPatch,
};
use reprise_core::queries::autocomplete::AutocompleteColumn;

use crate::ui::autocomplete_entry::AutocompleteEntry;
use crate::ui::strings;

// ── Pure-logic helpers (unchanged from v1, exercised by the tests below) ─────

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

fn rating_choice_labels(value: &MixedValue<i32>) -> Vec<String> {
    let mut labels = Vec::with_capacity(7);
    if matches!(value, MixedValue::Mixed) {
        labels.push(strings::text(strings::MULTIPLE_VALUES));
    }
    labels.push("\u{2606} \u{2014}".into());
    labels.extend((1..=RATING_MAX).map(|rating| format!("\u{2605} {rating}")));
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

// ── Navigation direction ─────────────────────────────────────────────────────

/// Task 4 will reference this from the flow; until then it appears unused.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigateDirection {
    Previous,
    Next,
}

// ── Field identity for dirty tracking ────────────────────────────────────────
//
// Only reachable from `present_redesigned`; Task 4 activates that path.
// Suppress dead-code warnings until then.

/// Indices into the `dirty` flags vector.
#[allow(dead_code)]
const FIELD_TITLE: usize = 0;
#[allow(dead_code)]
const FIELD_ARTIST: usize = 1;
#[allow(dead_code)]
const FIELD_ALBUM: usize = 2;
#[allow(dead_code)]
const FIELD_ALBUM_ARTIST: usize = 3;
#[allow(dead_code)]
const FIELD_YEAR: usize = 4;
#[allow(dead_code)]
const FIELD_TRACK_NO: usize = 5;
#[allow(dead_code)]
const FIELD_GENRE: usize = 6;
#[allow(dead_code)]
const FIELD_RATING: usize = 7;
#[allow(dead_code)]
const FIELD_COUNT: usize = 8;

/// Human-readable names for the pending-change bar, indexed by `FIELD_*`.
#[allow(dead_code)]
const FIELD_NAMES: [&str; FIELD_COUNT] = [
    "Title",
    "Artist",
    "Album",
    "Album Artist",
    "Year",
    "Track",
    "Genre",
    "Rating",
];

// ── Star glyphs ──────────────────────────────────────────────────────────────

#[allow(dead_code)]
const STAR_FILLED: &str = "\u{2605}";
#[allow(dead_code)]
const STAR_OUTLINE: &str = "\u{2606}";

// ── Snapshot for revert ──────────────────────────────────────────────────────

#[allow(dead_code)]
struct FieldSnapshot {
    summary: EditableTagSummary,
    rating: MixedValue<i32>,
}

// ── Old present() kept as deprecated wrapper ─────────────────────────────────

/// Legacy signature. Task 4 will replace every call site with the new one
/// and delete this wrapper.
pub fn present(
    parent: &adw::ApplicationWindow,
    summary: &EditableTagSummary,
    rating_summary: &MixedValue<i32>,
    track_count: usize,
    on_apply: impl Fn(TrackEditPatch) + 'static,
) {
    present_legacy(parent, summary, rating_summary, track_count, on_apply);
}

fn present_legacy(
    parent: &adw::ApplicationWindow,
    summary: &EditableTagSummary,
    rating_summary: &MixedValue<i32>,
    track_count: usize,
    on_apply: impl Fn(TrackEditPatch) + 'static,
) {
    // Minimal re-implementation of the old UI so the old flow still works
    // until Task 4 switches to present_redesigned.
    let is_multi = track_count > 1;

    let title = string_row(
        &strings::text(strings::TAG_TITLE),
        &summary.title,
        track_count,
    );
    let artist = string_row(
        &strings::text(strings::TAG_ARTIST),
        &summary.artist,
        track_count,
    );
    let album = string_row(
        &strings::text(strings::TAG_ALBUM),
        &summary.album,
        track_count,
    );
    let album_artist = string_row(
        &strings::text(strings::TAG_ALBUM_ARTIST),
        &summary.album_artist,
        track_count,
    );
    let year = number_row(
        &strings::text(strings::TAG_YEAR),
        &summary.year,
        track_count,
    );
    let track_no = number_row(
        &strings::text(strings::TAG_TRACK_NUMBER),
        &summary.track_no,
        track_count,
    );
    let genre = string_row(
        &strings::text(strings::TAG_GENRE),
        &summary.genre,
        track_count,
    );
    let (rating, rating_started_mixed) = legacy_rating_row(rating_summary);

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
    if is_multi {
        let hint = gtk4::Label::builder()
            .label(strings::tag_applied_to_all_hint(track_count))
            .xalign(0.0)
            .wrap(true)
            .build();
        hint.add_css_class("reprise-tag-hint");
        content.append(&hint);
    }
    content.append(&error_label);
    let scrolled = gtk4::ScrolledWindow::builder()
        .child(&content)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .build();

    let apply = gtk4::Button::with_label(&strings::text(strings::APPLY));
    apply.add_css_class("suggested-action");
    apply.set_sensitive(false);
    let header = legacy_editor_header(&apply);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&scrolled));
    let dialog = adw::Dialog::builder()
        .child(&toolbar)
        .content_width(520)
        .content_height(590)
        .build();
    dialog.add_css_class("reprise-tag-editor");

    dialog.set_default_widget(Some(&apply));
    for row in [
        &title,
        &artist,
        &album,
        &album_artist,
        &year,
        &track_no,
        &genre,
    ] {
        row.set_activates_default(true);
    }

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
        let dirty_flag = flag.clone();
        let all_dirty = dirty.clone();
        let apply_btn = apply.clone();
        row.connect_changed(move |_| {
            dirty_flag.set(true);
            apply_btn.set_sensitive(all_dirty.iter().any(|f| f.get()));
        });
    }
    {
        let rating_dirty = dirty[7].clone();
        let all_dirty = dirty.clone();
        let apply_btn = apply.clone();
        rating.connect_selected_notify(move |_| {
            rating_dirty.set(true);
            apply_btn.set_sensitive(all_dirty.iter().any(|f| f.get()));
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

fn badge_label(is_mixed: bool, track_count: usize) -> Option<gtk4::Label> {
    if track_count <= 1 {
        return None;
    }
    let text = if is_mixed {
        strings::text(strings::MULTIPLE_VALUES)
    } else {
        strings::text(strings::TAG_SAME_ON_ALL)
    };
    let label = gtk4::Label::new(Some(&text));
    label.add_css_class("reprise-tag-badge");
    Some(label)
}

fn string_row(label: &str, value: &MixedValue<String>, track_count: usize) -> adw::EntryRow {
    let title = match value {
        MixedValue::Uniform(_) => label.to_string(),
        MixedValue::Mixed => format!("{label} \u{2014} {}", &strings::text(strings::MULTIPLE_VALUES)),
    };
    let row = adw::EntryRow::builder().title(title).build();
    if let MixedValue::Uniform(value) = value {
        row.set_text(value);
    }
    if let Some(badge) = badge_label(matches!(value, MixedValue::Mixed), track_count) {
        row.add_suffix(&badge);
    }
    row
}

fn number_row(label: &str, value: &MixedValue<Option<u32>>, track_count: usize) -> adw::EntryRow {
    let title = match value {
        MixedValue::Mixed => format!("{label} \u{2014} {}", &strings::text(strings::MULTIPLE_VALUES)),
        MixedValue::Uniform(_) => label.to_string(),
    };
    let row = adw::EntryRow::builder()
        .title(title)
        .input_purpose(gtk4::InputPurpose::Digits)
        .build();
    if let MixedValue::Uniform(Some(value)) = value {
        row.set_text(&value.to_string());
    }
    if let Some(badge) = badge_label(matches!(value, MixedValue::Mixed), track_count) {
        row.add_suffix(&badge);
    }
    row
}

fn legacy_rating_row(value: &MixedValue<i32>) -> (adw::ComboRow, bool) {
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

fn legacy_editor_header(apply: &gtk4::Button) -> adw::HeaderBar {
    let header = adw::HeaderBar::new();
    header.pack_end(apply);
    header.set_title_widget(Some(&adw::WindowTitle::new(
        &strings::text(strings::EDIT_TAGS),
        "",
    )));
    header
}

// ══════════════════════════════════════════════════════════════════════════════
//  REDESIGNED DIALOG (new signature — Task 4 will switch callers)
// ══════════════════════════════════════════════════════════════════════════════

#[allow(dead_code)]
pub fn present_redesigned(
    parent: &adw::ApplicationWindow,
    conn: Rc<RefCell<Connection>>,
    tracks: Vec<(i64, PathBuf)>,
    tags: Vec<EditableTags>,
    ratings: Vec<i32>,
    on_apply: impl Fn(TrackEditPatch) + Clone + 'static,
    on_navigate: impl Fn(NavigateDirection) -> bool + 'static,
) {
    let track_count = tracks.len();
    let is_multi = track_count > 1;
    let summary = summarize(&tags).unwrap();
    let rating_summary = summarize_values(&ratings).unwrap();
    let _snapshot = Rc::new(FieldSnapshot {
        summary: summary.clone(),
        rating: rating_summary.clone(),
    });

    // ── Header ───────────────────────────────────────────────────────────

    let save_label = if is_multi {
        strings::tag_save_count(track_count)
    } else {
        strings::text(strings::TAG_SAVE)
    };
    let save_btn = gtk4::Button::with_label(&save_label);
    save_btn.add_css_class("suggested-action");
    save_btn.set_sensitive(false);

    let cancel_btn = gtk4::Button::with_label(&strings::text(strings::CANCEL));

    let dialog_title = if is_multi {
        strings::tag_edit_title_multi(track_count)
    } else {
        strings::text(strings::TAG_EDIT_TITLE_SINGLE)
    };
    let subtitle = if !is_multi {
        // Single-track subtitle: position info is added by Task 4
        String::new()
    } else {
        String::new()
    };

    let title_widget = adw::WindowTitle::new(&dialog_title, &subtitle);
    let header = adw::HeaderBar::new();
    header.pack_start(&cancel_btn);
    header.pack_end(&save_btn);
    header.set_title_widget(Some(&title_widget));
    header.set_show_start_title_buttons(false);
    header.set_show_end_title_buttons(false);

    // ── Cover art ────────────────────────────────────────────────────────

    let cover_area = build_cover_area(&tracks, is_multi);

    // ── Form fields ──────────────────────────────────────────────────────

    // Title: per-track in multi mode (read-only)
    let title_row = adw::EntryRow::builder()
        .title(strings::text(strings::TAG_TITLE))
        .build();
    if is_multi {
        title_row.set_editable(false);
        title_row.add_css_class("reprise-tag-mixed");
        add_annotation(&title_row, &strings::text(strings::TAG_PER_TRACK), false);
    }
    set_entry_from_mixed_string(&title_row, &summary.title);

    // Artist (autocomplete)
    let artist_ac = AutocompleteEntry::new(
        &strings::text(strings::TAG_ARTIST),
        AutocompleteColumn::Artist,
        conn.clone(),
    );
    init_autocomplete_from_mixed(&artist_ac, &summary.artist, track_count, is_multi);

    // Album (autocomplete)
    let album_ac = AutocompleteEntry::new(
        &strings::text(strings::TAG_ALBUM),
        AutocompleteColumn::Album,
        conn.clone(),
    );
    init_autocomplete_from_mixed(&album_ac, &summary.album, track_count, is_multi);

    // Album artist (autocomplete, with placeholder)
    let album_artist_ac = AutocompleteEntry::new(
        &strings::text(strings::TAG_ALBUM_ARTIST),
        AutocompleteColumn::AlbumArtist,
        conn.clone(),
    );
    init_autocomplete_from_mixed(
        &album_artist_ac,
        &summary.album_artist,
        track_count,
        is_multi,
    );

    // Genre (autocomplete)
    let genre_ac = AutocompleteEntry::new(
        &strings::text(strings::TAG_GENRE),
        AutocompleteColumn::Genre,
        conn.clone(),
    );
    init_autocomplete_from_mixed(&genre_ac, &summary.genre, track_count, is_multi);

    // Year
    let year_row = adw::EntryRow::builder()
        .title(strings::text(strings::TAG_YEAR))
        .input_purpose(gtk4::InputPurpose::Digits)
        .build();
    set_entry_from_mixed_number(&year_row, &summary.year);
    if is_multi {
        apply_mixed_annotation_number(&year_row, &summary.year, track_count);
    }

    // Track number: per-track in multi mode (read-only)
    let track_no_row = adw::EntryRow::builder()
        .title(strings::text(strings::TAG_TRACK_NUMBER))
        .input_purpose(gtk4::InputPurpose::Digits)
        .build();
    if is_multi {
        track_no_row.set_editable(false);
        track_no_row.add_css_class("reprise-tag-mixed");
        add_annotation(
            &track_no_row,
            &strings::text(strings::TAG_PER_TRACK),
            false,
        );
    }
    set_entry_from_mixed_number(&track_no_row, &summary.track_no);

    // ── Rating stars ─────────────────────────────────────────────────────

    let (rating_box, rating_value) = build_star_rating(&rating_summary);

    // ── Layout assembly ──────────────────────────────────────────────────

    let group = adw::PreferencesGroup::new();
    group.add(&title_row);
    group.add(artist_ac.row());
    group.add(album_ac.row());
    group.add(album_artist_ac.row());
    group.add(&genre_ac.row().clone());
    group.add(&year_row);
    group.add(&track_no_row);

    // Rating row: wrap the star box in an ActionRow for consistent layout
    let rating_action_row = adw::ActionRow::builder()
        .title(strings::text(strings::RATING))
        .build();
    rating_action_row.add_suffix(&rating_box);
    group.add(&rating_action_row);

    // Error label
    let error_label = gtk4::Label::builder()
        .label(strings::text(strings::TAG_NUMBER_ERROR))
        .css_classes(["reprise-tag-error"])
        .visible(false)
        .wrap(true)
        .xalign(0.0)
        .build();

    // Pending-change bar (multi-track only)
    let pending_bar = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    pending_bar.add_css_class("reprise-tag-pending");
    pending_bar.set_visible(false);

    // MusicBrainz button
    let mb_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    mb_box.add_css_class("reprise-tag-mb");
    let mb_btn =
        gtk4::Button::with_label(&strings::text(strings::TAG_FETCH_MUSICBRAINZ));
    mb_btn.set_sensitive(false); // Task 5 wires this
    mb_box.append(&mb_btn);

    // Navigation buttons (only useful in single-track mode within a multi-selection)
    let nav_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    nav_box.add_css_class("reprise-tag-nav");
    nav_box.set_halign(gtk4::Align::Center);
    let prev_btn = gtk4::Button::from_icon_name("go-previous-symbolic");
    prev_btn.set_tooltip_text(Some(&strings::text(strings::PREVIOUS)));
    let next_btn = gtk4::Button::from_icon_name("go-next-symbolic");
    next_btn.set_tooltip_text(Some(&strings::text(strings::NEXT)));
    nav_box.append(&prev_btn);
    nav_box.append(&next_btn);
    // Navigation hidden until Task 4 provides the on_navigate wiring
    nav_box.set_visible(false);

    // Main content column
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

    // ── Dirty tracking ───────────────────────────────────────────────────

    let dirty: Vec<Rc<Cell<bool>>> = (0..FIELD_COUNT)
        .map(|_| Rc::new(Cell::new(false)))
        .collect();

    // Helper: update save-button sensitivity + pending bar
    let update_save_state = {
        let dirty = dirty.clone();
        let save_btn = save_btn.clone();
        let pending_bar = pending_bar.clone();
        let is_multi = is_multi;

        // Capture field accessors for pending bar text
        let title_row_c = title_row.clone();
        let year_row_c = year_row.clone();
        let track_no_row_c = track_no_row.clone();
        let artist_ac_row = artist_ac.row().clone();
        let album_ac_row = album_ac.row().clone();
        let album_artist_ac_row = album_artist_ac.row().clone();
        let genre_ac_row = genre_ac.row().clone();
        let rating_value_c = rating_value.clone();

        Rc::new(move || {
            let any_dirty = dirty.iter().any(|f| f.get());
            save_btn.set_sensitive(any_dirty);

            if is_multi {
                // Rebuild pending bar
                while let Some(child) = pending_bar.first_child() {
                    pending_bar.remove(&child);
                }

                let dirty_count = dirty.iter().filter(|f| f.get()).count();
                if dirty_count > 0 {
                    let header_label = gtk4::Label::builder()
                        .label(strings::tag_pending_count(dirty_count))
                        .xalign(0.0)
                        .build();
                    header_label.add_css_class("reprise-tag-pending-header");
                    pending_bar.append(&header_label);

                    let rows: [(&adw::EntryRow, usize); 7] = [
                        (&title_row_c, FIELD_TITLE),
                        (&artist_ac_row, FIELD_ARTIST),
                        (&album_ac_row, FIELD_ALBUM),
                        (&album_artist_ac_row, FIELD_ALBUM_ARTIST),
                        (&year_row_c, FIELD_YEAR),
                        (&track_no_row_c, FIELD_TRACK_NO),
                        (&genre_ac_row, FIELD_GENRE),
                    ];

                    for (row, idx) in rows {
                        if dirty[idx].get() {
                            let item = build_pending_item(
                                FIELD_NAMES[idx],
                                &row.text(),
                            );
                            pending_bar.append(&item);
                        }
                    }

                    if dirty[FIELD_RATING].get() {
                        let rating_text = format!(
                            "{STAR_FILLED} {}",
                            rating_value_c.get()
                        );
                        let item = build_pending_item(FIELD_NAMES[FIELD_RATING], &rating_text);
                        pending_bar.append(&item);
                    }

                    pending_bar.set_visible(true);
                } else {
                    pending_bar.set_visible(false);
                }
            }
        })
    };

    // Wire entry-row changed signals
    let update_save_state: Rc<dyn Fn()> = update_save_state;

    let wire_entry_dirty =
        |row: &adw::EntryRow, field_idx: usize, update: &Rc<dyn Fn()>| {
            let dirty_flag = dirty[field_idx].clone();
            let update = update.clone();
            row.connect_changed(move |_| {
                dirty_flag.set(true);
                update();
            });
        };

    wire_entry_dirty(&title_row, FIELD_TITLE, &update_save_state);
    wire_entry_dirty(&year_row, FIELD_YEAR, &update_save_state);
    wire_entry_dirty(&track_no_row, FIELD_TRACK_NO, &update_save_state);

    // Wire autocomplete changed signals
    {
        let dirty_flag = dirty[FIELD_ARTIST].clone();
        let update = update_save_state.clone();
        artist_ac.connect_changed(move || {
            dirty_flag.set(true);
            update();
        });
    }
    {
        let dirty_flag = dirty[FIELD_ALBUM].clone();
        let update = update_save_state.clone();
        album_ac.connect_changed(move || {
            dirty_flag.set(true);
            update();
        });
    }
    {
        let dirty_flag = dirty[FIELD_ALBUM_ARTIST].clone();
        let update = update_save_state.clone();
        album_artist_ac.connect_changed(move || {
            dirty_flag.set(true);
            update();
        });
    }
    {
        let dirty_flag = dirty[FIELD_GENRE].clone();
        let update = update_save_state.clone();
        genre_ac.connect_changed(move || {
            dirty_flag.set(true);
            update();
        });
    }

    // Wire rating star clicks
    {
        let dirty_flag = dirty[FIELD_RATING].clone();
        let update = update_save_state.clone();
        let rating_value_c = rating_value.clone();
        wire_star_clicks(
            &rating_box,
            &rating_value_c,
            Rc::new(move || {
                dirty_flag.set(true);
                update();
            }),
        );
    }

    // ── Save action ──────────────────────────────────────────────────────

    // Move the autocomplete entries into Rc so they survive closures. We
    // need to keep them alive for the dialog's lifetime anyway (they own
    // the popover lifecycle via Drop).
    let artist_ac = Rc::new(artist_ac);
    let album_ac = Rc::new(album_ac);
    let album_artist_ac = Rc::new(album_artist_ac);
    let genre_ac = Rc::new(genre_ac);

    let do_save = {
        let dirty = dirty.clone();
        let title_row = title_row.clone();
        let year_row = year_row.clone();
        let track_no_row = track_no_row.clone();
        let artist_ac = artist_ac.clone();
        let album_ac = album_ac.clone();
        let album_artist_ac = album_artist_ac.clone();
        let genre_ac = genre_ac.clone();
        let rating_value = rating_value.clone();
        let _rating_summary_clone = rating_summary.clone();
        let error_label = error_label.clone();
        let dialog = dialog.clone();
        let on_apply = on_apply.clone();

        Rc::new(move || {
            let year_p = number_patch(dirty[FIELD_YEAR].get(), year_row.text().as_str());
            let track_p = number_patch(
                dirty[FIELD_TRACK_NO].get(),
                track_no_row.text().as_str(),
            );
            let (Ok(year_p), Ok(track_p)) = (year_p, track_p) else {
                year_row.add_css_class("error");
                track_no_row.add_css_class("error");
                error_label.set_visible(true);
                tracing::debug!("tag editor rejected an invalid year or track number");
                return;
            };

            let rating_patch = if dirty[FIELD_RATING].get() {
                let val = rating_value.get();
                Some(val)
            } else {
                None
            };

            let patch = TrackEditPatch {
                tags: TagPatch {
                    title: string_patch(dirty[FIELD_TITLE].get(), title_row.text().as_str()),
                    artist: string_patch(dirty[FIELD_ARTIST].get(), &artist_ac.text()),
                    album: string_patch(dirty[FIELD_ALBUM].get(), &album_ac.text()),
                    album_artist: string_patch(
                        dirty[FIELD_ALBUM_ARTIST].get(),
                        &album_artist_ac.text(),
                    ),
                    year: year_p,
                    track_no: track_p,
                    genre: string_patch(dirty[FIELD_GENRE].get(), &genre_ac.text()),
                },
                rating: rating_patch,
            };
            on_apply(patch);
            dialog.close();
        })
    };

    // Save button click
    {
        let do_save = do_save.clone();
        save_btn.connect_clicked(move |_| do_save());
    }

    // ── Ctrl+S shortcut ──────────────────────────────────────────────────

    {
        let do_save = do_save.clone();
        let save_btn = save_btn.clone();
        let key_controller = gtk4::EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, keyval, _, modifier| {
            if keyval == gdk::Key::s
                && modifier.contains(gdk::ModifierType::CONTROL_MASK)
                && save_btn.is_sensitive()
            {
                do_save();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        dialog.add_controller(key_controller);
    }

    // ── Cancel / Esc with unsaved-changes confirmation ───────────────────

    let confirm_discard = {
        let dirty = dirty.clone();
        let dialog = dialog.clone();
        let do_save = do_save.clone();
        let _dialog_title = dialog_title.clone();

        Rc::new(move || {
            let any_dirty = dirty.iter().any(|f| f.get());
            if !any_dirty {
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

            let dialog_c = dialog.clone();
            let do_save_c = do_save.clone();
            alert.connect_response(None, move |_, response| match response {
                "save" => do_save_c(),
                "discard" => {
                    dialog_c.close();
                }
                _ => {} // "cancel" — do nothing, stay in dialog
            });
            alert.present(Some(&dialog));
        })
    };

    {
        let confirm_discard = confirm_discard.clone();
        cancel_btn.connect_clicked(move |_| confirm_discard());
    }

    // Intercept Esc to show confirmation when dirty
    {
        let confirm_discard = confirm_discard.clone();
        let dirty = dirty.clone();
        let dialog_c = dialog.clone();
        let key_controller = gtk4::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gdk::Key::Escape {
                let any_dirty = dirty.iter().any(|f| f.get());
                if any_dirty {
                    confirm_discard();
                    glib::Propagation::Stop
                } else {
                    // Let the default close happen
                    glib::Propagation::Proceed
                }
            } else {
                glib::Propagation::Proceed
            }
        });
        dialog_c.add_controller(key_controller);
    }

    // ── Navigation button callbacks (slots for Task 4) ───────────────────

    {
        let on_navigate = Rc::new(on_navigate);
        {
            let on_nav = on_navigate.clone();
            prev_btn.connect_clicked(move |_| {
                on_nav(NavigateDirection::Previous);
            });
        }
        {
            let on_nav = on_navigate.clone();
            next_btn.connect_clicked(move |_| {
                on_nav(NavigateDirection::Next);
            });
        }
    }

    // ── Enter activates save from entry rows ─────────────────────────────

    dialog.set_default_widget(Some(&save_btn));
    title_row.set_activates_default(true);
    year_row.set_activates_default(true);
    track_no_row.set_activates_default(true);

    dialog.present(Some(parent));
    tracing::debug!(track_count, is_multi, "redesigned tag editor presented");
}

// ══════════════════════════════════════════════════════════════════════════════
//  WIDGET BUILDERS
// ══════════════════════════════════════════════════════════════════════════════

/// Builds the cover art area. For single track, shows a thumbnail. For
/// multi-track, shows a stacked representation with a count badge.
#[allow(dead_code)]
fn build_cover_area(tracks: &[(i64, PathBuf)], is_multi: bool) -> gtk4::Box {
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
        let badge = gtk4::Label::new(Some(&format!("{} tracks", tracks.len())));
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
#[allow(dead_code)]
fn load_cover_picture(track_path: Option<&Path>) -> gtk4::Box {
    let frame = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    frame.add_css_class("reprise-tag-cover");
    frame.set_overflow(gtk4::Overflow::Hidden);

    let picture = gtk4::Picture::new();
    picture.set_content_fit(gtk4::ContentFit::Cover);
    picture.set_can_shrink(true);

    let loaded = track_path
        .and_then(|path| cover::resolve_source(path))
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
#[allow(dead_code)]
fn build_star_rating(value: &MixedValue<i32>) -> (gtk4::Box, Rc<Cell<i32>>) {
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
#[allow(dead_code)]
fn update_star_display(container: &gtk4::Box, rating: i32) {
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
#[allow(dead_code)]
fn wire_star_clicks(
    container: &gtk4::Box,
    rating_value: &Rc<Cell<i32>>,
    on_changed: Rc<dyn Fn()>,
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
#[allow(dead_code)]
fn set_entry_from_mixed_string(row: &adw::EntryRow, value: &MixedValue<String>) {
    match value {
        MixedValue::Uniform(text) => row.set_text(text),
        MixedValue::Mixed => {
            // Leave empty; the placeholder/annotation conveys the state
        }
    }
}

/// Sets an `EntryRow` text from a `MixedValue<Option<u32>>`.
#[allow(dead_code)]
fn set_entry_from_mixed_number(row: &adw::EntryRow, value: &MixedValue<Option<u32>>) {
    match value {
        MixedValue::Uniform(Some(n)) => row.set_text(&n.to_string()),
        MixedValue::Uniform(None) | MixedValue::Mixed => {}
    }
}

/// Initialises an `AutocompleteEntry` from a `MixedValue`, adding
/// mixed-field annotations in multi-track mode.
#[allow(dead_code)]
fn init_autocomplete_from_mixed(
    ac: &AutocompleteEntry,
    value: &MixedValue<String>,
    _track_count: usize,
    is_multi: bool,
) {
    match value {
        MixedValue::Uniform(text) => {
            ac.set_text(text);
            if is_multi {
                add_annotation(
                    ac.row(),
                    &strings::text(strings::TAG_SAME_ON_ALL),
                    false,
                );
            }
        }
        MixedValue::Mixed => {
            if is_multi {
                ac.row().add_css_class("reprise-tag-mixed");
                add_annotation(
                    ac.row(),
                    &strings::text(strings::MULTIPLE_VALUES),
                    false,
                );
            }
        }
    }
}

/// Adds a mixed-field annotation for number fields in multi-track mode.
#[allow(dead_code)]
fn apply_mixed_annotation_number(
    row: &adw::EntryRow,
    value: &MixedValue<Option<u32>>,
    _track_count: usize,
) {
    match value {
        MixedValue::Uniform(_) => {
            add_annotation(row, &strings::text(strings::TAG_SAME_ON_ALL), false);
        }
        MixedValue::Mixed => {
            row.add_css_class("reprise-tag-mixed");
            add_annotation(
                row,
                &strings::text(strings::MULTIPLE_VALUES),
                false,
            );
        }
    }
}

/// Adds a small annotation label as a suffix to an `EntryRow`.
#[allow(dead_code)]
fn add_annotation(row: &adw::EntryRow, text: &str, accent: bool) {
    let label = gtk4::Label::new(Some(text));
    label.add_css_class("reprise-tag-field-annotation");
    if accent {
        label.add_css_class("accent");
    }
    row.add_suffix(&label);
}

/// Builds a single pending-change item: "Field -> Value" with layout.
#[allow(dead_code)]
fn build_pending_item(field_name: &str, value: &str) -> gtk4::Box {
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

    item
}

// ══════════════════════════════════════════════════════════════════════════════
//  TESTS
// ══════════════════════════════════════════════════════════════════════════════

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

    #[test]
    fn rating_choices_keep_mixed_unrated_and_five_stars_distinct() {
        assert_eq!(
            rating_choice_labels(&MixedValue::Mixed),
            vec![
                "(multiple values)",
                "\u{2606} \u{2014}",
                "\u{2605} 1",
                "\u{2605} 2",
                "\u{2605} 3",
                "\u{2605} 4",
                "\u{2605} 5"
            ]
        );
        assert_eq!(rating_from_selection(true, 0), None);
        assert_eq!(rating_from_selection(true, 1), Some(0));
        assert_eq!(rating_from_selection(true, 6), Some(5));
        assert_eq!(rating_from_selection(false, 0), Some(0));
        assert_eq!(rating_from_selection(false, 5), Some(5));
    }

    #[test]
    fn navigate_direction_has_expected_variants() {
        let prev = NavigateDirection::Previous;
        let next = NavigateDirection::Next;
        assert_ne!(prev, next);
    }

    #[test]
    fn field_names_cover_all_fields() {
        assert_eq!(FIELD_NAMES.len(), FIELD_COUNT);
    }
}
