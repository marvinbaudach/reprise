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
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::library::playlists;
use crate::ui::strings;
use crate::ui::track_actions;
use crate::ui::track_list::{reload, show_toast, Shared};
use crate::view_source::ViewSource;

/// `AdwAlertDialog` response ids for the "New playlist…" dialog — internal
/// identifiers, not user-facing text (mirrors `ui::sidebar`'s own
/// `RESPONSE_CANCEL`/`RESPONSE_CREATE`, private to that module, hence
/// duplicated here rather than shared).
const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_CREATE: &str = "create";

/// Dev/verification hook (permanent, like the `REPRISE_SMOKE_*` hooks in
/// `track_list.rs`): when set, programmatically selects the first two rows
/// and invokes the exact same action functions (`ui::track_actions`) the
/// context menu's `gio::SimpleAction` handlers call — never the popover
/// itself, which needs a real pointer and is a manual-check item (see the
/// Task 5 report) — once the initial load has run and the main loop is
/// idle. Two accepted forms:
///
/// - `queue`: calls `track_actions::queue_selected_ids` then
///   `PlayerController::append_to_queue` (via `on_queue_selected`), logging
///   "N tracks added to queue".
/// - `playlist:<name>`: looks up the playlist by exact name (same fallback
///   `library::playlists::list` scan `track_list.rs`'s `REPRISE_SMOKE_SOURCE`
///   hook already uses for `playlist:<name>`, since playlist ids aren't
///   stable across the scratch databases headless E2E runs seed fresh each
///   time) and calls `track_actions::add_selected_to_playlist`, logging "N
///   tracks added".
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
/// The action group name every `"tracklist.*"` detailed-action string below
/// refers to — inserted once on `column_view` by `wire_context_menu_actions`.
const ACTION_GROUP_NAME: &str = "tracklist";

/// Attaches a secondary-click (`button = 3`) context-menu gesture to a
/// freshly-`setup` cell widget (a plain `Label` for the five text columns,
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
/// ascending order (`gtk::Bitset` iteration order).
fn current_selection_positions(shared: &Rc<Shared>) -> Vec<u32> {
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
        Some(strings::CONTEXT_MENU_PLAY),
        Some(&format!("{ACTION_GROUP_NAME}.{ACTION_PLAY}")),
    );
    primary.append(
        Some(strings::CONTEXT_MENU_ADD_TO_QUEUE),
        Some(&format!("{ACTION_GROUP_NAME}.{ACTION_ADD_TO_QUEUE}")),
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
        Some(strings::CONTEXT_MENU_NEW_PLAYLIST),
        Some(&format!("{ACTION_GROUP_NAME}.{ACTION_NEW_PLAYLIST}")),
    );
    menu.append_submenu(
        Some(strings::CONTEXT_MENU_ADD_TO_PLAYLIST),
        &playlist_submenu,
    );

    if matches!(*shared.source.borrow(), ViewSource::Playlist(_)) {
        let remove_section = gio::Menu::new();
        remove_section.append(
            Some(strings::CONTEXT_MENU_REMOVE_FROM_PLAYLIST),
            Some(&format!(
                "{ACTION_GROUP_NAME}.{ACTION_REMOVE_FROM_PLAYLIST}"
            )),
        );
        menu.append_section(None, &remove_section);
    }

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
    match track_actions::remove_selected_from_playlist(&shared.conn, playlist_id, positions) {
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
        Err(error) => {
            tracing::error!(
                %error,
                playlist_id,
                "context menu: failed to remove tracks from playlist"
            );
            show_toast(shared, &strings::playlist_remove_tracks_failed_toast());
        }
    }
}

/// The context menu's "New playlist…" submenu leaf (`ACTION_NEW_PLAYLIST`):
/// prompts for a name via an `AdwAlertDialog` (same shape as `ui::sidebar`'s
/// own "New playlist" dialog, but not shared code — the two live in
/// different modules, and this one adds the just-selected `ids` afterward
/// instead of the sidebar's own "switch straight to it" behavior) and, on
/// Create, creates the playlist and appends `ids` to it in one step
/// (`ui::track_actions::create_playlist_and_add`). A no-op (dialog not
/// shown) for an empty selection.
fn show_new_playlist_dialog(shared: &Rc<Shared>, ids: Vec<i64>) {
    if ids.is_empty() {
        tracing::debug!("context menu: new-playlist requested with nothing selected; ignoring");
        return;
    }
    let Some(window) = shared.window.upgrade() else {
        tracing::warn!("context menu: window is gone; cannot show new-playlist dialog");
        return;
    };

    let entry = gtk4::Entry::builder()
        .placeholder_text(strings::NEW_PLAYLIST_ENTRY_PLACEHOLDER)
        .activates_default(true)
        .build();

    let dialog = adw::AlertDialog::builder()
        .heading(strings::NEW_PLAYLIST_DIALOG_HEADING)
        .default_response(RESPONSE_CREATE)
        .close_response(RESPONSE_CANCEL)
        .extra_child(&entry)
        .build();
    dialog.add_response(RESPONSE_CANCEL, strings::CANCEL);
    dialog.add_response(RESPONSE_CREATE, strings::CREATE);
    dialog.set_response_appearance(RESPONSE_CREATE, adw::ResponseAppearance::Suggested);
    // Backend accepts an empty/whitespace-only name (`playlists::create`'s
    // doc comment) — this is the UI-side validation, matching `ui::
    // sidebar`'s own dialog.
    dialog.set_response_enabled(RESPONSE_CREATE, false);

    entry.connect_changed({
        let dialog = dialog.clone();
        move |entry| {
            let has_name = !entry.text().trim().is_empty();
            dialog.set_response_enabled(RESPONSE_CREATE, has_name);
        }
    });

    let shared = shared.clone();
    dialog.choose(Some(&window), gio::Cancellable::NONE, move |response| {
        if response.as_str() != RESPONSE_CREATE {
            return;
        }
        let name = entry.text().trim().to_string();
        match track_actions::create_playlist_and_add(&shared.conn, &name, &ids) {
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
        }
    });
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
        let ids = current_selection_ids(&shared);

        if value == "queue" {
            handle_add_to_queue(&shared, &ids);
            return;
        }

        let Some(playlist_name) = value.strip_prefix("playlist:") else {
            tracing::warn!(value = %value, "{SMOKE_MENU_ACTION_ENV_VAR}: unrecognized value; ignoring");
            return;
        };
        let Some(playlist_id) = resolve_smoke_menu_action_playlist(&shared, playlist_name) else {
            tracing::warn!(
                playlist_name,
                "{SMOKE_MENU_ACTION_ENV_VAR}: no playlist found with this name"
            );
            return;
        };
        handle_add_to_playlist(&shared, playlist_id, &ids);
    });
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
