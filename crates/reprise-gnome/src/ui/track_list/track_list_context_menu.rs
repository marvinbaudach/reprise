//! Context menu + multi-select (Stage 3 Task 5) — split out of
//! `track_list.rs` exactly the way `player_controller.rs` split its MPRIS
//! mirror and fault-tolerance logic into `mpris_mirror.rs`/`playback_
//! faults.rs` (Stage 3 Task 1): same reasoning (keep the owning file from
//! growing without bound), same shape (this is an `impl`-free sibling module
//! that reaches into `track_list.rs`'s private `Shared` struct via
//! `pub(in crate::ui)` fields/functions, not a separate type). `track_list.rs`
//! still owns `Shared` itself, `TrackList::new`'s construction order, and
//! the five column factories (`append_column`/`append_rating_column`) that
//! call into this module's [`wire_context_menu_gesture`] from their
//! `connect_setup` closures; this module owns everything else about the
//! menu: building the `gio::Menu`/`PopoverMenu`, the `"tracklist"`
//! `gio::SimpleActionGroup`, the "New playlist…" dialog, and the
//! `REPRISE_SMOKE_MENU_ACTION` dev hook.
//!
//! The *logic* these functions invoke — mapping selected positions to track
//! ids, and the actual queue/playlist mutations — lives in `ui::
//! track_actions` instead, so it's testable without a display; see that
//! module's doc comment for the position→id/remove-by-position design.
//!
//! ## Row position via a stable `ListItem` handle, not qdata or per-bind
//! rewiring
//!
//! [`wire_context_menu_gesture`] is called once per cell widget, from
//! `connect_setup` (not `connect_bind`): its `GestureClick` closure captures
//! a clone of the cell's `gtk::ListItem` (a cheap GObject reference) and
//! reads `item.position()` fresh at click time, which always reflects
//! whichever row this recycled widget currently displays. This sidesteps
//! two harder alternatives: (1) `ColumnView`'s per-row widget class is
//! private, so there is no supported way to recover "which row" a raw `(x,
//! y)` point picked via `column_view.pick()` belongs to, and (2) stashing a
//! fresh position on the widget via GObject qdata (`ObjectExt::set_data`) is
//! `unsafe` in this project's pinned `glib` version (see `ui::sidebar`'s
//! module doc comment for the same conclusion).
//!
//! ## GNOME right-click selection convention
//!
//! Right-clicking a row that is *not* part of the current selection first
//! replaces the selection with just that row; right-clicking an
//! already-selected row (alone, or as part of a larger multi-selection)
//! leaves the selection untouched, so the menu acts on the whole set — see
//! [`show_context_menu`].

use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::graphene;
use gtk4::prelude::*;

use super::track_menu::{
    action_states, build_track_menu, playlist_entries, summarize_selection, MenuContext,
    MenuInputs, SelectionSummary,
};
use super::track_playback_selection::{self, ContextPlayDecision, PlayableSelection};
use crate::ui::delete_tracks;
use crate::ui::dialogs;
use crate::ui::popover_lifecycle;
use crate::ui::show_in_files;
use crate::ui::strings;
use crate::ui::tag_edit_flow;
use crate::ui::track_actions;
use crate::ui::track_list::{reload, show_toast, Shared};
use crate::ui::track_list_queue_menu;
use reprise_core::library::playlists;
use reprise_core::models::Track;
use reprise_core::view_source::ViewSource;

/// Bare `gio::SimpleAction` names in the `"tracklist"` action group —
/// internal identifiers, not user-facing text (see `strings.rs` for the
/// menu item labels themselves).
const ACTION_PLAY: &str = "play";
const ACTION_ADD_TO_QUEUE: &str = "add-to-queue";
pub(in crate::ui) const ACTION_PLAY_NEXT: &str = "play-next";
const ACTION_MOVE_TO_TOP: &str = "move-to-top";
const ACTION_MOVE_UP: &str = "move-up";
const ACTION_MOVE_DOWN: &str = "move-down";
const ACTION_ADD_TO_PLAYLIST: &str = "add-to-playlist";
const ACTION_NEW_PLAYLIST: &str = "new-playlist";
const ACTION_GO_TO_ALBUM: &str = "go-to-album";
const ACTION_GO_TO_ARTIST: &str = "go-to-artist";
const ACTION_SHOW_IN_FILES: &str = "show-in-files";
const ACTION_SHOW_IN_MISSING_FILES: &str = "show-in-missing-files";
pub(in crate::ui) const ACTION_REMOVE_FROM_PLAYLIST: &str = "remove-from-playlist";
/// Missing-source-only action: see `handle_remove_from_library`'s doc
/// comment. Distinct from `delete_tracks::ACTION_REMOVE`
/// (`"remove-selected-from-library"`, the CTX-unified menu's generic
/// path-matching delete offered elsewhere) — this one goes through
/// `track_actions::remove_missing_selected`'s batch/transactional delete
/// instead.
pub(in crate::ui) const ACTION_REMOVE_FROM_LIBRARY: &str = "remove-from-library";
/// The action group name every `"tracklist.*"` detailed-action string below
/// refers to — inserted once on `column_view` by `wire_context_menu_actions`.
const ACTION_GROUP_NAME: &str = "tracklist";

/// Attaches a secondary-click (`button = 3`) context-menu gesture to a
/// freshly-`setup` cell widget (a plain `Label` for the seven text columns,
/// or the `RatingWidget` for the Rating column) — see the module doc's `##
/// Row position via a stable ListItem handle` section for why this only
/// needs to run once per widget instance, from `connect_setup`, with no
/// per-bind rewiring.
pub(in crate::ui) fn wire_context_menu_gesture(
    widget: &impl IsA<gtk4::Widget>,
    item: &gtk4::ListItem,
    shared: &Rc<Shared>,
    column_view: &gtk4::ColumnView,
) {
    // input-parity: ACC-8 keyboard=menu-shift-f10
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);

    let item = item.clone();
    let shared = shared.clone();
    let column_view = column_view.clone();
    gesture.connect_pressed(move |gesture, _n_press, x, y| {
        let position = item.position();
        if position == gtk4::INVALID_LIST_POSITION {
            tracing::warn!("context menu: list item has no valid position; ignoring click");
            return;
        }
        // Claim the sequence: a secondary click has no other meaning on
        // this widget (rating stars only react to the primary button), but
        // claiming keeps this predictable if that ever changes.
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let Some(clicked_widget) = gesture.widget() else {
            tracing::warn!("context menu: gesture has no widget; ignoring click");
            return;
        };
        show_context_menu(&shared, &column_view, position, &clicked_widget, x, y);
    });

    widget.upcast_ref::<gtk4::Widget>().add_controller(gesture);
}

/// Reads `shared.selection`'s current selection as row positions, in
/// ascending order (`gtk::Bitset` iteration order). `pub(in crate::ui)` (not
/// private): `ui::track_list_dnd`'s drag-prepare handler reuses this exact
/// same read to decide whether a drag starting on an already-selected row
/// should carry the *whole* selection — see that module's doc comment.
pub(in crate::ui) fn current_selection_positions(shared: &Rc<Shared>) -> Vec<u32> {
    let bitset = shared.selection.selection();
    let Some((mut iter, first)) = gtk4::BitsetIter::init_first(&bitset) else {
        return Vec::new();
    };
    let mut positions = vec![first];
    positions.extend(iter.by_ref());
    positions
}

/// `current_selection_positions` mapped to track ids via `ui::track_actions::
/// selected_track_ids`.
pub(in crate::ui) fn current_selection_ids(shared: &Rc<Shared>) -> Vec<i64> {
    let positions = current_selection_positions(shared);
    track_actions::selected_track_ids(&positions, &shared.model)
}

/// PLAY-4b: the current selection with missing rows already filtered out —
/// the exact ids "Play"/"Play next"/"Add to queue" are allowed to act on.
/// See `track_playback_selection`'s module doc.
fn current_playable_selection(shared: &Rc<Shared>) -> PlayableSelection {
    let positions = current_selection_positions(shared);
    track_playback_selection::selected_playable_tracks(&positions, &shared.model)
}

pub(in crate::ui) fn current_selection_tracks(shared: &Rc<Shared>) -> Vec<Track> {
    current_selection_positions(shared)
        .into_iter()
        .filter_map(|position| shared.model.track_at(position))
        .collect()
}

/// Builds the `gio::Menu` model shown by a right-click — rebuilt fresh on
/// every open (`show_context_menu`) rather than cached, since the playlist
/// submenu must always reflect the *current* set of playlists (one created
/// or renamed between two right-clicks must show up on the next one).
/// Building a handful of `gio::MenuItem`s per open is cheap enough that
/// caching would only add invalidation complexity for no real benefit at
/// this scale.
pub(in crate::ui) fn build_context_menu_model(shared: &Rc<Shared>) -> gio::Menu {
    let source = shared.source.borrow().clone();
    let context = MenuContext::from_source(&source);
    let is_missing_view = matches!(&source, ViewSource::Missing);
    let summary = summarize_selection(&current_selection_tracks(shared));
    update_menu_action_states(shared, context, &summary);
    let entries = {
        let conn = &shared.conn;
        let rows = playlists::list(conn).unwrap_or_else(|error| {
            tracing::error!(%error, "context menu: failed to list playlists");
            Vec::new()
        });
        playlist_entries(&rows, &source)
    };
    let menu = build_track_menu(&MenuInputs {
        context,
        selection: &summary,
        playlists: &entries,
        is_missing_view,
    });
    if matches!(context, MenuContext::Playlist | MenuContext::Queue) {
        let reorder = gio::Menu::new();
        for (label, action) in [
            (strings::CONTEXT_MENU_MOVE_UP, ACTION_MOVE_UP),
            (strings::CONTEXT_MENU_MOVE_DOWN, ACTION_MOVE_DOWN),
        ] {
            reorder.append(
                Some(&strings::text(label)),
                Some(&format!("tracklist.{action}")),
            );
        }
        if context == MenuContext::Playlist {
            reorder.append(
                Some(&strings::text(strings::CONTEXT_MENU_MOVE_TO_TOP)),
                Some("tracklist.move-to-top"),
            );
        }
        menu.append_section(None, &reorder);
    }
    menu
}

/// Greys out the menu actions the current selection cannot support. Takes the
/// `context`/`summary` already computed by `build_context_menu_model` so the
/// source and selection are each read exactly once per menu open.
fn update_menu_action_states(
    shared: &Rc<Shared>,
    context: MenuContext,
    summary: &SelectionSummary,
) {
    let states = action_states(context, summary);
    // PLAY-4b: "Play next"/"Add to queue" use the missing-aware "at least
    // one playable track survives" rule (`PlayableSelection::
    // enqueue_enabled`), not `states.enqueue`'s coarser "no missing tracks
    // at all" rule — a mixed selection still enqueues just the playable ids
    // instead of being greyed out entirely.
    let enqueue_enabled = current_playable_selection(shared).enqueue_enabled();
    let move_up = super::track_list_keyboard_reorder::is_available(
        shared,
        super::track_list_keyboard_reorder::ReorderDirection::Up,
    );
    let move_down = super::track_list_keyboard_reorder::is_available(
        shared,
        super::track_list_keyboard_reorder::ReorderDirection::Down,
    );
    let move_to_top = super::track_list_keyboard_reorder::is_available(
        shared,
        super::track_list_keyboard_reorder::ReorderDirection::Top,
    ) || (context == MenuContext::Queue
        && track_list_queue_menu::selected_rows(shared).len() > 1);
    for (name, enabled) in [
        (ACTION_PLAY_NEXT, enqueue_enabled),
        (ACTION_ADD_TO_QUEUE, enqueue_enabled),
        (ACTION_MOVE_UP, move_up),
        (ACTION_MOVE_DOWN, move_down),
        (ACTION_MOVE_TO_TOP, move_to_top),
        (ACTION_GO_TO_ALBUM, states.go_to_album),
        (ACTION_GO_TO_ARTIST, states.go_to_artist),
        (ACTION_SHOW_IN_FILES, states.show_in_files),
        ("trash-selected-tracks", states.trash),
        (tag_edit_flow::ACTION_EDIT_TAGS, states.edit_tags),
    ] {
        let Some(action) = shared.menu_actions.lookup_action(name) else {
            continue;
        };
        if let Ok(action) = action.downcast::<gio::SimpleAction>() {
            action.set_enabled(enabled);
        }
    }
}

/// Builds the `"tracklist"` `gio::SimpleActionGroup` once (at `TrackList::
/// new` time) and inserts it on `column_view`; every menu item built by
/// `build_context_menu_model` refers to an action here by its detailed-
/// action string. Each handler reads the *current* selection at activation
/// time (`current_selection_positions`/`current_selection_ids`) rather than
/// anything captured when the menu was opened — the popover only exists
/// between open and click, so there's no meaningful window for the
/// selection to have changed underneath it, but reading it fresh costs
/// nothing and keeps this action group's wiring independent of any one
/// popover instance.
pub(in crate::ui) fn wire_context_menu_actions(
    column_view: &gtk4::ColumnView,
    shared: &Rc<Shared>,
) {
    let action_group = shared.menu_actions.clone();

    let play_action = gio::SimpleAction::new(ACTION_PLAY, None);
    {
        let shared = shared.clone();
        play_action.connect_activate(move |_, _| {
            handle_context_play(&shared);
        });
    }
    action_group.add_action(&play_action);

    let play_next_action = gio::SimpleAction::new(ACTION_PLAY_NEXT, None);
    {
        let shared = shared.clone();
        play_next_action.connect_activate(move |_, _| {
            let selection = current_playable_selection(&shared);
            track_list_queue_menu::play_next_selected(&shared, selection.ids());
        });
    }
    action_group.add_action(&play_next_action);

    let queue_action = gio::SimpleAction::new(ACTION_ADD_TO_QUEUE, None);
    {
        let shared = shared.clone();
        queue_action.connect_activate(move |_, _| {
            let selection = current_playable_selection(&shared);
            track_list_queue_menu::add_selected(&shared, selection.ids());
        });
    }
    action_group.add_action(&queue_action);

    for (name, direction) in [
        (
            ACTION_MOVE_UP,
            super::track_list_keyboard_reorder::ReorderDirection::Up,
        ),
        (
            ACTION_MOVE_DOWN,
            super::track_list_keyboard_reorder::ReorderDirection::Down,
        ),
    ] {
        let action = gio::SimpleAction::new(name, None);
        let shared = shared.clone();
        action.connect_activate(move |_, _| {
            super::track_list_keyboard_reorder::perform(&shared, direction);
        });
        action_group.add_action(&action);
    }

    let move_to_top_action = gio::SimpleAction::new(ACTION_MOVE_TO_TOP, None);
    {
        let shared = shared.clone();
        move_to_top_action.connect_activate(move |_, _| {
            if super::track_list_keyboard_reorder::perform(
                &shared,
                super::track_list_keyboard_reorder::ReorderDirection::Top,
            ) {
                return;
            }
            let rows = track_list_queue_menu::selected_rows(&shared);
            let callback = shared.on_queue_move_to_top.borrow().clone();
            let moved = callback.map_or(0, |callback| callback(&rows));
            if moved > 0 {
                show_toast(&shared, &strings::tracks_moved_to_top_toast(moved));
            }
        });
    }
    action_group.add_action(&move_to_top_action);

    track_list_queue_menu::add_remove_action(&action_group, shared);
    tag_edit_flow::add_action(&action_group, shared);
    delete_tracks::add_actions(&action_group, column_view, shared);

    let go_to_album_action = gio::SimpleAction::new(ACTION_GO_TO_ALBUM, None);
    {
        let shared = shared.clone();
        go_to_album_action.connect_activate(move |_, _| {
            let Some(track) = current_selection_tracks(&shared).into_iter().next() else {
                return;
            };
            let callback = shared.on_go_to_album.borrow().clone();
            if let Some(callback) = callback {
                callback(track.id, track.album, track.album_artist);
            }
        });
    }
    action_group.add_action(&go_to_album_action);

    let go_to_artist_action = gio::SimpleAction::new(ACTION_GO_TO_ARTIST, None);
    {
        let shared = shared.clone();
        go_to_artist_action.connect_activate(move |_, _| {
            let Some(track) = current_selection_tracks(&shared).into_iter().next() else {
                return;
            };
            let callback = shared.on_go_to_artist.borrow().clone();
            if let Some(callback) = callback {
                callback(track.id, track.album_artist);
            }
        });
    }
    action_group.add_action(&go_to_artist_action);

    let show_in_files_action = gio::SimpleAction::new(ACTION_SHOW_IN_FILES, None);
    {
        let shared = shared.clone();
        show_in_files_action.connect_activate(move |_, _| {
            let paths: Vec<_> = current_selection_tracks(&shared)
                .into_iter()
                .filter(|track| !track.is_missing())
                .map(|track| std::path::PathBuf::from(track.path))
                .collect();
            show_in_files::show_in_files(&paths);
        });
    }
    action_group.add_action(&show_in_files_action);

    let show_missing_action = gio::SimpleAction::new(ACTION_SHOW_IN_MISSING_FILES, None);
    {
        let shared = shared.clone();
        show_missing_action.connect_activate(move |_, _| {
            let callback = shared.on_show_missing_files.borrow().clone();
            if let Some(callback) = callback {
                callback();
            }
        });
    }
    action_group.add_action(&show_missing_action);

    let add_to_playlist_action =
        gio::SimpleAction::new(ACTION_ADD_TO_PLAYLIST, Some(glib::VariantTy::INT64));
    {
        let shared = shared.clone();
        add_to_playlist_action.connect_activate(move |_, parameter| {
            let Some(playlist_id) = parameter.and_then(glib::Variant::get::<i64>) else {
                tracing::warn!("context menu: add-to-playlist fired with no playlist id");
                return;
            };
            let ids = current_selection_ids(&shared);
            handle_add_to_playlist(&shared, playlist_id, &ids);
        });
    }
    action_group.add_action(&add_to_playlist_action);

    let new_playlist_action = gio::SimpleAction::new(ACTION_NEW_PLAYLIST, None);
    {
        let shared = shared.clone();
        new_playlist_action.connect_activate(move |_, _| {
            let ids = current_selection_ids(&shared);
            show_new_playlist_dialog(&shared, ids);
        });
    }
    action_group.add_action(&new_playlist_action);

    let remove_action = gio::SimpleAction::new(ACTION_REMOVE_FROM_PLAYLIST, None);
    {
        let shared = shared.clone();
        remove_action.connect_activate(move |_, _| {
            let positions = current_selection_positions(&shared);
            handle_remove_from_playlist(&shared, &positions);
        });
    }
    action_group.add_action(&remove_action);

    let remove_from_library_action = gio::SimpleAction::new(ACTION_REMOVE_FROM_LIBRARY, None);
    {
        let shared = shared.clone();
        remove_from_library_action.connect_activate(move |_, _| {
            let ids = current_selection_ids(&shared);
            handle_remove_from_library(&shared, &ids);
        });
    }
    action_group.add_action(&remove_from_library_action);

    column_view.insert_action_group(ACTION_GROUP_NAME, Some(&action_group));
    super::track_list_context_keys::wire(column_view, shared);
    super::track_list_keyboard_reorder::wire(column_view, shared);
}

/// Opens the row context menu for a secondary click at widget-local `(x,
/// y)` on `widget` (the specific Label/RatingWidget clicked), for the row
/// at `position` — see the module doc's `## GNOME right-click selection
/// convention` section for the reselect-if-not-selected behavior.
fn show_context_menu(
    shared: &Rc<Shared>,
    column_view: &gtk4::ColumnView,
    position: u32,
    widget: &gtk4::Widget,
    x: f64,
    y: f64,
) {
    if !shared.selection.is_selected(position) {
        shared.selection.select_range(position, 1, true);
    }

    let menu_model = build_context_menu_model(shared);
    let popover = gtk4::PopoverMenu::from_model(Some(&menu_model));
    popover.set_parent(column_view);
    popover.set_has_arrow(false);

    let click_point = graphene::Point::new(x as f32, y as f32);
    let target_point = widget
        .compute_point(column_view, &click_point)
        .unwrap_or(click_point);
    let rect = gtk4::gdk::Rectangle::new(target_point.x() as i32, target_point.y() as i32, 1, 1);
    popover.set_pointing_to(Some(&rect));

    // The popover parents itself to `column_view` above; unparent it once
    // closed so repeated right-clicks don't accumulate stale popovers as
    // children of the view.
    popover_lifecycle::unparent_after_actions(popover.upcast_ref());
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(column_view);
    focus_guard.restore_on_popover_close(popover.upcast_ref());

    popover.popup();
}

/// "Play" action handler (`ACTION_PLAY`) — see `ui::track_playback_
/// selection::context_play_decision`'s doc comment for the PLAY-4b
/// missing-aware semantics: an all-missing selection explains instead of
/// playing, a mixed selection plays only the playable ids.
pub(in crate::ui) fn handle_context_play(shared: &Rc<Shared>) {
    let positions = current_selection_positions(shared);
    match track_playback_selection::context_play_decision(&positions, &shared.model) {
        ContextPlayDecision::Play {
            ids,
            first_position,
        } => {
            if !track_list_queue_menu::play_position_if_queue(shared, first_position) {
                handle_play(shared, first_position, &ids);
            }
        }
        ContextPlayDecision::Explain(track) => {
            crate::ui::track_list_activation::explain_missing_track(shared, &track);
        }
        ContextPlayDecision::Noop => {
            tracing::debug!("context menu: play requested with nothing selected; ignoring");
        }
    }
}

/// Starts playback for a non-Queue source via the same `on_activate` seam
/// row activation uses (`ui::track_list_activation::activate_track`) — the
/// CTX unification folded the dedicated `on_play_selected` callback into
/// this single playback entry point. `first_position` resolves the
/// representative `Track` `on_activate` expects; `ids` (already filtered to
/// playable tracks by `context_play_decision`) with start index `0` are
/// exactly `PlayerController::play_from_view`'s parameters — Rhythmbox's
/// context-menu-play semantics: start at the first selected row, with every
/// other selected row queued right after it.
fn handle_play(shared: &Rc<Shared>, first_position: u32, ids: &[i64]) {
    let Some(track) = shared.model.track_at(first_position) else {
        tracing::warn!(
            first_position,
            "context menu: play action fired but no track at the first selected position"
        );
        return;
    };
    let count = ids.len();
    tracing::info!(count, "context menu: play action starting playback");
    let place = crate::ui::track_list::view_state_memory::capture_place(shared);
    (shared.on_activate)(&track, ids.to_vec(), 0, place);
}

/// Looks up `playlist_id`'s display name for a toast, falling back to a
/// generic placeholder if the lookup fails (e.g. the playlist was deleted
/// out from under a still-open menu) rather than failing the whole toast.
fn playlist_name_for_toast(shared: &Rc<Shared>, playlist_id: i64) -> String {
    let conn = &shared.conn;
    playlists::list(conn)
        .unwrap_or_default()
        .into_iter()
        .find(|p| p.id == playlist_id)
        .map_or_else(|| format!("playlist {playlist_id}"), |p| p.name)
}

/// "Add to playlist" action handler (`ACTION_ADD_TO_PLAYLIST`, existing
/// playlist chosen from the submenu). A no-op for an empty selection.
pub(in crate::ui) fn handle_add_to_playlist(shared: &Rc<Shared>, playlist_id: i64, ids: &[i64]) {
    if ids.is_empty() {
        tracing::debug!("context menu: add-to-playlist requested with nothing selected; ignoring");
        return;
    }
    let playlist_name = playlist_name_for_toast(shared, playlist_id);
    let drop_callback = shared.on_sidebar_playlist_drop.borrow().clone();
    if let Some(drop_callback) = drop_callback {
        drop_callback(playlist_id, &playlist_name, ids);
        return;
    }
    match track_actions::add_selected_to_playlist(&shared.conn, playlist_id, ids) {
        Ok(inserted) => {
            tracing::info!(
                playlist_id,
                inserted,
                "context menu: tracks added to playlist"
            );
            notify_playlist_mutated(shared);
            show_toast(
                shared,
                &strings::tracks_added_to_playlist_toast(inserted as usize, &playlist_name),
            );
            reload(shared);
        }
        Err(error) => {
            tracing::error!(
                %error,
                playlist_id,
                "context menu: failed to add tracks to playlist"
            );
            show_toast(
                shared,
                &strings::playlist_add_tracks_failed_toast(&playlist_name),
            );
        }
    }
}

/// "Remove from playlist" action handler (`ACTION_REMOVE_FROM_PLAYLIST`) —
/// only reachable from the menu while `source` is `ViewSource::Playlist`
/// (see `build_context_menu_model`'s guard), but re-checked here too since
/// the source could in principle change between the menu opening and this
/// firing. See `ui::track_actions`'s module doc for why this takes row
/// *positions*, not ids.
pub(in crate::ui) fn handle_remove_from_playlist(shared: &Rc<Shared>, positions: &[u32]) {
    let source = shared.source.borrow().clone();
    let ViewSource::Playlist(playlist_id) = source else {
        tracing::warn!(
            "context menu: remove-from-playlist fired outside a playlist source; ignoring"
        );
        return;
    };
    if positions.is_empty() {
        tracing::debug!(
            "context menu: remove-from-playlist requested with nothing selected; ignoring"
        );
        return;
    }
    match track_actions::remove_selected_from_playlist(
        &shared.conn,
        playlist_id,
        positions,
        &shared.model,
    ) {
        Ok(removed) => {
            tracing::info!(
                playlist_id,
                removed,
                "context menu: tracks removed from playlist"
            );
            notify_playlist_mutated(shared);
            show_toast(
                shared,
                &strings::tracks_removed_from_playlist_toast(removed as usize),
            );
            reload(shared);
        }
        Err(track_actions::RemoveFromPlaylistError::Unresolvable) => {
            // Safety backstop (see `ui::track_actions`'s module doc): abort
            // the whole remove rather than guess. Nothing was deleted, so
            // there's nothing to reload — just tell the user what happened.
            tracing::warn!(
                playlist_id,
                "context menu: could not resolve true playlist positions for the selected \
                 row(s); aborting remove entirely rather than guessing"
            );
            show_toast(
                shared,
                &strings::playlist_remove_tracks_unresolvable_toast(),
            );
        }
        Err(track_actions::RemoveFromPlaylistError::Db(error)) => {
            tracing::error!(
                %error,
                playlist_id,
                "context menu: failed to remove tracks from playlist"
            );
            show_toast(shared, &strings::playlist_remove_tracks_failed_toast());
        }
    }
}

/// "Remove from library" action handler (`ACTION_REMOVE_FROM_LIBRARY`,
/// Missing source only) — see `ui::track_actions::remove_missing_selected`'s
/// doc comment for the DB-only delete guarantee. Guarded on `ViewSource::
/// Missing` the same way `handle_remove_from_playlist` guards on
/// `ViewSource::Playlist`: the CTX-unified menu has no dedicated entry
/// pointing at this action (its generic `delete_tracks`-owned "Remove from
/// library…" covers every other source instead), so this stays reachable
/// only via the `REPRISE_SMOKE_MENU_ACTION=remove-from-library` hook, always
/// combined with a Missing-source selection. A no-op for an empty selection.
pub(in crate::ui) fn handle_remove_from_library(shared: &Rc<Shared>, ids: &[i64]) {
    if !matches!(*shared.source.borrow(), ViewSource::Missing) {
        tracing::warn!(
            "context menu: remove-from-library fired outside the Missing source; ignoring"
        );
        return;
    }
    if ids.is_empty() {
        tracing::debug!(
            "context menu: remove-from-library requested with nothing selected; ignoring"
        );
        return;
    }
    match track_actions::remove_missing_selected(&shared.conn, ids) {
        Ok(removed) => {
            tracing::info!(
                removed = removed.len(),
                "context menu: tracks removed from library"
            );
            notify_library_mutated(shared, &removed);
            show_toast(
                shared,
                &strings::tracks_removed_from_library_toast(removed.len()),
            );
            crate::ui::delete_tracks::reload_after_catalog_delete(shared);
        }
        Err(error) => {
            tracing::error!(%error, "context menu: failed to remove tracks from library");
            show_toast(shared, &strings::tracks_removed_from_library_failed_toast());
        }
    }
}

/// Clone-out-then-call `on_library_mutated` — see the `Shared::on_library_
/// mutated` doc comment in `track_list.rs`. `removed_ids` is the exact set
/// `queries::remove_missing_tracks` actually deleted, passed through so
/// `window.rs`'s wiring can purge those same ids from the playback queue
/// (`PlayerController::purge_queue_ids`).
fn notify_library_mutated(shared: &Rc<Shared>, removed_ids: &[i64]) {
    let callback = shared.on_library_mutated.borrow().clone();
    match callback {
        Some(callback) => callback(removed_ids),
        None => tracing::warn!(
            "context menu: library mutated but no on_library_mutated callback is wired"
        ),
    }
}

/// The context menu's "New playlist…" submenu leaf (`ACTION_NEW_PLAYLIST`):
/// prompts for a name via the shared `dialogs::prompt_name` helper (the same
/// helper `ui::sidebar`'s own "New playlist" dialog uses) and, on Create,
/// creates the playlist and appends `ids` to it in one step
/// (`ui::track_actions::create_playlist_and_add`) — the sidebar's own
/// `on_confirm` instead switches straight to the new playlist, which is the
/// only difference between the two call sites. A no-op (dialog not shown)
/// for an empty selection.
fn show_new_playlist_dialog(shared: &Rc<Shared>, ids: Vec<i64>) {
    if ids.is_empty() {
        tracing::debug!("context menu: new-playlist requested with nothing selected; ignoring");
        return;
    }
    let Some(window) = shared.window.upgrade() else {
        tracing::warn!("context menu: window is gone; cannot show new-playlist dialog");
        return;
    };

    let shared = shared.clone();
    dialogs::prompt_name(
        &window,
        &strings::text(strings::NEW_PLAYLIST_DIALOG_HEADING),
        &strings::text(strings::NEW_PLAYLIST_ENTRY_PLACEHOLDER),
        &strings::text(strings::CREATE),
        move |name| match track_actions::create_playlist_and_add(&shared.conn, &name, &ids) {
            Ok((playlist_id, inserted)) => {
                tracing::info!(
                    playlist_id,
                    name,
                    inserted,
                    "context menu: playlist created and tracks added"
                );
                notify_playlist_mutated(&shared);
                show_toast(
                    &shared,
                    &strings::tracks_added_to_playlist_toast(inserted as usize, &name),
                );
                reload(&shared);
            }
            Err(error) => {
                tracing::error!(%error, name, "context menu: failed to create playlist");
                show_toast(&shared, &strings::playlist_create_failed_toast(&name));
            }
        },
    );
}

/// Clone-out-then-call `on_playlist_mutated` (hoisted per this project's
/// `RefCell` callback discipline) — invoked after every successful context-
/// menu playlist mutation (add to existing, add to new, remove).
fn notify_playlist_mutated(shared: &Rc<Shared>) {
    let callback = shared.on_playlist_mutated.borrow().clone();
    match callback {
        Some(callback) => callback(),
        None => tracing::warn!(
            "context menu: playlist mutated but no on_playlist_mutated callback is wired"
        ),
    }
}
