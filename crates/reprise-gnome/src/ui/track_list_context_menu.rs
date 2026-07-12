//! Context menu + multi-select (Stage 3 Task 5) — split out of
//! `track_list.rs` exactly the way `player_controller.rs` split its MPRIS
//! mirror and fault-tolerance logic into `mpris_mirror.rs`/`playback_
//! faults.rs` (Stage 3 Task 1): same reasoning (keep the owning file from
//! growing without bound), same shape (this is an `impl`-free sibling module
//! that reaches into `track_list.rs`'s private `Shared` struct via
//! `pub(super)` fields/functions, not a separate type). `track_list.rs`
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

use crate::ui::delete_tracks;
use crate::ui::dialogs;
use crate::ui::strings;
use crate::ui::tag_edit_flow;
use crate::ui::track_actions;
use crate::ui::track_list::{reload, show_toast, Shared};
use reprise_core::library::playlists;
use reprise_core::view_source::ViewSource;

/// Dev/verification hook (permanent, like the `REPRISE_SMOKE_*` hooks in
/// `track_list.rs`): when set, programmatically selects the first two rows
/// and invokes the exact same action functions (`ui::track_actions`) the
/// context menu's `gio::SimpleAction` handlers call — never the popover
/// itself, which needs a real pointer and is a manual-check item (see the
/// Task 5 report) — once the initial load has run and the main loop is
/// idle. Two accepted forms:
///
/// - `play`: calls `handle_play` — the exact same handler `ACTION_PLAY`'s
///   `gio::SimpleAction` invokes — which starts playback at the first
///   selected row via `on_play_selected` (`PlayerController::play_from_
///   view`), with every other selected row queued right behind it. Added for
///   Stage 3 Task 9's closing E2E: `arm_smoke_menu_action` is armed *after*
///   `track_list.rs`'s `arm_smoke_source` (construction order in `TrackList::
///   new`), so — unlike `REPRISE_SMOKE_ACTIVATE`, whose own idle callback is
///   armed *before* `arm_smoke_source` and therefore always fires against
///   whatever source was active at construction (`ViewSource::Library`),
///   never a source a same-run `REPRISE_SMOKE_SOURCE` switch just applied —
///   this form reliably starts playback against the *current* (possibly
///   just-switched) source, e.g. combined with `REPRISE_SMOKE_SOURCE=
///   playlist:<name>` to prove playback follows a playlist's own order
///   headlessly. This also closes a pre-existing coverage gap: before this
///   form existed, the "Play" context-menu action had no smoke-hook coverage
///   at all (only "Add to queue"/"Add to playlist"/"Remove from playlist"
///   did).
/// - `queue`: calls `track_actions::queue_selected_ids` then
///   `PlayerController::append_to_queue` (via `on_queue_selected`), logging
///   "N tracks added to queue".
/// - `playlist:<name>`: looks up the playlist by exact name (same fallback
///   `library::playlists::list` scan `track_list.rs`'s `REPRISE_SMOKE_SOURCE`
///   hook already uses for `playlist:<name>`, since playlist ids aren't
///   stable across the scratch databases headless E2E runs seed fresh each
///   time) and calls `track_actions::add_selected_to_playlist`, logging "N
///   tracks added".
/// - `remove-from-playlist`: calls `handle_remove_from_playlist` with the
///   selected rows' raw *positions* (not ids) — the one form that exercises
///   `ui::track_actions::remove_selected_from_playlist`'s durable position-
///   resolution fix (Task 5 Fix Round 1). Combine with `track_list.rs`'s
///   `REPRISE_SMOKE_SOURCE=playlist:<name>` and `REPRISE_SMOKE_FILTER=<text>`
///   hooks to drive a sorted-or-filtered playlist view headlessly, then
///   inspect `playlist_tracks` directly (e.g. via `sqlite3`) to confirm the
///   *visible* row was removed, not whatever sits at `pt.position 0`.
/// - `remove-from-library` (Stage-3 close-out): calls `handle_remove_from_
///   library` — the exact handler `ACTION_REMOVE_FROM_LIBRARY`'s `gio::
///   SimpleAction` invokes — with the current selection's ids. Combine with
///   `track_list.rs`'s `REPRISE_SMOKE_SOURCE=missing` so the selection lands
///   on the Missing source's rows, then inspect `tracks`/`playlist_tracks`
///   directly to confirm: the row is gone, every playlist it belonged to
///   renumbered gaplessly, and (via the player's queue) any queued copy of
///   it purged — the property this task's fix restores.
///
/// Usage: `REPRISE_SCAN_DIR=… REPRISE_SMOKE_MENU_ACTION=queue
///  REPRISE_SMOKE_QUIT=1 xvfb-run -a cargo run`.
const SMOKE_MENU_ACTION_ENV_VAR: &str = "REPRISE_SMOKE_MENU_ACTION";
/// Number of leading rows the `REPRISE_SMOKE_MENU_ACTION` hook selects —
/// matches the E2E plan's "select first 2 rows" (Task 5 brief).
const SMOKE_MENU_ACTION_ROW_COUNT: u32 = 2;

/// Bare `gio::SimpleAction` names in the `"tracklist"` action group —
/// internal identifiers, not user-facing text (see `strings.rs` for the
/// menu item labels themselves).
const ACTION_PLAY: &str = "play";
const ACTION_ADD_TO_QUEUE: &str = "add-to-queue";
const ACTION_ADD_TO_PLAYLIST: &str = "add-to-playlist";
const ACTION_NEW_PLAYLIST: &str = "new-playlist";
const ACTION_REMOVE_FROM_PLAYLIST: &str = "remove-from-playlist";
/// Stage 3 Task 8: Missing-source-only actions — see `build_context_menu_
/// model`'s `ViewSource::Missing` arm.
const ACTION_RESCAN_LIBRARY: &str = "rescan-library";
const ACTION_REMOVE_FROM_LIBRARY: &str = "remove-from-library";
/// The action group name every `"tracklist.*"` detailed-action string below
/// refers to — inserted once on `column_view` by `wire_context_menu_actions`.
const ACTION_GROUP_NAME: &str = "tracklist";

/// Attaches a secondary-click (`button = 3`) context-menu gesture to a
/// freshly-`setup` cell widget (a plain `Label` for the seven text columns,
/// or the `RatingWidget` for the Rating column) — see the module doc's `##
/// Row position via a stable ListItem handle` section for why this only
/// needs to run once per widget instance, from `connect_setup`, with no
/// per-bind rewiring.
pub(super) fn wire_context_menu_gesture(
    widget: &impl IsA<gtk4::Widget>,
    item: &gtk4::ListItem,
    shared: &Rc<Shared>,
    column_view: &gtk4::ColumnView,
) {
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
/// ascending order (`gtk::Bitset` iteration order). `pub(super)` (not
/// private): `ui::track_list_dnd`'s drag-prepare handler reuses this exact
/// same read to decide whether a drag starting on an already-selected row
/// should carry the *whole* selection — see that module's doc comment.
pub(super) fn current_selection_positions(shared: &Rc<Shared>) -> Vec<u32> {
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
fn current_selection_ids(shared: &Rc<Shared>) -> Vec<i64> {
    let positions = current_selection_positions(shared);
    track_actions::selected_track_ids(&positions, &shared.model)
}

/// Builds the `gio::Menu` model shown by a right-click — rebuilt fresh on
/// every open (`show_context_menu`) rather than cached, since the playlist
/// submenu must always reflect the *current* set of playlists (one created
/// or renamed between two right-clicks must show up on the next one).
/// Building a handful of `gio::MenuItem`s per open is cheap enough that
/// caching would only add invalidation complexity for no real benefit at
/// this scale.
fn build_context_menu_model(shared: &Rc<Shared>) -> gio::Menu {
    let menu = gio::Menu::new();

    let primary = gio::Menu::new();
    primary.append(
        Some(&strings::text(strings::CONTEXT_MENU_PLAY)),
        Some(&format!("{ACTION_GROUP_NAME}.{ACTION_PLAY}")),
    );
    primary.append(
        Some(&strings::text(strings::CONTEXT_MENU_ADD_TO_QUEUE)),
        Some(&format!("{ACTION_GROUP_NAME}.{ACTION_ADD_TO_QUEUE}")),
    );
    primary.append(
        Some(&strings::text(strings::EDIT_TAGS)),
        Some(&format!(
            "{ACTION_GROUP_NAME}.{}",
            tag_edit_flow::ACTION_EDIT_TAGS
        )),
    );
    menu.append_section(None, &primary);

    let playlist_submenu = gio::Menu::new();
    let existing_playlists = {
        let conn = shared.conn.borrow();
        playlists::list(&conn).unwrap_or_else(|error| {
            tracing::error!(%error, "context menu: failed to list playlists");
            Vec::new()
        })
    };
    for playlist in &existing_playlists {
        let item = gio::MenuItem::new(Some(&playlist.name), None);
        item.set_action_and_target_value(
            Some(&format!("{ACTION_GROUP_NAME}.{ACTION_ADD_TO_PLAYLIST}")),
            Some(&playlist.id.to_variant()),
        );
        playlist_submenu.append_item(&item);
    }
    playlist_submenu.append(
        Some(&strings::text(strings::CONTEXT_MENU_NEW_PLAYLIST)),
        Some(&format!("{ACTION_GROUP_NAME}.{ACTION_NEW_PLAYLIST}")),
    );
    menu.append_submenu(
        Some(&strings::text(strings::CONTEXT_MENU_ADD_TO_PLAYLIST)),
        &playlist_submenu,
    );

    if matches!(*shared.source.borrow(), ViewSource::Playlist(_)) {
        let remove_section = gio::Menu::new();
        remove_section.append(
            Some(&strings::text(strings::CONTEXT_MENU_REMOVE_FROM_PLAYLIST)),
            Some(&format!(
                "{ACTION_GROUP_NAME}.{ACTION_REMOVE_FROM_PLAYLIST}"
            )),
        );
        menu.append_section(None, &remove_section);
    }

    // Stage 3 Task 8: the Missing source's problem-source actions — "Rescan
    // library" acts on the persisted library root regardless of selection
    // (so it's offered even with nothing selected), "Remove from library"
    // acts on the current selection, exactly like "Remove from playlist"
    // above.
    if matches!(*shared.source.borrow(), ViewSource::Missing) {
        let missing_section = gio::Menu::new();
        missing_section.append(
            Some(&strings::text(strings::CONTEXT_MENU_RESCAN_LIBRARY)),
            Some(&format!("{ACTION_GROUP_NAME}.{ACTION_RESCAN_LIBRARY}")),
        );
        menu.append_section(None, &missing_section);
    }

    delete_tracks::append_menu_section(&menu, ACTION_GROUP_NAME);

    menu
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
pub(super) fn wire_context_menu_actions(column_view: &gtk4::ColumnView, shared: &Rc<Shared>) {
    let action_group = gio::SimpleActionGroup::new();

    let play_action = gio::SimpleAction::new(ACTION_PLAY, None);
    {
        let shared = shared.clone();
        play_action.connect_activate(move |_, _| {
            let ids = current_selection_ids(&shared);
            handle_play(&shared, &ids);
        });
    }
    action_group.add_action(&play_action);

    let queue_action = gio::SimpleAction::new(ACTION_ADD_TO_QUEUE, None);
    {
        let shared = shared.clone();
        queue_action.connect_activate(move |_, _| {
            let ids = current_selection_ids(&shared);
            handle_add_to_queue(&shared, &ids);
        });
    }
    action_group.add_action(&queue_action);
    tag_edit_flow::add_action(&action_group, shared);
    delete_tracks::add_actions(&action_group, column_view, shared);

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

    let rescan_library_action = gio::SimpleAction::new(ACTION_RESCAN_LIBRARY, None);
    {
        let shared = shared.clone();
        rescan_library_action.connect_activate(move |_, _| handle_rescan_library(&shared));
    }
    action_group.add_action(&rescan_library_action);

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
    popover.connect_closed(gtk4::prelude::WidgetExt::unparent);

    popover.popup();
}

/// "Play" action handler (`ACTION_PLAY`) — see `ui::track_actions::
/// play_selected_ids`'s doc comment for the semantics.
fn handle_play(shared: &Rc<Shared>, ids: &[i64]) {
    let Some((ids, start_index)) = track_actions::play_selected_ids(ids) else {
        tracing::debug!("context menu: play requested with nothing selected; ignoring");
        return;
    };
    let count = ids.len();
    let callback = shared.on_play_selected.borrow().clone();
    match callback {
        Some(callback) => callback(ids, start_index),
        None => tracing::warn!(
            count,
            "context menu: play action fired but no on_play_selected callback is wired"
        ),
    }
}

/// "Add to queue" action handler (`ACTION_ADD_TO_QUEUE`).
fn handle_add_to_queue(shared: &Rc<Shared>, ids: &[i64]) {
    let Some(ids) = track_actions::queue_selected_ids(ids) else {
        tracing::debug!("context menu: add-to-queue requested with nothing selected; ignoring");
        return;
    };
    let count = ids.len();
    let callback = shared.on_queue_selected.borrow().clone();
    match callback {
        Some(callback) => {
            callback(ids);
            tracing::info!(count, "context menu: tracks added to queue");
            show_toast(shared, &strings::tracks_added_to_queue_toast(count));
        }
        None => tracing::warn!(
            count,
            "context menu: add-to-queue action fired but no on_queue_selected callback is wired"
        ),
    }
}

/// Looks up `playlist_id`'s display name for a toast, falling back to a
/// generic placeholder if the lookup fails (e.g. the playlist was deleted
/// out from under a still-open menu) rather than failing the whole toast.
fn playlist_name_for_toast(shared: &Rc<Shared>, playlist_id: i64) -> String {
    let conn = shared.conn.borrow();
    playlists::list(&conn)
        .unwrap_or_default()
        .into_iter()
        .find(|p| p.id == playlist_id)
        .map_or_else(|| format!("playlist {playlist_id}"), |p| p.name)
}

/// "Add to playlist" action handler (`ACTION_ADD_TO_PLAYLIST`, existing
/// playlist chosen from the submenu). A no-op for an empty selection.
fn handle_add_to_playlist(shared: &Rc<Shared>, playlist_id: i64, ids: &[i64]) {
    if ids.is_empty() {
        tracing::debug!("context menu: add-to-playlist requested with nothing selected; ignoring");
        return;
    }
    let playlist_name = playlist_name_for_toast(shared, playlist_id);
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
fn handle_remove_from_playlist(shared: &Rc<Shared>, positions: &[u32]) {
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

/// "Rescan library" action handler (`ACTION_RESCAN_LIBRARY`, Missing source
/// only, Stage 3 Task 8) — hoisted clone-out then call, per this project's
/// `RefCell` callback discipline; `window.rs`'s `trigger_rescan_of_library_
/// root` (wired via `TrackList::set_on_rescan_library`) owns the actual scan
/// flow and its own toasts, so there is nothing further to do here on
/// either outcome.
fn handle_rescan_library(shared: &Rc<Shared>) {
    let callback = shared.on_rescan_library.borrow().clone();
    match callback {
        Some(callback) => callback(),
        None => tracing::warn!(
            "context menu: rescan-library fired but no on_rescan_library callback is wired"
        ),
    }
}

/// "Remove from library" action handler (`ACTION_REMOVE_FROM_LIBRARY`,
/// Missing source only, Stage 3 Task 8) — see `ui::track_actions::remove_
/// missing_selected`'s doc comment for the DB-only delete guarantee. A no-op
/// for an empty selection.
fn handle_remove_from_library(shared: &Rc<Shared>, ids: &[i64]) {
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
            reload(shared);
        }
        Err(error) => {
            tracing::error!(%error, "context menu: failed to remove tracks from library");
            show_toast(shared, &strings::tracks_removed_from_library_failed_toast());
        }
    }
}

/// Clone-out-then-call `on_library_mutated` (Stage 3 Task 8) — see the
/// `Shared::on_library_mutated` doc comment in `track_list.rs`. `removed_
/// ids` is the exact set `queries::remove_missing_tracks` actually deleted
/// (Stage-3 close-out), passed through so `window.rs`'s wiring can purge
/// those same ids from the playback queue.
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

/// Arms the `REPRISE_SMOKE_MENU_ACTION` hook (see
/// `SMOKE_MENU_ACTION_ENV_VAR`): one idle callback, deferred so it runs once
/// the main loop is up (matching every other `arm_smoke_*` hook in
/// `track_list.rs`), that selects the first `SMOKE_MENU_ACTION_ROW_COUNT`
/// rows via `shared.selection` and invokes the exact same `handle_add_to_
/// queue`/`handle_add_to_playlist` functions the menu's `gio::SimpleAction`
/// handlers call — never the popover itself (see the const's doc comment).
///
/// Stage-3 close-out: `value` may be a comma-separated LIST of actions
/// (e.g. `queue,remove-from-library`), run in order against the *same*
/// selection — this is what lets one headless run prove "a track that's
/// queued, then hard-deleted, gets purged from the queue": a track already
/// flagged `missing` is invisible to every OTHER source's own queries, but
/// `ViewSource::Missing` (switched to beforehand via `REPRISE_SMOKE_SOURCE`)
/// shows it, so the selection here can queue it there first (queueing a
/// still-existing-but-missing track is a legitimate action — nothing gates
/// "Add to queue" by source) and then remove it, in the same callback,
/// exactly mirroring the real sequence "queue a track, later its file
/// disappears (or, here, a scan already flagged it missing), then Remove
/// from library" a real user session could produce.
pub(super) fn arm_smoke_menu_action(shared: &Rc<Shared>) {
    let Ok(value) = std::env::var(SMOKE_MENU_ACTION_ENV_VAR) else {
        return;
    };
    tracing::info!(value = %value, "{SMOKE_MENU_ACTION_ENV_VAR} set: arming programmatic menu action");
    let shared = shared.clone();
    glib::idle_add_local_once(move || {
        let row_count = shared.model.n_items().min(SMOKE_MENU_ACTION_ROW_COUNT);
        if row_count == 0 {
            tracing::warn!("{SMOKE_MENU_ACTION_ENV_VAR}: track list is empty; nothing to select");
            return;
        }
        shared.selection.select_range(0, row_count, true);

        for action in value.split(',').map(str::trim) {
            dispatch_smoke_menu_action(&shared, action);
        }
    });
}

/// Runs one `,`-separated token of the `REPRISE_SMOKE_MENU_ACTION` value
/// against the CURRENT selection (`arm_smoke_menu_action` selects once,
/// before the whole list runs) — see that function's doc comment for why a
/// list of actions shares one selection.
fn dispatch_smoke_menu_action(shared: &Rc<Shared>, action: &str) {
    if action == ACTION_REMOVE_FROM_PLAYLIST {
        // Positions, not ids — exercises the exact same position-resolution
        // path a real right-click "Remove from playlist" uses (see `ui::
        // track_actions`'s module doc), so this is what lets the hook drive
        // the Fix Round 1 sorted/filtered-playlist E2E check headlessly.
        let positions = current_selection_positions(shared);
        handle_remove_from_playlist(shared, &positions);
        return;
    }

    let ids = current_selection_ids(shared);

    if action == "play" {
        handle_play(shared, &ids);
        return;
    }

    if action == "queue" {
        handle_add_to_queue(shared, &ids);
        return;
    }

    if action == ACTION_REMOVE_FROM_LIBRARY {
        // Stage-3 close-out: drives `handle_remove_from_library` (the exact
        // function the real "Remove from library" menu item calls)
        // headlessly, so an E2E run can prove the hard-delete's fallout —
        // playlist-position compaction, queue purge — without a human
        // right-clicking a Missing-source row. Selects the first `SMOKE_
        // MENU_ACTION_ROW_COUNT` rows of the CURRENT view like every other
        // action here; a run that first switches to `ViewSource::Missing`
        // (via `REPRISE_SMOKE_SOURCE`) with exactly one missing track
        // selects exactly that one row.
        handle_remove_from_library(shared, &ids);
        return;
    }

    let Some(playlist_name) = action.strip_prefix("playlist:") else {
        tracing::warn!(value = %action, "{SMOKE_MENU_ACTION_ENV_VAR}: unrecognized value; ignoring");
        return;
    };
    let Some(playlist_id) = resolve_smoke_menu_action_playlist(shared, playlist_name) else {
        tracing::warn!(
            playlist_name,
            "{SMOKE_MENU_ACTION_ENV_VAR}: no playlist found with this name"
        );
        return;
    };
    handle_add_to_playlist(shared, playlist_id, &ids);
}

/// Looks up a playlist id by exact name for the `REPRISE_SMOKE_MENU_ACTION=
/// playlist:<name>` hook — same reasoning as `track_list.rs`'s own
/// `resolve_smoke_source_playlist_by_name` (playlist ids aren't stable
/// across the scratch databases headless E2E runs seed fresh each time, so
/// the hook takes a name instead of an id). `None` (caller warns) if the
/// lookup fails or no playlist has that exact name; picks the first by
/// position on a name collision, same as the other lookup.
fn resolve_smoke_menu_action_playlist(shared: &Rc<Shared>, name: &str) -> Option<i64> {
    let conn = shared.conn.borrow();
    let all = playlists::list(&conn)
        .inspect_err(|error| {
            tracing::error!(
                %error,
                name,
                "failed to list playlists for smoke-menu-action name lookup"
            );
        })
        .ok()?;
    all.into_iter().find(|p| p.name == name).map(|p| p.id)
}
