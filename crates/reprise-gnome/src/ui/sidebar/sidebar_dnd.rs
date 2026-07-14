//! The sidebar's playlist-row drag-and-drop drop target (Stage 3 Task 6) —
//! split out of `sidebar.rs` (Stage 3 Task 6 review finding #2) purely to
//! keep that file under the project's 800-line rule, the same way `track_
//! list.rs` split `track_list_dnd.rs` out. Reaches into `sidebar.rs`'s
//! private `Shared` via `pub(super)` fields/functions, exactly the way
//! `track_list_dnd.rs` reaches into `track_list.rs`'s.
//!
//! ## Two entry points into [`handle_playlist_drop`] (Stage 3 Task 6 review
//! finding #1)
//!
//! [`handle_playlist_drop`] used to be inlined directly in [`wire_playlist_
//! drop_target`]'s `connect_drop` closure — untestable and undrivable from
//! outside a live `GtkDropTarget`, the same "logic embedded in a widget
//! callback" trap Task 5's data-loss bug lived in. It's now a standalone
//! function with exactly two callers, both running the identical sequence
//! (DB write, sidebar `rebuild`, toast, `on_tracks_added` notify):
//!
//! - [`wire_playlist_drop_target`]'s `connect_drop` closure — the real
//!   pointer-drag path.
//! - `Sidebar::handle_playlist_drop` (in `sidebar.rs`), which `window.rs`
//!   wires to `TrackList::set_on_sidebar_playlist_drop` — the seam `ui::
//!   track_list_dnd_smoke`'s `REPRISE_SMOKE_DND=addplaylist:<name>` hook
//!   calls, so headless verification exercises this exact function instead
//!   of only the lower-level `library::playlists::add_tracks` primitive.

use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

use crate::ui::sidebar::{rebuild, show_toast, Shared};
use crate::ui::strings;
use crate::ui::track_list_dnd;
use reprise_core::library::playlist_membership;

fn drop_added_rows(inserted: u32) -> bool {
    inserted > 0
}

/// Attaches a `gtk::DropTarget` to a playlist row (Stage 3 Task 6's DoD half:
/// "playlists fillable via drag and drop"): accepts the same `String`
/// drag-payload format `ui::track_list_dnd::wire_row_dnd`'s drag source
/// produces (see that module's `## Content payload format` section — the
/// reorder-position half of the payload is irrelevant here, only `ids`
/// matters). A parse failure (malformed/foreign payload) is a silent no-op
/// at this layer — dropping something this row doesn't understand should
/// never produce a toast or a log line at drop time, only `ui::track_list_
/// dnd::parse_drag_payload` itself (already exercised by that module's own
/// tests) needs to reason about *why* a payload didn't parse. Everything
/// past parsing is [`handle_playlist_drop`].
pub(super) fn wire_playlist_drop_target(
    shared: &Rc<Shared>,
    row: &gtk4::ListBoxRow,
    playlist_id: i64,
    playlist_name: &str,
) {
    let drop_target = gtk4::DropTarget::new(glib::Type::STRING, gtk4::gdk::DragAction::COPY);

    let shared = shared.clone();
    let playlist_name = playlist_name.to_string();
    drop_target.connect_drop(move |_target, value, _x, _y| {
        let Ok(payload_str) = value.get::<String>() else {
            return false;
        };
        let Some(payload) = track_list_dnd::parse_drag_payload(&payload_str) else {
            return false;
        };
        handle_playlist_drop(&shared, playlist_id, &playlist_name, &payload.ids)
    });

    row.add_controller(drop_target);
}

/// The actual "add dragged tracks to a playlist" logic — see the module
/// doc's `## Two entry points` section for why this is a standalone
/// function rather than inlined in a GTK closure. Appends every id in `ids`
/// to `playlist_id` via `library::playlists::add_tracks`, and — on success —
/// refreshes the sidebar's own counts (`rebuild`, same as `create_playlist_
/// and_select`), shows a toast, and notifies `on_tracks_added` so the track
/// list can pick up the change too (see that field's doc comment for why).
/// An empty `ids` is a no-op (`false`, no toast/log) — dropping something
/// this row doesn't understand (or a multi-id payload that parsed to
/// nothing) should never produce user-visible feedback. Returns whether
/// anything was actually added, mirroring `track_list_dnd`'s reorder
/// handlers' bool-return convention.
pub(super) fn handle_playlist_drop(
    shared: &Rc<Shared>,
    playlist_id: i64,
    playlist_name: &str,
    ids: &[i64],
) -> bool {
    if ids.is_empty() {
        return false;
    }

    let result = {
        let mut conn = shared.conn.borrow_mut();
        playlist_membership::add_unique_tracks(&mut conn, playlist_id, ids)
    };
    match result {
        Ok(inserted) => {
            if !drop_added_rows(inserted) {
                tracing::info!(
                    playlist_id,
                    "sidebar playlist drop skipped: every track is already present"
                );
                return false;
            }
            tracing::info!(
                playlist_id,
                inserted,
                "sidebar playlist drop: tracks added to playlist via drag and drop"
            );
            rebuild(shared, None, "tracks added via drag and drop");
            show_toast(
                shared,
                &strings::tracks_added_to_playlist_toast(inserted as usize, playlist_name),
            );
            let callback = shared.on_tracks_added.borrow().clone();
            if let Some(callback) = callback {
                callback();
            }
            true
        }
        Err(error) => {
            tracing::error!(%error, playlist_id, "failed to add tracks to playlist via drag and drop");
            show_toast(
                shared,
                &strings::playlist_drop_add_failed_toast(playlist_name),
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::drop_added_rows;

    #[test]
    fn duplicate_only_drop_reports_no_change() {
        assert!(!drop_added_rows(0));
        assert!(drop_added_rows(1));
    }
}
