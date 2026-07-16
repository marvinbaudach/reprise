//! Tag editor dialog matching mockups 2g (single), 3a (multi), 4a
//! (autocomplete). Cover art display, prev/next navigation, mixed-field
//! UX with field annotations, pending-change bar, clickable rating stars,
//! and Ctrl+S save. The dialog receives raw track data and computes the
//! summary internally.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use rusqlite::Connection;

use reprise_core::library::tag_edit::{
    summarize, summarize_values, EditableTagSummary, EditableTags, MixedValue, TagPatch,
    TrackEditPatch,
};
use reprise_core::queries::autocomplete::AutocompleteColumn;
use reprise_core::release_lookup;

use crate::ui::autocomplete_entry::AutocompleteEntry;
use crate::ui::strings;
pub use crate::ui::tag_editor_state::NavigateDirection;
use crate::ui::tag_editor_state::*;
use crate::ui::tag_editor_widgets::*;

pub(super) type UpdateCallback = Rc<dyn Fn()>;
type UpdateCallbackSlot = Rc<RefCell<Option<UpdateCallback>>>;

// ── Star glyphs ──────────────────────────────────────────────────────────────

pub(super) const STAR_FILLED: &str = "\u{2605}";
pub(super) const STAR_OUTLINE: &str = "\u{2606}";

// ── Snapshot for revert ──────────────────────────────────────────────────────

struct FieldSnapshot {
    summary: EditableTagSummary,
    rating: MixedValue<i32>,
}

// ══════════════════════════════════════════════════════════════════════════════
//  TAG EDITOR DIALOG
// ══════════════════════════════════════════════════════════════════════════════

pub fn present(
    parent: &adw::ApplicationWindow,
    conn: &Rc<RefCell<Connection>>,
    tracks: &[(i64, PathBuf)],
    tags: &[EditableTags],
    ratings: &[i32],
    on_apply: impl Fn(TrackEditPatch) + Clone + 'static,
    on_navigate: impl Fn(NavigateDirection) -> bool + 'static,
) {
    if tracks.is_empty() {
        tracing::warn!("tag editor called with empty track list");
        return;
    }
    let track_count = tracks.len();
    let is_multi = track_count > 1;
    let summary = summarize(tags).unwrap();
    let rating_summary = summarize_values(ratings).unwrap();
    let snapshot = Rc::new(FieldSnapshot {
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

    let cover_area = build_cover_area(tracks, is_multi);

    // ── Form fields ──────────────────────────────────────────────────────

    // Title: per-track in multi mode (read-only, no click-to-unlock)
    let title_row = adw::EntryRow::builder()
        .title(strings::text(strings::TAG_TITLE))
        .build();
    if is_multi {
        title_row.set_editable(false);
        title_row.add_css_class("reprise-tag-mixed");
        add_annotation(&title_row, &strings::text(strings::TAG_PER_TRACK), false);
    }
    set_entry_from_mixed_string(&title_row, &summary.title);

    // Artist (autocomplete) — Mixed → click-to-unlock
    let artist_ac = AutocompleteEntry::new(
        &strings::text(strings::TAG_ARTIST),
        AutocompleteColumn::Artist,
        conn.clone(),
    );
    let artist_annotation =
        init_autocomplete_from_mixed(&artist_ac, &summary.artist, track_count, is_multi);
    if is_multi && matches!(summary.artist, MixedValue::Mixed) {
        attach_click_to_unlock(artist_ac.row(), artist_annotation.as_ref(), track_count);
    }

    // Album (autocomplete) — Mixed → click-to-unlock
    let album_ac = AutocompleteEntry::new(
        &strings::text(strings::TAG_ALBUM),
        AutocompleteColumn::Album,
        conn.clone(),
    );
    let album_annotation =
        init_autocomplete_from_mixed(&album_ac, &summary.album, track_count, is_multi);
    if is_multi && matches!(summary.album, MixedValue::Mixed) {
        attach_click_to_unlock(album_ac.row(), album_annotation.as_ref(), track_count);
    }

    // Album artist (autocomplete, with placeholder) — Mixed → click-to-unlock
    let album_artist_ac = AutocompleteEntry::new(
        &strings::text(strings::TAG_ALBUM_ARTIST),
        AutocompleteColumn::AlbumArtist,
        conn.clone(),
    );
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

    // Genre (autocomplete) — Mixed → click-to-unlock
    let genre_ac = AutocompleteEntry::new(
        &strings::text(strings::TAG_GENRE),
        AutocompleteColumn::Genre,
        conn.clone(),
    );
    let genre_annotation =
        init_autocomplete_from_mixed(&genre_ac, &summary.genre, track_count, is_multi);
    if is_multi && matches!(summary.genre, MixedValue::Mixed) {
        attach_click_to_unlock(genre_ac.row(), genre_annotation.as_ref(), track_count);
    }

    // Year — Mixed → click-to-unlock
    let year_row = adw::EntryRow::builder()
        .title(strings::text(strings::TAG_YEAR))
        .input_purpose(gtk4::InputPurpose::Digits)
        .build();
    set_entry_from_mixed_number(&year_row, &summary.year);
    let year_annotation_label: Option<gtk4::Label> = if is_multi {
        apply_mixed_annotation_number(&year_row, &summary.year, track_count)
    } else {
        None
    };
    if is_multi && matches!(summary.year, MixedValue::Mixed) {
        attach_click_to_unlock(&year_row, year_annotation_label.as_ref(), track_count);
    }

    // Track number: per-track in multi mode (read-only, no click-to-unlock)
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
    let mb_btn = gtk4::Button::with_label(&strings::text(strings::TAG_FETCH_MUSICBRAINZ));
    mb_btn.set_sensitive(false); // Task 5 wires this
    mb_box.append(&mb_btn);
    let mb_hint = gtk4::Label::new(Some(&strings::text(strings::TAG_FETCH_HINT)));
    mb_hint.add_css_class("reprise-tag-mb-hint");
    mb_hint.set_xalign(0.0);
    mb_box.append(&mb_hint);

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

    // Late-bound reference so revert callbacks inside update_save_state can
    // call update_save_state itself without a circular Rc construction.
    let update_fn_holder: UpdateCallbackSlot = Rc::new(RefCell::new(None));

    // Helper: update save-button sensitivity + pending bar
    let update_save_state = {
        let dirty = dirty.clone();
        let save_btn = save_btn.clone();
        let pending_bar = pending_bar.clone();
        let snapshot = snapshot.clone();
        let update_fn_holder = update_fn_holder.clone();

        // Capture field accessors for pending bar text and revert
        let title_row_c = title_row.clone();
        let year_row_c = year_row.clone();
        let track_no_row_c = track_no_row.clone();
        let artist_ac_row = artist_ac.row().clone();
        let album_ac_row = album_ac.row().clone();
        let album_artist_ac_row = album_artist_ac.row().clone();
        let genre_ac_row = genre_ac.row().clone();
        let rating_value_c = rating_value.clone();
        let rating_box_c = rating_box.clone();
        let year_annotation_c = year_annotation_label.clone();
        let artist_annotation_c = artist_annotation.clone();
        let album_annotation_c = album_annotation.clone();
        let album_artist_annotation_c = album_artist_annotation.clone();
        let genre_annotation_c = genre_annotation.clone();

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

                    // Build a revert callback for each dirty entry-row field.
                    // Helper captures needed context via clones inside the loop.
                    let get_update = || {
                        update_fn_holder
                            .borrow()
                            .as_ref()
                            .expect("update_fn_holder filled before first interaction")
                            .clone()
                    };

                    // String/number entry-row fields
                    let entry_fields: [(&adw::EntryRow, usize, Option<&gtk4::Label>); 7] = [
                        (&title_row_c, FIELD_TITLE, None),
                        (&artist_ac_row, FIELD_ARTIST, artist_annotation_c.as_ref()),
                        (&album_ac_row, FIELD_ALBUM, album_annotation_c.as_ref()),
                        (
                            &album_artist_ac_row,
                            FIELD_ALBUM_ARTIST,
                            album_artist_annotation_c.as_ref(),
                        ),
                        (&year_row_c, FIELD_YEAR, year_annotation_c.as_ref()),
                        (&track_no_row_c, FIELD_TRACK_NO, None),
                        (&genre_ac_row, FIELD_GENRE, genre_annotation_c.as_ref()),
                    ];

                    for (row, idx, annotation) in entry_fields {
                        if dirty[idx].get() {
                            let dirty_flag = dirty[idx].clone();
                            let row_c = row.clone();
                            let annotation_c = annotation.cloned();
                            let update = get_update();
                            let snap = snapshot.clone();
                            let on_revert: Box<dyn Fn()> = Box::new(move || {
                                // Restore original value
                                let orig_text = field_snapshot_text(&snap.summary, idx);
                                let was_mixed = field_snapshot_is_mixed(&snap.summary, idx);
                                if was_mixed {
                                    row_c.set_editable(false);
                                    row_c.add_css_class("reprise-tag-mixed");
                                    if let Some(lbl) = &annotation_c {
                                        lbl.set_text(&strings::text(strings::MULTIPLE_VALUES));
                                        lbl.remove_css_class("accent");
                                    }
                                    row_c.set_text("");
                                } else {
                                    row_c.set_text(orig_text.as_deref().unwrap_or(""));
                                }
                                dirty_flag.set(false);
                                update();
                            });
                            let item = build_pending_item(&field_name(idx), &row.text(), on_revert);
                            pending_bar.append(&item);
                        }
                    }

                    if dirty[FIELD_RATING].get() {
                        let dirty_flag = dirty[FIELD_RATING].clone();
                        let rating_val = rating_value_c.clone();
                        let rating_box_r = rating_box_c.clone();
                        let orig_rating = snapshot.rating.clone();
                        let update = get_update();
                        let rating_text = format!("{STAR_FILLED} {}", rating_value_c.get());
                        let on_revert: Box<dyn Fn()> = Box::new(move || {
                            let orig = match &orig_rating {
                                MixedValue::Uniform(v) => *v,
                                MixedValue::Mixed => 0,
                            };
                            rating_val.set(orig);
                            update_star_display(&rating_box_r, orig);
                            dirty_flag.set(false);
                            update();
                        });
                        let item =
                            build_pending_item(&field_name(FIELD_RATING), &rating_text, on_revert);
                        pending_bar.append(&item);
                    }

                    pending_bar.set_visible(true);
                } else {
                    pending_bar.set_visible(false);
                }
            }
        })
    };

    // Fill late-bound holder so revert callbacks can call update_save_state
    *update_fn_holder.borrow_mut() = Some(update_save_state.clone());

    // Wire entry-row changed signals
    let update_save_state: Rc<dyn Fn()> = update_save_state;

    let wire_entry_dirty = |row: &adw::EntryRow, field_idx: usize, update: &Rc<dyn Fn()>| {
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
        let on_rating_changed: UpdateCallback = Rc::new(move || {
            dirty_flag.set(true);
            update();
        });
        wire_star_clicks(&rating_box, &rating_value_c, &on_rating_changed);
    }

    // ── Save action ──────────────────────────────────────────────────────

    // Move the autocomplete entries into Rc so they survive closures. We
    // need to keep them alive for the dialog's lifetime anyway (they own
    // the popover lifecycle via Drop).
    let artist_ac = Rc::new(artist_ac);
    let album_ac = Rc::new(album_ac);
    let album_artist_ac = Rc::new(album_artist_ac);
    let genre_ac = Rc::new(genre_ac);

    // ── MusicBrainz fetch button (single-track only) ─────────────────────

    if !is_multi {
        mb_btn.set_sensitive(true);
        let mb_btn_c = mb_btn.clone();
        let mb_hint_c = mb_hint.clone();
        let year_row_c = year_row.clone();
        let album_artist_ac_c = album_artist_ac.clone();
        let genre_ac_c = genre_ac.clone();
        let artist_ac_c = artist_ac.clone();
        let album_ac_c = album_ac.clone();
        let update = update_save_state.clone();

        mb_btn.connect_clicked(move |_| {
            let artist = artist_ac_c.text();
            let album = album_ac_c.text();

            if artist.trim().is_empty() || album.trim().is_empty() {
                mb_hint_c.set_text(&strings::text(strings::TAG_FETCH_NO_RESULTS));
                return;
            }

            mb_btn_c.set_sensitive(false);
            mb_hint_c.set_text(&strings::text(strings::TAG_FETCH_LOADING));

            let (tx, rx) =
                async_channel::bounded::<Result<release_lookup::ReleaseLookupResult, String>>(1);

            if let Err(error) = std::thread::Builder::new()
                .name("reprise-mb-lookup".into())
                .spawn(move || {
                    let result =
                        release_lookup::lookup_release(&artist, &album).map_err(|e| e.to_string());
                    let _ = tx.send_blocking(result);
                })
            {
                tracing::warn!(%error, "could not start MusicBrainz lookup thread");
            }

            let mb_btn_r = mb_btn_c.clone();
            let mb_hint_r = mb_hint_c.clone();
            let year_row_r = year_row_c.clone();
            let album_artist_ac_r = album_artist_ac_c.clone();
            let genre_ac_r = genre_ac_c.clone();
            let update_r = update.clone();

            glib::spawn_future_local(async move {
                let Ok(result) = rx.recv().await else {
                    mb_btn_r.set_sensitive(true);
                    return;
                };
                mb_btn_r.set_sensitive(true);

                match result {
                    Err(error) => {
                        tracing::warn!(%error, "MusicBrainz lookup failed");
                        let msg = if error.contains("no matching") {
                            strings::TAG_FETCH_NO_RESULTS
                        } else {
                            strings::TAG_FETCH_NETWORK_ERROR
                        };
                        mb_hint_r.set_text(&strings::text(msg));
                    }
                    Ok(lookup) => {
                        let mut filled_any = false;

                        if let Some(year) = lookup.year {
                            if year_row_r.text().is_empty() {
                                year_row_r.set_text(&year.to_string());
                                filled_any = true;
                            }
                        }
                        if let Some(album_artist) = &lookup.album_artist {
                            if album_artist_ac_r.text().is_empty() {
                                album_artist_ac_r.set_text(album_artist);
                                filled_any = true;
                            }
                        }
                        if let Some(genre) = &lookup.genre {
                            if genre_ac_r.text().is_empty() {
                                genre_ac_r.set_text(genre);
                                filled_any = true;
                            }
                        }

                        // set_text on entryrows/autocompletes fires connect_changed
                        // which sets dirty flags, but update_save_state must be
                        // called to refresh the pending bar correctly.
                        if filled_any {
                            update_r();
                            mb_hint_r.set_text(&strings::text(strings::TAG_FETCH_FIELDS_FILLED));
                        } else {
                            mb_hint_r.set_text(&strings::text(strings::TAG_FETCH_NOTHING_TO_FILL));
                        }
                    }
                }
            });
        });
    }

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
        let error_label = error_label.clone();
        let dialog = dialog.clone();
        let on_apply = on_apply.clone();

        Rc::new(move || {
            let year_p = number_patch(dirty[FIELD_YEAR].get(), year_row.text().as_str());
            let track_p = number_patch(dirty[FIELD_TRACK_NO].get(), track_no_row.text().as_str());
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
//  TESTS
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
#[path = "tag_editor_tests.rs"]
mod tests;
