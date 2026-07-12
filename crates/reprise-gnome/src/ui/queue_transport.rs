//! Queue-driven transport methods, split out of `player_controller.rs`
//! (Task 8) purely to keep that file under the project's file-size limit and
//! make room for the Now-Playing fan-out — same rationale, and same
//! `impl PlayerController` sibling-module shape, as the Stage 3 Task 1 split
//! that produced `mpris_mirror.rs`/`playback_faults.rs`. No behavioral
//! change: every method here moved verbatim.
//!
//! These are `pub(super)` so `mpris_mirror.rs`'s `handle_mpris_command` (and,
//! for `queue_ids_snapshot`, `track_list.rs`) can call them too — shared with
//! the bar's own button clicks so a physical media key and the on-screen
//! control run exactly one code path (DRY).
//!
//! Borrow discipline: every `queue` access here follows `player_controller.
//! rs`'s `## Queue borrow discipline` doc section — each borrow runs inside
//! its own statement/block, dropped before any call that could re-enter this
//! controller.

use crate::ui::player_controller::PlayerController;

impl PlayerController {
    /// Toggles play/pause on the player — shared by the bar's play/pause
    /// button and MPRIS's `PlayPause` method (see `player_controller.rs`'s
    /// `## MPRIS` doc section). Logs and no-ops on failure, matching the
    /// prior inline button-closure behavior.
    pub(super) fn toggle_pause(&self) {
        if let Err(error) = self.player.toggle_pause() {
            tracing::error!(%error, "toggle play/pause failed");
        }
    }

    /// Steps the queue to the previous track and plays it (or resets to
    /// stopped if there is none) — shared by the bar's previous button and
    /// MPRIS's `Previous` method. Borrow discipline: `previous()` runs
    /// inside this one `let` statement, so the borrow drops before
    /// `play_track_id`/`reset_to_stopped` run.
    pub(super) fn previous(&self) {
        let previous = self.queue.borrow_mut().previous();
        match previous {
            Some(id) => self.play_track_id(id),
            None => self.reset_to_stopped(),
        }
    }

    /// Steps the queue to the next track and plays it (or resets to stopped
    /// if there is none) — shared by the bar's next button and MPRIS's
    /// `Next` method. Same borrow discipline as `previous`.
    pub(super) fn next(&self) {
        let next = self.queue.borrow_mut().next_manual();
        match next {
            Some(id) => self.play_track_id(id),
            None => self.reset_to_stopped(),
        }
    }

    /// "Add to queue" context-menu action (Stage 3 Task 5): appends `ids` to
    /// the end of the current queue via `Queue::append_tracks` — see that
    /// method's doc comment for the exact append/no-auto-start semantics —
    /// without ever calling `play_track_id`. A no-op for an empty `ids`
    /// slice. If the queue was previously empty, the transport buttons are
    /// re-enabled to match its now-non-empty state (the same re-derivation
    /// `play_track_id` already does on every successful playback start), but
    /// no track starts playing: `ui::track_actions::queue_selected_ids`
    /// guards the empty case, and the queue itself only forms a `pos` of
    /// `Some(0)` for bookkeeping (see `Queue::append_tracks`) — playback
    /// stays exactly as it was. Borrow discipline: `append_tracks`/`is_
    /// empty` each run inside their own statement, so no `queue` borrow is
    /// alive across the `sync_transport_enabled` call.
    pub(super) fn append_to_queue(&self, ids: &[i64]) {
        if ids.is_empty() {
            tracing::debug!("append to queue: nothing to add; ignoring");
            return;
        }
        self.queue.borrow_mut().append_tracks(ids);
        let queue_len = self.queue.borrow().len();
        let queue_has_tracks = queue_len > 0;
        self.sync_transport_enabled(queue_has_tracks);
        tracing::info!(added = ids.len(), queue_len, "tracks added to queue");
    }

    /// Snapshot of every queued track id in current play order (Stage 3
    /// Task 3's `ViewSource::Queue` seam — see `queue::Queue::ids_in_order`'s
    /// doc comment). `track_list.rs`'s queue-ids provider closure (wired in
    /// `window::build`) calls this each time the track list reloads while
    /// showing the Queue source, so that view always reflects the queue's
    /// live state (including shuffle) rather than a stale copy. No explicit
    /// hoisting `let` is needed here: `ids_in_order()` returns an owned
    /// `Vec`, so the temporary `Ref` this creates already drops at the end
    /// of this one expression, before the function returns.
    pub(super) fn queue_ids_snapshot(&self) -> Vec<i64> {
        self.queue.borrow().ids_in_order()
    }

    /// Queue drag-reorder (Stage 3 Task 6): moves the queued track at index
    /// `from` to index `to` via `queue::Queue::move_item` — see that method's
    /// doc comment for the current-track-preservation contract and
    /// out-of-range/no-op handling (never panics). `ui::track_list_dnd`'s
    /// queue-reorder drop handler calls this via `TrackList::set_on_queue_
    /// reorder`, then reloads the track list itself so the Queue view picks
    /// up the new order — this method only mutates queue state, the same
    /// "state mutation only, caller decides what to refresh" contract as
    /// `append_to_queue`. Returns `Queue::move_item`'s own bool verbatim, so
    /// a no-op move (empty queue, out-of-range index, `from == to`) is
    /// reported as `false` rather than the caller assuming success just
    /// because a player was available to ask.
    pub(super) fn move_queue_item(&self, from: usize, to: usize) -> bool {
        self.queue.borrow_mut().move_item(from, to)
    }

    /// Purges hard-deleted track ids from the queue (Stage-3 close-out):
    /// "Remove from library" (`queries::remove_missing_tracks`) deletes
    /// `tracks` rows outright — without this, a queued id that no longer
    /// resolves to a row desyncs `Queue::len`/`ids_in_order` from what
    /// `ViewSource::Queue`'s window query can actually render (see
    /// `queries.rs`'s module doc, `Queue` section, and `query_track_count`'s
    /// `Queue` arm). Called from `ui::track_list_context_menu::handle_
    /// remove_from_library` with exactly the ids `remove_missing_tracks`
    /// reports as actually deleted — never the raw requested selection,
    /// which could include ids that turned out not to be missing any more
    /// and so were never deleted. A no-op for an empty slice (no `queue`
    /// borrow taken at all).
    pub(super) fn purge_queue_ids(&self, ids: &[i64]) {
        if ids.is_empty() {
            return;
        }
        let changed = self.queue.borrow_mut().remove_ids(ids);
        if changed {
            tracing::info!(
                removed = ids.len(),
                queue_len = self.queue.borrow().len(),
                "queue purged of hard-deleted track ids"
            );
        }
    }
}
