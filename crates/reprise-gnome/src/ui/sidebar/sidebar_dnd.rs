//! The sidebar's drag-and-drop drop targets — one per fillable nav row:
//! playlist rows (Stage 3 Task 6, [`wire_playlist_drop_target`]) and the
//! Queue row ([`wire_queue_drop_target`], the drag analogue of the context
//! menu's "Add to queue"). Both accept the identical payload `ui::track_
//! list_dnd`'s drag source produces and share the same shape: a thin
//! `connect_drop` closure that only parses, plus a standalone handler
//! ([`handle_playlist_drop`]/[`handle_queue_drop`]) holding the real logic —
//! see the `## Two entry points` section below for why. Originally the
//! playlist-row target alone (Stage 3 Task 6) —
//! split out of `sidebar.rs` (Stage 3 Task 6 review finding #2) purely to
//! keep that file under the project's 800-line rule, the same way `track_
//! list.rs` split `track_list_dnd.rs` out. Reaches into `sidebar.rs`'s
//! private `Shared` via `pub(in crate::ui)` fields/functions, exactly the way
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
//! - `Sidebar::handle_playlist_drop` (below), which `window.rs`
//!   wires to `TrackList::set_on_sidebar_playlist_drop` — the seam `ui::
//!   track_list_dnd_smoke`'s `REPRISE_SMOKE_DND=addplaylist:<name>` hook
//!   calls, so headless verification exercises this exact function instead
//!   of only the lower-level `library::playlists::add_tracks` primitive.

use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

use crate::ui::sidebar::{rebuild, show_toast, Shared, Sidebar};
use crate::ui::strings;
use crate::ui::track_list_dnd;
use reprise_core::library::playlist_membership;

/// Callback for a drag-and-drop drop onto the Queue nav row — see
/// `Shared::on_queue_drop`'s doc comment. Lives beside its drop handler
/// (relocated from `sidebar.rs`, orchestrator size rule).
pub(in crate::ui) type OnQueueDrop = std::rc::Rc<dyn Fn(&[i64]) -> bool>;
pub(in crate::ui) type OnConversionDrop = std::rc::Rc<dyn Fn(&[i64]) -> bool>;

impl Sidebar {
    pub fn set_on_conversion_drop(&self, callback: impl Fn(&[i64]) -> bool + 'static) {
        *self.shared.on_conversion_drop.borrow_mut() = Some(Rc::new(callback));
    }

    /// Drives the same drop-handling sequence as the real playlist-row drop
    /// target for callers that cannot synthesize a pointer drag.
    pub fn handle_playlist_drop(&self, playlist_id: i64, playlist_name: &str, ids: &[i64]) -> bool {
        handle_playlist_drop(&self.shared, playlist_id, playlist_name, ids)
    }
}

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
pub(in crate::ui) fn wire_playlist_drop_target(
    shared: &Rc<Shared>,
    row: &gtk4::ListBoxRow,
    playlist_id: i64,
    playlist_name: &str,
) {
    // input-parity: ACC-8 keyboard=context-menu-add
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
pub(in crate::ui) fn handle_playlist_drop(
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

/// Attaches a `gtk::DropTarget` to the Queue nav row — the Queue-row
/// analogue of [`wire_playlist_drop_target`], accepting the identical
/// payload format (only `ids` matters here too). Same silent-no-op-on-parse-
/// failure contract as the playlist target; everything past parsing is
/// [`handle_queue_drop`].
pub(in crate::ui) fn wire_queue_drop_target(shared: &Rc<Shared>, row: &gtk4::ListBoxRow) {
    // input-parity: ACC-8 keyboard=context-menu-add
    let drop_target = gtk4::DropTarget::new(glib::Type::STRING, gtk4::gdk::DragAction::COPY);

    let shared = shared.clone();
    drop_target.connect_drop(move |_target, value, _x, _y| {
        let Ok(payload_str) = value.get::<String>() else {
            return false;
        };
        let Some(payload) = track_list_dnd::parse_drag_payload(&payload_str) else {
            return false;
        };
        handle_queue_drop(&shared, &payload.ids)
    });

    row.add_controller(drop_target);
}

/// Makes the experimental Conversions row the second enqueue path specified by
/// the feature design: a multi-track drag becomes one instrumental batch.
pub(in crate::ui) fn wire_conversion_drop_target(shared: &Rc<Shared>, row: &gtk4::ListBoxRow) {
    // input-parity: ACC-8 keyboard=context-menu-create-instrumental
    let drop_target = gtk4::DropTarget::new(glib::Type::STRING, gtk4::gdk::DragAction::COPY);
    let shared = shared.clone();
    drop_target.connect_drop(move |_target, value, _x, _y| {
        let Ok(payload_str) = value.get::<String>() else {
            return false;
        };
        let Some(payload) = track_list_dnd::parse_drag_payload(&payload_str) else {
            return false;
        };
        handle_conversion_drop(&shared, &payload.ids)
    });
    row.add_controller(drop_target);
}

pub(in crate::ui) fn handle_conversion_drop(shared: &Rc<Shared>, ids: &[i64]) -> bool {
    let callback = shared.on_conversion_drop.borrow().clone();
    dispatch_conversion_drop(callback, ids)
}

fn dispatch_conversion_drop(callback: Option<OnConversionDrop>, ids: &[i64]) -> bool {
    if ids.is_empty() {
        return false;
    }
    let Some(callback) = callback else {
        tracing::warn!("conversion drop fired without a window callback");
        return false;
    };
    callback(ids)
}

/// The actual "append dropped tracks to the queue" logic — standalone for
/// the same two-entry-points reason as [`handle_playlist_drop`] (real drop
/// target here, `Sidebar::handle_queue_drop` for the `REPRISE_SMOKE_DND=
/// addqueue` smoke hook). Dispatches to `Shared::on_queue_drop` (wired by
/// `window.rs` to `PlayerController::append_to_queue` — see that field's
/// doc comment, including why no sidebar `rebuild` runs here) and shows the
/// same "N tracks added to queue" toast the context-menu action shows.
/// An empty `ids` is a no-op (`false`, callback never invoked), matching
/// [`handle_playlist_drop`]'s contract.
pub(in crate::ui) fn handle_queue_drop(shared: &Rc<Shared>, ids: &[i64]) -> bool {
    if ids.is_empty() {
        return false;
    }

    let callback = shared.on_queue_drop.borrow().clone();
    let Some(callback) = callback else {
        tracing::warn!("queue drop fired but no on_queue_drop callback is wired; ignoring");
        return false;
    };
    let appended = callback(ids);
    if appended {
        tracing::info!(
            count = ids.len(),
            "tracks appended to queue via drag and drop"
        );
        show_toast(shared, &strings::tracks_added_to_queue_toast(ids.len()));
    } else {
        tracing::debug!("queue drop callback reported no-op; skipping toast");
    }
    appended
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::{dispatch_conversion_drop, drop_added_rows};

    #[test]
    fn duplicate_only_drop_reports_no_change() {
        assert!(!drop_added_rows(0));
        assert!(drop_added_rows(1));
    }

    #[test]
    fn conversion_drop_dispatches_the_complete_drag_selection() {
        let observed = Rc::new(RefCell::new(Vec::new()));
        let callback = {
            let observed = observed.clone();
            Rc::new(move |ids: &[i64]| {
                *observed.borrow_mut() = ids.to_vec();
                true
            })
        };

        assert!(dispatch_conversion_drop(Some(callback), &[4, 2, 7]));
        assert_eq!(*observed.borrow(), vec![4, 2, 7]);
        assert!(!dispatch_conversion_drop(None, &[4]));
        assert!(!dispatch_conversion_drop(
            Some(Rc::new(|_: &[i64]| true)),
            &[]
        ));
    }
}
