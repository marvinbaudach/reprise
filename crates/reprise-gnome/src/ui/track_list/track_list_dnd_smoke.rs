//! The `REPRISE_SMOKE_DND` dev/verification hook — split out of `track_list_
//! dnd.rs` (Stage 3 Task 6 review finding #2, purely to keep that file under
//! the project's 800-line rule; no behavior change). Pointer drag gestures
//! aren't headless-drivable, so this invokes the *same* underlying functions
//! the real drop handlers call, once the initial load has run and the main
//! loop is idle. Three forms:
//!
//! - `addplaylist:<name>`: selects the first two rows (mirrors `track_list_
//!   context_menu`'s `REPRISE_SMOKE_MENU_ACTION=playlist:<name>`, which this
//!   deliberately parallels), resolves their ids, looks the playlist up by
//!   exact name (ids aren't stable across the scratch databases headless E2E
//!   runs seed fresh each time), then calls `shared.on_sidebar_playlist_
//!   drop` — the callback `window.rs` wires to `Sidebar::handle_playlist_
//!   drop` (Stage 3 Task 6 review finding #1). That is the *exact* function
//!   `ui::sidebar`'s real `gtk::DropTarget` calls on a genuine pointer drop:
//!   the database write, the sidebar's own `rebuild` + toast, and (through
//!   `Shared::on_tracks_added` -> `window.rs`'s wiring) this track list's own
//!   `reload` all run through this one call, exactly as they would for a
//!   real drag. This form previously called `library::playlists::add_tracks`
//!   directly, which only proved the database write, not any of the
//!   sidebar-refresh/toast wiring a real drop performs — see `Sidebar::
//!   handle_playlist_drop`'s doc comment for the fuller history.
//! - `addqueue`: the Queue-row twin of `addplaylist:<name>` — selects the
//!   same leading rows, resolves their ids, and calls `shared.on_sidebar_
//!   queue_drop`, the callback `window.rs` wires to `Sidebar::handle_queue_
//!   drop`. That is the exact function the Queue nav row's real `gtk::
//!   DropTarget` calls on a genuine pointer drop, so the append, its toast,
//!   and (through `PlayerController::notify_queue_changed`) the sidebar's
//!   Queue-count refresh plus Queue-view reload all run as they would for a
//!   real drag. Takes no name argument: there is only one queue.
//! - `reorderplaylist:<from>-<to>`: `from`/`to` are *view* positions — this
//!   builds the exact `DragPayload` a real single-row drag from view
//!   position `from` to view position `to` would produce (via `reorder_
//!   position_for_drag`, the identical resolution a real drag-prepare uses)
//!   and calls [`handle_playlist_reorder_drop`] — so this is what proves the
//!   TRUE-position rule under a sorted/filtered view: combine with `track_
//!   list.rs`'s `REPRISE_SMOKE_SORT_COLUMN` hook first, and this must then
//!   no-op (guard blocks it) rather than move the wrong row.
//! - `reorderqueue:<from>-<to>`: `from`/`to` are queue indices (== Queue view
//!   positions, unconditionally); builds the matching payload and calls
//!   [`handle_queue_reorder_drop`].
//!
//! Usage: `REPRISE_SCAN_DIR=… REPRISE_SMOKE_DND=addplaylist:MyList
//!  REPRISE_SMOKE_QUIT=1 xvfb-run -a cargo run`.

use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

use crate::ui::track_actions;
use crate::ui::track_list::{playlist_reorder_allowed, Shared};
use crate::ui::track_list_context_menu;
use crate::ui::track_list_dnd::{
    handle_playlist_reorder_drop, handle_queue_reorder_drop, reorder_position_for_drag, DragPayload,
};
use reprise_core::library::playlists;
use reprise_core::view_source::ViewSource;

const SMOKE_DND_ENV_VAR: &str = "REPRISE_SMOKE_DND";
/// Number of leading rows the `addplaylist:` form selects — mirrors `track_
/// list_context_menu`'s `SMOKE_MENU_ACTION_ROW_COUNT` (2 tracks is enough to
/// prove a multi-id drag payload resolves and inserts correctly).
const SMOKE_DND_ADD_ROW_COUNT: u32 = 2;

pub(in crate::ui) fn arm_smoke_dnd(shared: &Rc<Shared>) {
    let Ok(value) = std::env::var(SMOKE_DND_ENV_VAR) else {
        return;
    };
    tracing::info!(value = %value, "{SMOKE_DND_ENV_VAR} set: arming programmatic drag-and-drop simulation");
    let shared = shared.clone();
    glib::idle_add_local_once(move || {
        if let Some(name) = value.strip_prefix("addplaylist:") {
            smoke_add_to_playlist(&shared, name);
        } else if value == "addqueue" {
            smoke_add_to_queue(&shared);
        } else if let Some(range) = value.strip_prefix("reorderplaylist:") {
            smoke_reorder_playlist(&shared, range);
        } else if let Some(range) = value.strip_prefix("reorderqueue:") {
            smoke_reorder_queue(&shared, range);
        } else {
            tracing::warn!(value = %value, "{SMOKE_DND_ENV_VAR}: unrecognized value; ignoring");
        }
    });
}

/// Parses `"<from>-<to>"` (both forms of the hook) into a `(u32, u32)` pair
/// of view positions. `None` (caller warns) for anything else.
fn parse_from_to(range: &str) -> Option<(u32, u32)> {
    let (from, to) = range.split_once('-')?;
    Some((from.parse().ok()?, to.parse().ok()?))
}

/// `addplaylist:<name>` — see the module doc comment for why this dispatches
/// through `shared.on_sidebar_playlist_drop` rather than calling `library::
/// playlists::add_tracks` directly.
fn smoke_add_to_playlist(shared: &Rc<Shared>, name: &str) {
    let Some(ids) = select_leading_rows(shared) else {
        return;
    };

    let Some(playlist_id) = resolve_smoke_dnd_playlist_by_name(shared, name) else {
        tracing::warn!(
            name,
            "{SMOKE_DND_ENV_VAR}: no playlist found with this name"
        );
        return;
    };

    let callback = shared.on_sidebar_playlist_drop.borrow().clone();
    let Some(callback) = callback else {
        tracing::warn!(
            "{SMOKE_DND_ENV_VAR}: no sidebar playlist-drop handler wired (window.rs not fully \
             built yet?); ignoring"
        );
        return;
    };
    let added = callback(playlist_id, name, &ids);
    tracing::info!(
        playlist_id,
        added,
        "{SMOKE_DND_ENV_VAR}: simulated drop dispatched through the sidebar drop handler"
    );
}

/// Selects the leading rows a drag would carry and resolves their ids —
/// shared by the `addplaylist:<name>` and `addqueue` forms, which differ
/// only in where the resolved ids are then dropped. `None` (caller returns,
/// warning already logged) when the track list is empty.
fn select_leading_rows(shared: &Rc<Shared>) -> Option<Vec<i64>> {
    let row_count = shared.model.n_items().min(SMOKE_DND_ADD_ROW_COUNT);
    if row_count == 0 {
        tracing::warn!("{SMOKE_DND_ENV_VAR}: track list is empty; nothing to add");
        return None;
    }
    shared.selection.select_range(0, row_count, true);
    let positions = track_list_context_menu::current_selection_positions(shared);
    Some(track_actions::selected_track_ids(&positions, &shared.model))
}

/// `addqueue` — see the module doc comment for why this dispatches through
/// `shared.on_sidebar_queue_drop` rather than calling the player directly.
fn smoke_add_to_queue(shared: &Rc<Shared>) {
    let Some(ids) = select_leading_rows(shared) else {
        return;
    };

    let callback = shared.on_sidebar_queue_drop.borrow().clone();
    let Some(callback) = callback else {
        tracing::warn!(
            "{SMOKE_DND_ENV_VAR}: no sidebar queue-drop handler wired (window.rs not fully \
             built yet?); ignoring"
        );
        return;
    };
    let appended = callback(&ids);
    tracing::info!(
        count = ids.len(),
        appended,
        "{SMOKE_DND_ENV_VAR}: simulated queue drop dispatched through the sidebar drop handler"
    );
}

/// `reorderplaylist:<from>-<to>` — see the module doc comment.
fn smoke_reorder_playlist(shared: &Rc<Shared>, range: &str) {
    let Some((from, to)) = parse_from_to(range) else {
        tracing::warn!(
            range,
            "{SMOKE_DND_ENV_VAR}: malformed reorderplaylist range; ignoring"
        );
        return;
    };
    let ViewSource::Playlist(playlist_id) = shared.source.borrow().clone() else {
        tracing::warn!("{SMOKE_DND_ENV_VAR}: reorderplaylist requested outside a playlist source");
        return;
    };
    let Some(dragged_id) = shared.model.track_at(from).map(|t| t.id) else {
        tracing::warn!(
            from,
            "{SMOKE_DND_ENV_VAR}: no track at the 'from' view position"
        );
        return;
    };
    let allowed = playlist_reorder_allowed(shared);
    let source = shared.source.borrow().clone();
    let reorder_position = reorder_position_for_drag(&shared.model, &source, allowed, from);
    let payload = DragPayload {
        ids: vec![dragged_id],
        reorder_position,
    };
    let moved = handle_playlist_reorder_drop(shared, playlist_id, &payload, to);
    tracing::info!(
        playlist_id,
        from,
        to,
        moved,
        "{SMOKE_DND_ENV_VAR}: reorderplaylist simulated drop result"
    );
}

/// `reorderqueue:<from>-<to>` — see the module doc comment.
fn smoke_reorder_queue(shared: &Rc<Shared>, range: &str) {
    let Some((from, to)) = parse_from_to(range) else {
        tracing::warn!(
            range,
            "{SMOKE_DND_ENV_VAR}: malformed reorderqueue range; ignoring"
        );
        return;
    };
    if !matches!(*shared.source.borrow(), ViewSource::Queue) {
        tracing::warn!("{SMOKE_DND_ENV_VAR}: reorderqueue requested outside the queue source");
        return;
    }
    let Some(dragged_id) = shared.model.track_at(from).map(|t| t.id) else {
        tracing::warn!(
            from,
            "{SMOKE_DND_ENV_VAR}: no track at the 'from' view position"
        );
        return;
    };
    let payload = DragPayload {
        ids: vec![dragged_id],
        reorder_position: Some(i64::from(from)),
    };
    let moved = handle_queue_reorder_drop(shared, &payload, to);
    tracing::info!(
        from,
        to,
        moved,
        "{SMOKE_DND_ENV_VAR}: reorderqueue simulated drop result"
    );
}

/// Looks up a playlist id by exact name for the `addplaylist:<name>` form of
/// the hook — same reasoning (and same shape) as `track_list_context_menu`'s
/// `resolve_smoke_menu_action_playlist`/`track_list.rs`'s `resolve_smoke_
/// source_playlist_by_name`: playlist ids aren't stable across the scratch
/// databases headless E2E runs seed fresh each time. `None` (caller warns) if
/// the lookup fails or no playlist has that exact name; picks the first by
/// position on a name collision, same as the other two lookups.
fn resolve_smoke_dnd_playlist_by_name(shared: &Rc<Shared>, name: &str) -> Option<i64> {
    let conn = &shared.conn;
    let all = playlists::list(conn)
        .inspect_err(|error| {
            tracing::error!(%error, name, "failed to list playlists for smoke-dnd name lookup");
        })
        .ok()?;
    all.into_iter().find(|p| p.name == name).map(|p| p.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ## parse_from_to (smoke hook range parsing)

    #[test]
    fn parse_from_to_parses_a_valid_range() {
        assert_eq!(parse_from_to("0-2"), Some((0, 2)));
    }

    #[test]
    fn parse_from_to_rejects_malformed_ranges() {
        assert_eq!(parse_from_to("nope"), None);
        assert_eq!(parse_from_to("1-2-3"), None);
        assert_eq!(parse_from_to("-1-2"), None);
    }
}
