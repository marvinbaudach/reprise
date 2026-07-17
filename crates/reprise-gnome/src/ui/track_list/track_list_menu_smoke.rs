//! The `REPRISE_SMOKE_MENU_ACTION` dev/verification hook, split out of
//! `track_list_context_menu.rs` (file-size rule): selects the first rows of
//! the current view and drives the exact same handler functions the real
//! context-menu `gio::SimpleAction`s call — never the popover itself.

use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

use super::track_playback_selection;
use crate::ui::track_list::Shared;
use crate::ui::track_list_context_menu::{
    current_selection_ids, current_selection_positions, handle_add_to_playlist,
    handle_context_play, handle_remove_from_library, handle_remove_from_playlist, ACTION_PLAY_NEXT,
    ACTION_REMOVE_FROM_LIBRARY, ACTION_REMOVE_FROM_PLAYLIST,
};
use crate::ui::track_list_queue_menu;
use reprise_core::library::playlists;

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
pub(in crate::ui) fn arm_smoke_menu_action(shared: &Rc<Shared>) {
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
    if action == track_list_queue_menu::ACTION_REMOVE_FROM_QUEUE {
        track_list_queue_menu::remove_selected(shared);
        return;
    }

    if action == "play" {
        handle_context_play(shared);
        return;
    }

    let positions = current_selection_positions(shared);
    let playable = track_playback_selection::selected_playable_tracks(&positions, &shared.model);
    let ids = current_selection_ids(shared);

    if action == "queue" {
        track_list_queue_menu::add_selected(shared, playable.ids());
        return;
    }

    if action == ACTION_PLAY_NEXT {
        // QUE-3 acceptance: drives the real "Play next" handler so a
        // headless run can prove the Play Next section appears between Now
        // Playing and the snapshot tail, and plays first.
        track_list_queue_menu::play_next_selected(shared, playable.ids());
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
