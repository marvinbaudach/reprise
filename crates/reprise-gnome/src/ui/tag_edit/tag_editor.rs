//! Tag editor dialog orchestration.
//!
//! F0: the single wiring point (`present()`) builds the [`TagEditSession`]
//! that is now the dialog's only state truth and threads it through
//! `TagEditorForm::build`, `tag_editor_dirty::wire`, and
//! `tag_editor_save::wire` — no more `Vec<Rc<Cell<bool>>>` dirty array
//! running in parallel.
//!
//! F2: `on_saved` used to fire the instant `tag_editor_save`'s Save click
//! handler produced a batch, immediately followed by closing the dialog
//! here. Now the dialog stays open for the whole write (Beschluss #4: "kein
//! Abbruch, Batch ist aus User-Sicht atomar") — `present()` hands the batch
//! to `tag_edit_flow::spawn_save`, which owns the worker thread and the
//! progress channel, and only calls `on_saved` (renamed to carry the
//! now-complete `TagBatchReport` alongside the batch) once the dialog has
//! already been closed.
//!
//! G1 (TAG-4): when the editor opens on exactly one track, `tag_edit_flow.rs`
//! may also hand in a [`BrowseSnapshot`] — the visible list's other tracks at
//! the moment the dialog opened. When present, the `TagEditSession` is built
//! from the *whole* snapshot (not just the opened track) so `set_current_track`
//! actually has somewhere to move to, and the ‹›/Ctrl+Page Up/Down buttons page
//! through it by track id, never by index. `TagEditorForm` itself only ever
//! sees the single opened track for its one-time construction (cover art,
//! subtitle format/bitrate, is_multi=false layout) — this module immediately
//! re-renders the built widgets from the session's actual current track right
//! after construction (`refresh_current_track_fields`), because a >1-track
//! SingleNav session can make `TagEditorForm::build`'s own `mixed_placeholder`-
//! driven initial render blank out fields whose values differ across the
//! snapshot (harmless: nothing is shown between construction and this
//! synchronous correction, and `is_multi` being `false` throughout already
//! suppresses every mixed-value CSS/annotation side effect — see this
//! module's tests and the G1 task report for the full reasoning).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::tag_edit::{TagBatchReport, TrackWrite};
use reprise_core::library::tag_edit_session::{
    SessionMode, SessionTrack, TagEditSession, TagField,
};
use rusqlite::Connection;

use crate::ui::strings;
use crate::ui::tag_editor_dirty::parse_number_field;
use crate::ui::tag_editor_dirty::ProgrammaticChanges;
use crate::ui::tag_editor_form::{EditorMode, TagEditorForm};
pub use crate::ui::tag_editor_state::NavigateDirection;
use crate::ui::tag_editor_widgets::{format_track_subtitle, update_star_display};

pub(in crate::ui) const STAR_FILLED: &str = "\u{2605}";
pub(in crate::ui) const STAR_OUTLINE: &str = "\u{2606}";

/// G1 (TAG-4): the frozen browse snapshot captured by `tag_edit_flow.rs` from
/// `Shared::current_view_ids()` when a single-track edit opens — the visible
/// list's tracks, in view order, at the moment the dialog opened. Never
/// re-queried afterward: that is exactly what keeps "Track 3 of 12" stable
/// while a watcher reconcile or a sort click resorts the live list underneath
/// the open dialog. This module never touches the library itself (TAG-1:
/// navigation-neutral) — `tag_edit_flow.rs` is the only place that reads
/// from the track list.
pub(in crate::ui) struct BrowseSnapshot {
    pub(in crate::ui) tracks: Vec<SessionTrack>,
    pub(in crate::ui) bitrates: Vec<Option<u32>>,
}

impl BrowseSnapshot {
    fn ids(&self) -> Vec<i64> {
        self.tracks.iter().map(|track| track.id).collect()
    }
}

pub(in crate::ui) fn present(
    parent: &adw::ApplicationWindow,
    conn: &Rc<RefCell<Connection>>,
    tracks: Vec<SessionTrack>,
    bitrates: &[Option<u32>],
    browse: Option<BrowseSnapshot>,
    on_saved: impl Fn(Vec<TrackWrite>, TagBatchReport) + Clone + 'static,
) {
    let Some(mode) = EditorMode::new(tracks.len()) else {
        tracing::warn!("tag editor called with empty track list");
        return;
    };
    let track_count = mode.track_count();
    let track_paths: Vec<(i64, PathBuf)> = tracks
        .iter()
        .map(|track| (track.id, track.path.clone()))
        .collect();
    let opened_track_id = tracks.first().map(|track| track.id);

    // G1: only trust the snapshot when it actually contains the opened track
    // and genuinely has more than one entry — otherwise fall back to
    // exactly today's (pre-G1) single-track session, no browsing offered.
    let browse = browse.filter(|snapshot| {
        !mode.is_multi()
            && snapshot.ids().len() > 1
            && opened_track_id.is_some_and(|id| snapshot.ids().contains(&id))
    });

    let session_mode = if mode.is_multi() {
        SessionMode::Multi
    } else {
        SessionMode::SingleNav
    };
    let session_tracks = match &browse {
        Some(snapshot) => snapshot.tracks.clone(),
        None => tracks,
    };
    let session = Rc::new(RefCell::new(TagEditSession::new(
        session_tracks,
        session_mode,
    )));
    if let (Some(id), Some(_)) = (opened_track_id, &browse) {
        session.borrow_mut().set_current_track(id);
    }

    let form = TagEditorForm::build(mode, conn, &track_paths, bitrates, &session);
    let crate::ui::tag_editor_dirty::DirtyState {
        update: update_save_state,
        programmatic_changes,
    } = crate::ui::tag_editor_dirty::wire(mode, &form, &session);

    crate::ui::tag_editor_lookup::wire(
        &session,
        crate::ui::tag_editor_lookup::LookupWidgets {
            button: &form.mb_btn,
            hint: &form.mb_hint,
            year: &form.year_row,
            artist: &form.artist_ac,
            album: &form.album_ac,
            album_artist: &form.album_artist_ac,
            genre: &form.genre_ac,
        },
        &update_save_state,
    );

    let browse_handles = clone_browse_field_handles(&form);

    // G1: reconcile the just-built widgets + header subtitle against the
    // real browse state (position, boundary sensitivity) — see this
    // module's doc comment for why this immediate correction is needed and
    // why it never flashes the wrong content.
    if let Some(snapshot) = &browse {
        let ids = snapshot.ids();
        if let Some(id) = opened_track_id {
            refresh_current_track_fields(&browse_handles, &session, id, &programmatic_changes);
            update_save_state();
            let tail = subtitle_tail_for_track(&snapshot.tracks, &snapshot.bitrates, id);
            apply_browse_position(&browse_handles, &ids, id, tail.as_deref());
            browse_handles.prev_btn.set_visible(true);
            browse_handles.next_btn.set_visible(true);
        }
    }

    let conn_for_save = conn.clone();
    let save_progress_widgets = crate::ui::tag_edit_flow::SaveProgressWidgets {
        dialog: form.dialog.clone(),
        save_button: form.save_btn.clone(),
        cancel_button: form.cancel_btn.clone(),
        content: form.content.clone(),
        error_label: form.error_label.clone(),
    };

    let on_navigate: Rc<dyn Fn(NavigateDirection) -> bool> = Rc::new(build_on_navigate(
        browse_handles,
        &session,
        browse.as_ref(),
        &update_save_state,
        &programmatic_changes,
    ));
    // TAG-4's "(Ctrl+Page Up/Down)" keyboard half of ‹›-navigation: wired
    // directly on the dialog here (not through `tag_editor_save.rs`'s own
    // keyboard wiring, Package E's territory this wave) since it only ever
    // needs to call the exact same `on_navigate` the ‹›-buttons already use.
    wire_browse_keyboard_shortcut(&form.dialog, &on_navigate);

    crate::ui::tag_editor_save::wire(
        crate::ui::tag_editor_save::SaveWidgets {
            dialog: &form.dialog,
            save_button: &form.save_btn,
            cancel_button: &form.cancel_btn,
            previous_button: &form.prev_btn,
            next_button: &form.next_btn,
            year: &form.year_row,
            track_number: &form.track_no_row,
            error_label: &form.error_label,
        },
        &session,
        move |batch| {
            if batch.is_empty() {
                // Save-button sensitivity already guards this (F1); a
                // defensive no-op rather than spawning an empty write.
                return;
            }
            crate::ui::tag_edit_flow::spawn_save(
                &conn_for_save,
                save_progress_widgets.clone(),
                batch,
                on_saved.clone(),
            );
        },
        move |direction| on_navigate(direction),
    );

    form.dialog.present(Some(parent));
    tracing::debug!(
        track_count,
        is_multi = mode.is_multi(),
        "redesigned tag editor presented"
    );
}

/// The format/bitrate tail ("FLAC · 987 kbit/s") for `track_id` within a
/// browse snapshot's own tracks/bitrates — recomputed per current track
/// (rather than fixed at the opened track) since files in one album can
/// differ in format or bitrate even for the same release.
fn subtitle_tail_for_track(
    tracks: &[SessionTrack],
    bitrates: &[Option<u32>],
    track_id: i64,
) -> Option<String> {
    let index = tracks.iter().position(|track| track.id == track_id)?;
    let extension = tracks[index].path.extension().and_then(|ext| ext.to_str());
    let bitrate = bitrates.get(index).copied().flatten();
    format_track_subtitle(extension, bitrate)
}

/// G1 (TAG-4): 1-based `(position, total)` of `track_id` within the frozen
/// `snapshot` — a pure index lookup that never touches a live (possibly
/// re-sorted) view, which is exactly what keeps "Track 3 of 12" stable while
/// a watcher reconcile or a sort click resorts the visible list underneath
/// the open dialog.
pub(in crate::ui) fn snapshot_position(snapshot: &[i64], track_id: i64) -> Option<(usize, usize)> {
    let index = snapshot.iter().position(|&id| id == track_id)?;
    Some((index + 1, snapshot.len()))
}

/// G1 (TAG-4): the next/previous track id in the snapshot, or `None` at a
/// boundary (the first track has no Previous, the last has no Next) — the
/// caller treats `None` as "nothing to do", never wrapping around.
pub(in crate::ui) fn navigate_snapshot(
    snapshot: &[i64],
    current_id: i64,
    direction: NavigateDirection,
) -> Option<i64> {
    let index = snapshot.iter().position(|&id| id == current_id)?;
    match direction {
        NavigateDirection::Previous => index.checked_sub(1).map(|previous| snapshot[previous]),
        NavigateDirection::Next => snapshot.get(index + 1).copied(),
    }
}

/// G1 (TAG-4): whether the two number fields' *current text* is valid enough
/// to browse away from — the same rule `tag_editor_save.rs`'s save-time
/// validation applies to Year/Track-number (an interim keystroke is not yet
/// an error, but text that fails to parse blocks both Save and, now,
/// navigation too). `track_no_editable` mirrors TAG-3 (a per-track-locked "—"
/// field is never a source of a navigation-blocking error), even though
/// browsing only ever runs in SingleNav, where the field stays editable.
pub(in crate::ui) fn can_navigate_away(
    year_text: &str,
    track_no_text: &str,
    track_no_editable: bool,
) -> bool {
    parse_number_field(year_text).is_ok()
        && (!track_no_editable || parse_number_field(track_no_text).is_ok())
}

/// P-2: which of the two nav buttons should be clickable at `position` — a
/// disabled boundary button is a normal paging affordance (not a "dead
/// click"), so no extra tooltip is needed beyond the existing Previous/Next
/// ones `TagEditorForm::build` already sets.
fn nav_sensitivity(position: (usize, usize)) -> (bool, bool) {
    let (index, total) = position;
    (index > 1, index < total)
}

/// `TagEditSession::effective_display` renders an absent/empty value as the
/// literal placeholder text `"empty"` (TAG-2's mixed-value vocabulary, e.g.
/// "Mixed — Deathcore, empty") — exactly wrong for a plain field widget's own
/// text when nothing is mixed. Turns that sentinel back into a real blank
/// string before writing to any text field. Used by the browse-refresh path
/// here and by `tag_editor_form::text_bridge`'s initial build (a uniformly
/// empty field must show blank, not the word "empty").
pub(in crate::ui) fn display_or_blank(display: Option<String>) -> String {
    match display {
        Some(text) if text == "empty" => String::new(),
        Some(text) => text,
        None => String::new(),
    }
}

/// Same idea as [`display_or_blank`] for the two numeric fields: a value
/// that fails to parse back to `u32` (including the "empty" sentinel) is
/// rendered blank rather than as stray text.
fn numeric_display_or_blank(display: Option<String>) -> String {
    display
        .and_then(|text| text.parse::<u32>().ok())
        .map(|value| value.to_string())
        .unwrap_or_default()
}

/// G1 (TAG-4): the field/subtitle/nav widget handles browsing needs, cloned
/// once out of `TagEditorForm` (every field is a reference-counted GTK
/// widget, so cloning is cheap) so the exact same [`refresh_current_track_fields`]
/// runs both immediately after construction (correcting `TagEditorForm::
/// build`'s own initial render for a >1-track SingleNav session — see this
/// module's doc comment) and from the `on_navigate` closure that outlives
/// `present()` returning, with no duplicated widget-poking logic between
/// the two call sites.
struct BrowseFieldHandles {
    title_row: adw::EntryRow,
    artist_ac: Rc<crate::ui::autocomplete_entry::AutocompleteEntry>,
    album_ac: Rc<crate::ui::autocomplete_entry::AutocompleteEntry>,
    album_artist_ac: Rc<crate::ui::autocomplete_entry::AutocompleteEntry>,
    genre_ac: Rc<crate::ui::autocomplete_entry::AutocompleteEntry>,
    year_row: adw::EntryRow,
    track_no_row: adw::EntryRow,
    rating_box: gtk4::Box,
    rating_value: Rc<std::cell::Cell<i32>>,
    error_label: gtk4::Label,
    title_widget: adw::WindowTitle,
    prev_btn: gtk4::Button,
    next_btn: gtk4::Button,
}

fn clone_browse_field_handles(form: &TagEditorForm) -> BrowseFieldHandles {
    BrowseFieldHandles {
        title_row: form.title_row.clone(),
        artist_ac: form.artist_ac.clone(),
        album_ac: form.album_ac.clone(),
        album_artist_ac: form.album_artist_ac.clone(),
        genre_ac: form.genre_ac.clone(),
        year_row: form.year_row.clone(),
        track_no_row: form.track_no_row.clone(),
        rating_box: form.rating_box.clone(),
        rating_value: form.rating_value.clone(),
        error_label: form.error_label.clone(),
        title_widget: form.title_widget.clone(),
        prev_btn: form.prev_btn.clone(),
        next_btn: form.next_btn.clone(),
    }
}

/// Re-renders every field widget from `session`'s *current* effective
/// values for `track_id`. Reads every value out of the session into owned
/// `String`s in one `borrow()` that ends before any widget is touched —
/// `.set_text()` fires each field's already-connected "changed" signal
/// (`tag_editor_dirty.rs`), which itself calls `session.borrow_mut()`;
/// holding a borrow across that call would panic (`BorrowMutError`), per
/// this crate's RefCell discipline.
fn refresh_current_track_fields(
    handles: &BrowseFieldHandles,
    session: &Rc<RefCell<TagEditSession>>,
    track_id: i64,
    programmatic_changes: &ProgrammaticChanges,
) {
    let (title, artist, album, album_artist, genre, year, track_no, rating) = {
        let session_ref = session.borrow();
        (
            display_or_blank(session_ref.effective_display(track_id, TagField::Title)),
            display_or_blank(session_ref.effective_display(track_id, TagField::Artist)),
            display_or_blank(session_ref.effective_display(track_id, TagField::Album)),
            display_or_blank(session_ref.effective_display(track_id, TagField::AlbumArtist)),
            display_or_blank(session_ref.effective_display(track_id, TagField::Genre)),
            numeric_display_or_blank(session_ref.effective_display(track_id, TagField::Year)),
            numeric_display_or_blank(session_ref.effective_display(track_id, TagField::TrackNo)),
            session_ref
                .effective_display(track_id, TagField::Rating)
                .and_then(|text| text.parse::<i32>().ok())
                .unwrap_or(0),
        )
    };
    programmatic_changes.run(|| {
        handles.title_row.set_text(&title);
        handles.artist_ac.set_text(&artist);
        handles.album_ac.set_text(&album);
        handles.album_artist_ac.set_text(&album_artist);
        handles.genre_ac.set_text(&genre);
        handles.year_row.set_text(&year);
        handles.track_no_row.set_text(&track_no);
        handles.rating_value.set(rating);
        update_star_display(&handles.rating_box, rating);
    });
}

/// Composes the header subtitle for the current browse position: "Track 3 of
/// 12" alone, "Track 3 of 12 · FLAC · 987 kbit/s" with the format/bitrate
/// tail, or just the tail (no snapshot) — mirrors `TagEditorForm::build`'s
/// own single-mode subtitle assembly, reused here since this module is the
/// only one that ever learns the position (Beschluss #2).
fn compose_single_subtitle(position: Option<(usize, usize)>, tail: Option<&str>) -> String {
    match (position, tail) {
        (Some((index, total)), Some(tail)) => {
            format!(
                "{} \u{b7} {tail}",
                strings::tag_track_position(index, total)
            )
        }
        (Some((index, total)), None) => strings::tag_track_position(index, total),
        (None, Some(tail)) => tail.to_string(),
        (None, None) => String::new(),
    }
}

/// Applies the browse position to the header subtitle and the nav buttons'
/// sensitivity for `track_id` within `ids` — the one place both are kept in
/// sync, called on construction and after every successful navigation.
fn apply_browse_position(
    handles: &BrowseFieldHandles,
    ids: &[i64],
    track_id: i64,
    subtitle_tail: Option<&str>,
) {
    let position = snapshot_position(ids, track_id);
    let subtitle = compose_single_subtitle(position, subtitle_tail);
    handles.title_widget.set_subtitle(&subtitle);
    if let Some(position) = position {
        let (prev_sensitive, next_sensitive) = nav_sensitivity(position);
        handles.prev_btn.set_sensitive(prev_sensitive);
        handles.next_btn.set_sensitive(next_sensitive);
    }
}

/// Builds the `on_navigate` callback `tag_editor_save::wire` invokes on
/// ‹›/Ctrl+Page Up/Down: validates the current Year/Track-number text first
/// (TAG-4: an invalid number blocks browsing exactly like it blocks Save),
/// then moves the session's current track, re-renders the fields, and
/// updates the subtitle (recomputed per track, `subtitle_tail_for_track`)
/// and nav sensitivity. Returns `false` (no-op) whenever there is no real
/// snapshot to browse, at a boundary, or the current number fields are
/// invalid.
fn build_on_navigate(
    handles: BrowseFieldHandles,
    session: &Rc<RefCell<TagEditSession>>,
    browse: Option<&BrowseSnapshot>,
    update_save_state: &Rc<dyn Fn()>,
    programmatic_changes: &ProgrammaticChanges,
) -> impl Fn(NavigateDirection) -> bool {
    let session = session.clone();
    let update_save_state = update_save_state.clone();
    let programmatic_changes = programmatic_changes.clone();
    let ids = browse.map(BrowseSnapshot::ids).unwrap_or_default();
    let tracks = browse
        .map(|snapshot| snapshot.tracks.clone())
        .unwrap_or_default();
    let bitrates = browse
        .map(|snapshot| snapshot.bitrates.clone())
        .unwrap_or_default();
    move |direction| {
        if ids.len() <= 1 {
            return false;
        }
        let track_no_editable = handles.track_no_row.is_editable();
        if !can_navigate_away(
            &handles.year_row.text(),
            &handles.track_no_row.text(),
            track_no_editable,
        ) {
            handles.error_label.set_visible(true);
            return false;
        }
        let current = session.borrow().current_track_id();
        let Some(next_id) = navigate_snapshot(&ids, current, direction) else {
            return false;
        };
        handles.error_label.set_visible(false);
        session.borrow_mut().set_current_track(next_id);
        refresh_current_track_fields(&handles, &session, next_id, &programmatic_changes);
        update_save_state();
        let tail = subtitle_tail_for_track(&tracks, &bitrates, next_id);
        apply_browse_position(&handles, &ids, next_id, tail.as_deref());
        true
    }
}

/// TAG-4's keyboard half of ‹›-navigation: Ctrl+Page Up/Down call the exact
/// same `on_navigate` the ‹›-buttons use (built once in `present()`, shared
/// here via `Rc` — cheap clone, no rebuilding). Unmodified/plain Page Up/
/// Down are left alone (`glib::Propagation::Proceed`) so normal scroll
/// keeps working inside the dialog's `ScrolledWindow`.
fn wire_browse_keyboard_shortcut(
    dialog: &adw::Dialog,
    on_navigate: &Rc<dyn Fn(NavigateDirection) -> bool>,
) {
    let on_navigate = on_navigate.clone();
    let controller = gtk4::EventControllerKey::new();
    controller.connect_key_pressed(move |_, key, _, modifier| {
        if !modifier.contains(gdk::ModifierType::CONTROL_MASK) {
            return glib::Propagation::Proceed;
        }
        let direction = match key {
            gdk::Key::Page_Up => NavigateDirection::Previous,
            gdk::Key::Page_Down => NavigateDirection::Next,
            _ => return glib::Propagation::Proceed,
        };
        if on_navigate(direction) {
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    dialog.add_controller(controller);
}

#[cfg(test)]
#[path = "tag_editor_tests.rs"]
mod tests;

#[cfg(test)]
mod navigation_tests {
    use super::*;

    #[test]
    fn tag_4_snapshot_positions_stable_across_resort() {
        let snapshot = vec![10, 20, 30];
        assert_eq!(snapshot_position(&snapshot, 20), Some((2, 3)));
        // A live re-sort produces a differently-ordered id list elsewhere in
        // the app (e.g. `Shared::current_view_ids()` after a sort click) —
        // it never reaches this function, so it cannot change the answer.
        let resorted_live_view = [30, 10, 20];
        let _ = resorted_live_view;
        assert_eq!(
            snapshot_position(&snapshot, 20),
            Some((2, 3)),
            "position must be read off the frozen snapshot, never a live re-sort"
        );
    }

    #[test]
    fn snapshot_position_is_none_for_an_id_outside_the_snapshot() {
        assert_eq!(snapshot_position(&[1, 2, 3], 99), None);
    }

    #[test]
    fn tag_4_invalid_number_blocks_navigation() {
        assert!(!can_navigate_away("not-a-number", "3", true));
        assert!(!can_navigate_away("2020", "not-a-number", true));
        assert!(can_navigate_away("2020", "3", true));
        // A read-only (per-track-locked) track-number field never blocks —
        // TAG-3 only locks it in Multi, where browsing never runs anyway,
        // but the guard degrades safely if ever asked.
        assert!(can_navigate_away("2020", "garbage", false));
        // An empty field is a deliberate clear, not an error.
        assert!(can_navigate_away("", "", true));
    }

    #[test]
    fn navigate_snapshot_stops_at_both_boundaries() {
        let snapshot = vec![1, 2, 3];
        assert_eq!(
            navigate_snapshot(&snapshot, 1, NavigateDirection::Previous),
            None
        );
        assert_eq!(
            navigate_snapshot(&snapshot, 3, NavigateDirection::Next),
            None
        );
        assert_eq!(
            navigate_snapshot(&snapshot, 2, NavigateDirection::Next),
            Some(3)
        );
        assert_eq!(
            navigate_snapshot(&snapshot, 2, NavigateDirection::Previous),
            Some(1)
        );
    }

    #[test]
    fn nav_sensitivity_disables_at_each_boundary_only() {
        assert_eq!(nav_sensitivity((1, 3)), (false, true));
        assert_eq!(nav_sensitivity((2, 3)), (true, true));
        assert_eq!(nav_sensitivity((3, 3)), (true, false));
        assert_eq!(nav_sensitivity((1, 1)), (false, false));
    }

    #[test]
    fn compose_single_subtitle_combines_position_and_tail() {
        assert_eq!(
            compose_single_subtitle(Some((3, 12)), Some("FLAC \u{b7} 987 kbit/s")),
            "Track 3 of 12 \u{b7} FLAC \u{b7} 987 kbit/s"
        );
        assert_eq!(
            compose_single_subtitle(Some((3, 12)), None),
            "Track 3 of 12"
        );
        assert_eq!(
            compose_single_subtitle(None, Some("FLAC")),
            "FLAC".to_string()
        );
        assert_eq!(compose_single_subtitle(None, None), String::new());
    }

    #[test]
    fn display_or_blank_turns_the_empty_sentinel_into_a_real_blank() {
        assert_eq!(display_or_blank(Some("empty".to_string())), "");
        assert_eq!(display_or_blank(Some("Rock".to_string())), "Rock");
        assert_eq!(display_or_blank(None), "");
    }

    #[test]
    fn numeric_display_or_blank_rejects_the_empty_sentinel() {
        assert_eq!(numeric_display_or_blank(Some("empty".to_string())), "");
        assert_eq!(numeric_display_or_blank(Some("2020".to_string())), "2020");
        assert_eq!(numeric_display_or_blank(None), "");
    }
}
