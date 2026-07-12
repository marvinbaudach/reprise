//! The sidebar's playlist-row right-click context menu (Stage 3 Task 7):
//! currently a single "Export playlist…" action. Split out of `sidebar.rs`
//! for the same file-size reason as `sidebar_dnd.rs` — see that module's own
//! doc comment for the pattern this mirrors (reaching into `sidebar.rs`'s
//! private `Shared` via `pub(super)` fields/functions).
//!
//! The actual file-write + M3U-serialize logic lives in `ui::playlist_io`
//! (shared with the global "Import playlist…" flow in `window.rs`, and with
//! the `REPRISE_SMOKE_M3U=export:<name>:<path>` dev hook); this module owns
//! only the widget wiring: the right-click gesture, the `gio::Menu`/
//! `PopoverMenu`, and the `gtk::FileDialog` save flow.

use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;

use crate::ui::playlist_io;
use crate::ui::sidebar::{show_toast, Shared};
use crate::ui::strings;

/// Bare action name — internal identifier, not user-facing text (see
/// `&strings::text(strings::EXPORT_PLAYLIST)` for the menu item's actual copy). Mirrors
/// `ui::track_list_context_menu`'s `ACTION_*`/`ACTION_GROUP_NAME` naming.
const ACTION_EXPORT: &str = "export";
const ACTION_GROUP_NAME: &str = "playlistrow";

/// Attaches a secondary-click (`button = 3`) context-menu gesture to a
/// playlist row — same pattern as `ui::track_list_context_menu::wire_
/// context_menu_gesture`, applied to a sidebar `ListBoxRow` instead of a
/// `ColumnView` cell.
pub(super) fn wire_playlist_context_menu(
    shared: &Rc<Shared>,
    row: &gtk4::ListBoxRow,
    playlist_id: i64,
    playlist_name: &str,
) {
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);

    let shared = shared.clone();
    let row_for_popover = row.clone();
    let playlist_name = playlist_name.to_string();
    gesture.connect_pressed(move |gesture, _n_press, x, y| {
        // A right-click has no other meaning on a sidebar row.
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        show_context_menu(&shared, &row_for_popover, playlist_id, &playlist_name, x, y);
    });

    row.add_controller(gesture);
}

/// Builds and pops up the one-item context menu at the click point. The
/// `gio::SimpleActionGroup` is rebuilt on every open (like `ui::track_list_
/// context_menu::build_context_menu_model`) — cheap at one item, and avoids
/// any invalidation concern from a stale captured `playlist_id`/`name`.
fn show_context_menu(
    shared: &Rc<Shared>,
    row: &gtk4::ListBoxRow,
    playlist_id: i64,
    playlist_name: &str,
    x: f64,
    y: f64,
) {
    let action_group = gio::SimpleActionGroup::new();
    let export_action = gio::SimpleAction::new(ACTION_EXPORT, None);
    {
        let shared = shared.clone();
        let playlist_name = playlist_name.to_string();
        export_action.connect_activate(move |_, _| {
            start_export(&shared, playlist_id, &playlist_name);
        });
    }
    action_group.add_action(&export_action);
    row.insert_action_group(ACTION_GROUP_NAME, Some(&action_group));

    let menu = gio::Menu::new();
    menu.append(
        Some(&strings::text(strings::EXPORT_PLAYLIST)),
        Some(&format!("{ACTION_GROUP_NAME}.{ACTION_EXPORT}")),
    );

    let popover = gtk4::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(row);
    popover.set_has_arrow(false);

    let rect = gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
    popover.set_pointing_to(Some(&rect));
    // Unparent once closed, same as `ui::track_list_context_menu::show_
    // context_menu`, so repeated right-clicks don't accumulate stale
    // popovers as children of the row.
    popover.connect_closed(gtk4::prelude::WidgetExt::unparent);

    popover.popup();
}

/// Opens the "Export playlist…" save dialog and, on a chosen path, runs
/// `ui::playlist_io::export_playlist` — the same function the `REPRISE_
/// SMOKE_M3U=export:<name>:<path>` hook calls, so this dialog callback is a
/// thin wrapper, not a second implementation. Dismissing the dialog is a
/// normal, expected outcome (not an error), matching every other
/// `gtk::FileDialog` callback in this project.
fn start_export(shared: &Rc<Shared>, playlist_id: i64, playlist_name: &str) {
    let Some(window) = shared.window.upgrade() else {
        tracing::warn!("sidebar: window is gone; cannot show export-playlist dialog");
        return;
    };

    let filter = playlist_io::m3u_file_filter();
    let filters = gio::ListStore::new::<gtk4::FileFilter>();
    filters.append(&filter);
    let dialog = gtk4::FileDialog::builder()
        .title(strings::text(strings::EXPORT_PLAYLIST_DIALOG_TITLE))
        .modal(true)
        .filters(&filters)
        .default_filter(&filter)
        .initial_name(format!("{playlist_name}.m3u"))
        .build();

    let shared = shared.clone();
    let playlist_name = playlist_name.to_string();
    glib::spawn_future_local(async move {
        let file = match dialog.save_future(Some(&window)).await {
            Ok(file) => file,
            Err(error) => {
                if error.matches(gtk4::DialogError::Dismissed)
                    || error.matches(gtk4::DialogError::Cancelled)
                {
                    tracing::debug!("export playlist dialog dismissed");
                } else {
                    tracing::error!(%error, "export playlist dialog failed");
                }
                return;
            }
        };
        let Some(path) = file.path() else {
            tracing::warn!("chosen export path has no local filesystem path; cannot export");
            return;
        };

        match playlist_io::export_playlist(&shared.conn, playlist_id, &path) {
            Ok(count) => {
                tracing::info!(
                    playlist_id,
                    count,
                    path = %path.display(),
                    "playlist exported"
                );
                show_toast(&shared, &strings::playlist_exported_toast(&playlist_name));
            }
            Err(error) => {
                tracing::error!(%error, playlist_id, "playlist export failed");
                show_toast(
                    &shared,
                    &strings::playlist_export_failed_toast(&playlist_name),
                );
            }
        }
    });
}
