/// Queue engine: a pure Rust module (no GTK, no DB) for track queuing, playback order,
/// shuffle, and repeat modes. Uses Fisher-Yates shuffle via fastrand for determinism.
use tracing::warn;

mod snapshot;
pub use snapshot::{QueueSnapshot, QueueSnapshotError};

/// Repeat mode for the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

/// Queue state: tracks, playback order (possibly shuffled), current position, and repeat mode.
///
/// Invariant: if pos is Some(idx), then idx < order.len(), and order.len() == ids.len().
/// If ids is empty, pos is None.
#[derive(Debug, Clone)]
pub struct Queue {
    ids: Vec<i64>,
    order: Vec<usize>,
    pos: Option<usize>,
    repeat: Repeat,
    shuffled: bool,
}

impl Queue {
    /// Create a new, empty queue.
    pub fn new() -> Self {
        Self {
            ids: Vec::new(),
            order: Vec::new(),
            pos: None,
            repeat: Repeat::default(),
            shuffled: false,
        }
    }

    /// Set the queue to a list of track IDs and start playback at `start_index`.
    /// If `start_index` is out of range, it is clamped to the last track (or None if empty).
    /// If shuffle is active, re-shuffle the new order while keeping the start_index track as current.
    pub fn set_tracks(&mut self, ids: Vec<i64>, start_index: usize) {
        self.ids = ids;
        let len = self.ids.len();

        // Initialize order as linear indices.
        self.order = (0..len).collect();

        // Set position, clamping start_index to valid range.
        self.pos = if len == 0 {
            None
        } else {
            Some(start_index.min(len - 1))
        };

        // If shuffle is sticky, re-shuffle the new order while keeping current track in place
        if self.shuffled && len > 0 {
            if let Some(current_pos) = self.pos {
                // Remember the current track's track-index
                let current_track_slot = self.order.get(current_pos).copied();

                // Fisher-Yates shuffle: permute indices
                let n = self.order.len();
                for i in (1..n).rev() {
                    let j = fastrand::usize(0..=i);
                    self.order.swap(i, j);
                }

                // Move current track back to its position
                if let Some(track_slot) = current_track_slot {
                    if let Some(pos) = self.order.iter().position(|&idx| idx == track_slot) {
                        self.order.swap(current_pos, pos);
                    } else {
                        warn!(
                            "Current track slot {} not found in order after shuffle",
                            track_slot
                        );
                    }
                }
            }
        }
    }

    /// Get the ID of the currently playing track, if any.
    pub fn current(&self) -> Option<i64> {
        self.pos.and_then(|idx| {
            self.order
                .get(idx)
                .and_then(|&track_idx| self.ids.get(track_idx).copied())
        })
    }

    /// Advance to the next track automatically (track finished naturally).
    /// If Repeat::One, return the same track.
    /// If Repeat::All, wrap to the first track at the end.
    /// If Repeat::Off, return None at the end and clear position.
    pub fn advance_auto(&mut self) -> Option<i64> {
        match self.pos {
            None => None,
            Some(idx) => {
                if self.repeat == Repeat::One {
                    // Stay on the current track.
                    self.current()
                } else {
                    // Move to the next track in order.
                    let next_idx = idx + 1;
                    if next_idx < self.order.len() {
                        self.pos = Some(next_idx);
                        self.current()
                    } else {
                        // At the end.
                        if self.repeat == Repeat::All {
                            self.pos = Some(0);
                            self.current()
                        } else {
                            self.pos = None;
                            None
                        }
                    }
                }
            }
        }
    }

    /// Move to the next track (user pressed next button).
    /// Ignores Repeat::One, always moves forward (or wraps if Repeat::All).
    /// Returns None if at the end and Repeat::Off.
    pub fn next_manual(&mut self) -> Option<i64> {
        match self.pos {
            None => None,
            Some(idx) => {
                let next_idx = idx + 1;
                if next_idx < self.order.len() {
                    self.pos = Some(next_idx);
                    self.current()
                } else {
                    // At the end.
                    if self.repeat == Repeat::All {
                        self.pos = Some(0);
                        self.current()
                    } else {
                        self.pos = None;
                        None
                    }
                }
            }
        }
    }

    /// Move to the previous track (user pressed previous button).
    /// At the first track, stay on the first track.
    /// If queue is exhausted (pos == None) and queue is non-empty, resume at the last track.
    /// Empty queue returns None.
    pub fn previous(&mut self) -> Option<i64> {
        match self.pos {
            None => {
                // Queue exhausted; if non-empty, resume at the last track
                if !self.order.is_empty() {
                    self.pos = Some(self.order.len() - 1);
                    self.current()
                } else {
                    None
                }
            }
            Some(idx) => {
                if idx == 0 {
                    // Already at the first track; stay here.
                    self.current()
                } else {
                    self.pos = Some(idx - 1);
                    self.current()
                }
            }
        }
    }

    /// Enable or disable shuffle mode.
    /// When enabling: Fisher-Yates shuffle the order, keeping the current track at its current position.
    /// When disabling: restore linear order, keeping the current track at its linear index.
    pub fn set_shuffle(&mut self, on: bool) {
        if on && !self.shuffled {
            // Currently linear; shuffle while keeping current track in place.
            if let Some(current_pos) = self.pos {
                // Remember the current track's track-index (defensive: use .get())
                let current_track_slot = match self.order.get(current_pos) {
                    Some(&slot) => slot,
                    None => {
                        warn!(
                            "Current position {} out of bounds for order vec of len {}",
                            current_pos,
                            self.order.len()
                        );
                        // Guard failed: bail out BEFORE flipping `shuffled`,
                        // not after (Stage 3 Task 1) — this branch is
                        // unreachable under the struct's own invariant
                        // (`pos < order.len()` always), but setting
                        // `shuffled = true` here first would have desynced
                        // the flag from the order actually still being
                        // linear, had it ever been reached.
                        return;
                    }
                };
                self.shuffled = true;

                // Fisher-Yates shuffle: permute indices, but skip current track.
                let n = self.order.len();
                for i in (1..n).rev() {
                    let j = fastrand::usize(0..=i);
                    self.order.swap(i, j);
                }

                // Move current track back to its position.
                if let Some(pos) = self.order.iter().position(|&idx| idx == current_track_slot) {
                    self.order.swap(current_pos, pos);
                }
            } else {
                // No current track; just shuffle normally. No guard to fail
                // here, so `shuffled` is set unconditionally.
                self.shuffled = true;
                let n = self.order.len();
                for i in (1..n).rev() {
                    let j = fastrand::usize(0..=i);
                    self.order.swap(i, j);
                }
            }
        } else if !on && self.shuffled {
            // Currently shuffled; restore linear order.
            self.shuffled = false;
            let current_track_slot = self.pos.and_then(|idx| self.order.get(idx).copied());

            // Restore linear order.
            self.order = (0..self.ids.len()).collect();

            // Update position to follow the current track's linear index.
            if let Some(id_idx) = current_track_slot {
                self.pos = Some(id_idx);
            }
        }
    }

    /// Check if shuffle is currently enabled.
    pub fn is_shuffled(&self) -> bool {
        self.shuffled
    }

    /// Set the repeat mode.
    pub fn set_repeat(&mut self, r: Repeat) {
        self.repeat = r;
    }

    /// Get the current repeat mode.
    pub fn repeat(&self) -> Repeat {
        self.repeat
    }

    /// Get the number of tracks in the queue.
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Check if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Appends `new_ids` to the end of the queue (Stage 3 Task 5's "Add to
    /// queue" context-menu action) — deliberately not a re-run of `set_
    /// tracks` (which replaces the whole queue and resets/reshuffles it).
    /// The current track (`pos`) is left untouched: appending is purely
    /// additive. Both the linear and shuffled cases append the new ids to
    /// the *tail* of `order` in the order given, rather than weaving them
    /// into an existing shuffled prefix — "add to end" is a deliberate,
    /// predictable action, not an invitation to reshuffle. A no-op for an
    /// empty `new_ids` slice (nothing to append).
    ///
    /// If the queue was truly empty before this call (`ids` had zero
    /// elements — the only case `Queue`'s own invariant ties to `pos ==
    /// None`), the appended tracks become the queue and `pos` becomes
    /// `Some(0)` so the invariant holds again. This does *not* apply to a
    /// queue that is merely *exhausted* (`pos == None` but `ids` already
    /// non-empty, e.g. `Repeat::Off` ran off the end) — appending more
    /// tracks there must not silently resurrect a position, since nothing
    /// asked for playback to resume. Either way, this method never starts
    /// playback itself; it only updates queue state. Whether/when to
    /// actually start playing is entirely the caller's decision (see `ui::
    /// player_controller::PlayerController::append_to_queue`, which never
    /// calls play for this action, matching the Rhythmbox-style "add to
    /// queue" contract).
    pub fn append_tracks(&mut self, new_ids: &[i64]) {
        if new_ids.is_empty() {
            return;
        }

        let was_empty = self.ids.is_empty();
        let start_idx = self.ids.len();
        self.ids.extend_from_slice(new_ids);
        let new_len = self.ids.len();
        self.order.extend(start_idx..new_len);

        if was_empty {
            self.pos = Some(0);
        }
    }

    /// Moves the track at `order` index `from` to index `to` (Stage 3 Task 6:
    /// drag-reorder within the Queue view). Like `library::playlists::
    /// move_position`, but operating on the in-memory `order` vec instead of
    /// SQL rows — `ui::track_list_dnd`'s queue-reorder drop handler is the
    /// caller, via `ui::player_controller::PlayerController::move_queue_item`.
    ///
    /// The *currently playing* track must stay current after the move (same
    /// contract as `set_shuffle`'s current-track preservation): this looks up
    /// the track by its stable identity (the `ids`-index stored at `order[pos]`),
    /// not by numeric index, since the move can shift which `order` slot the
    /// current track sits in — including when the move crosses over it (e.g.
    /// moving an earlier track to a later index shifts the current track's own
    /// index down by one).
    ///
    /// Bounds: an empty queue, or `from`/`to` out of range (`>= order.len()`),
    /// is a logged no-op rather than a panic — matching every other
    /// out-of-range guard on this type (`set_shuffle`'s pos guard, etc.).
    /// `from == to` is also a no-op (nothing to move). Returns whether a move
    /// actually happened — `false` for any of the no-op cases above, `true`
    /// otherwise — so `ui::player_controller::PlayerController::move_queue_
    /// item` (and, through it, `ui::track_list_dnd`'s queue-reorder drop
    /// handler) can report a degraded/no-op outcome as failure rather than
    /// success (Stage 3 Task 6 review finding #3).
    pub fn move_item(&mut self, from: usize, to: usize) -> bool {
        let len = self.order.len();
        if len == 0 {
            warn!("move_item: queue is empty; no-op");
            return false;
        }
        if from >= len || to >= len {
            warn!(from, to, len, "move_item: index out of range; no-op");
            return false;
        }
        if from == to {
            return false;
        }

        // Remember the *track*'s identity (its `ids`-index), not its current
        // numeric position in `order` — that position is exactly what's
        // about to shift.
        let current_track_slot = self.pos.map(|idx| self.order[idx]);

        let entry = self.order.remove(from);
        self.order.insert(to, entry);

        if let Some(track_slot) = current_track_slot {
            self.pos = self.order.iter().position(|&idx| idx == track_slot);
        }
        true
    }

    /// Purges every occurrence of each id in `remove` from the queue (Stage-3
    /// close-out: "Remove from library" hard-deletes `tracks` rows —
    /// `queries::remove_missing_track`/`remove_missing_tracks` — and a
    /// queued id that no longer resolves to a row desyncs `len()`/`ids_in_
    /// order()` from what `ViewSource::Queue`'s window query can actually
    /// render; see `queries.rs`'s module doc, `Queue` section). Every
    /// occurrence of a removed id is dropped, not just one — a hard-deleted
    /// track is gone from the database entirely, so any queue slot
    /// referencing it (even a duplicate, e.g. the same track queued twice)
    /// is equally stale.
    ///
    /// The *currently playing* track (if it survives) stays current, by the
    /// same stable-identity technique `move_item` uses (looked up by its
    /// slot in `ids`, not by numeric position, since removal shifts every
    /// later index down). If the current track ITSELF is being removed, this
    /// advances to the next surviving track in play order — never wraps
    /// backward — or becomes `None` if no track survives after it. A queue
    /// that was already exhausted (`pos == None`, e.g. `Repeat::Off` ran off
    /// the end) stays exhausted; a removal never resurrects a position,
    /// matching `append_tracks`' same contract after exhaustion.
    ///
    /// A no-op (returns `false`, no mutation) for an empty `remove` slice, an
    /// empty queue, or a `remove` slice that matches nothing currently
    /// queued. Returns whether anything was actually removed.
    pub fn remove_ids(&mut self, remove: &[i64]) -> bool {
        if remove.is_empty() || self.ids.is_empty() {
            return false;
        }
        let remove_set: std::collections::HashSet<i64> = remove.iter().copied().collect();
        if !self.ids.iter().any(|id| remove_set.contains(id)) {
            return false;
        }

        // The order-slot (position in the play sequence) the current track
        // occupies — `None` if the queue is already exhausted.
        let old_pos_slot = self.pos;

        // Build the surviving `ids` list and a map from every OLD `ids`-index
        // to its new one (`None` for a removed index).
        let mut new_ids = Vec::with_capacity(self.ids.len());
        let mut index_map: Vec<Option<usize>> = Vec::with_capacity(self.ids.len());
        for &id in &self.ids {
            if remove_set.contains(&id) {
                index_map.push(None);
            } else {
                index_map.push(Some(new_ids.len()));
                new_ids.push(id);
            }
        }

        let new_order: Vec<usize> = self
            .order
            .iter()
            .filter_map(|&old_idx| index_map[old_idx])
            .collect();

        let new_pos = match old_pos_slot {
            None => None, // already exhausted; stays exhausted
            Some(slot) => {
                let old_idx = self.order[slot];
                match index_map[old_idx] {
                    // Current track survived: find its (unique) slot in the
                    // new order.
                    Some(new_idx) => new_order.iter().position(|&idx| idx == new_idx),
                    // Current track itself was removed: advance to the next
                    // surviving track in play order (never wrap backward to
                    // an earlier one) — `None` if nothing survives after it.
                    None => self.order[slot + 1..]
                        .iter()
                        .find_map(|&old_idx| index_map[old_idx])
                        .and_then(|new_idx| new_order.iter().position(|&idx| idx == new_idx)),
                }
            }
        };

        self.ids = new_ids;
        self.order = new_order;
        self.pos = new_pos;
        true
    }

    /// Every queued track id in current play order — reflecting shuffle, if
    /// active — used by `ViewSource::Queue` (Stage 3 Task 3): `ui::
    /// player_controller::queue_ids_snapshot` clones this out so the "Queue"
    /// track-list view always shows the same order the queue will actually
    /// play next, not just the ids as they were originally loaded by `set_
    /// tracks`. `.get()` rather than direct indexing: `order`'s entries are
    /// always valid indices into `ids` under this struct's own invariant,
    /// but a defensive skip costs nothing and avoids ever panicking here.
    pub fn ids_in_order(&self) -> Vec<i64> {
        self.order
            .iter()
            .filter_map(|&idx| self.ids.get(idx).copied())
            .collect()
    }
}

impl Default for Queue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // Test 1: Empty queue
    #[test]
    fn test_empty_queue() {
        let q = Queue::new();
        assert!(q.is_empty());
        assert_eq!(q.len(), 0);
        assert_eq!(q.current(), None);
    }

    #[test]
    fn test_empty_queue_operations() {
        let mut q = Queue::new();
        assert_eq!(q.advance_auto(), None);
        assert_eq!(q.next_manual(), None);
        assert_eq!(q.previous(), None);
    }

    // Test 2: set_tracks with valid start_index
    #[test]
    fn test_set_tracks_middle() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 1);
        assert_eq!(q.current(), Some(20));
        assert_eq!(q.len(), 3);
    }

    // Test 3: Linear advance_auto with Repeat::Off
    #[test]
    fn test_linear_advance_repeat_off() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 0);
        q.set_repeat(Repeat::Off);

        assert_eq!(q.current(), Some(10));
        assert_eq!(q.advance_auto(), Some(20));
        assert_eq!(q.current(), Some(20));
        assert_eq!(q.advance_auto(), Some(30));
        assert_eq!(q.current(), Some(30));
        assert_eq!(q.advance_auto(), None);
        assert_eq!(q.current(), None);
    }

    // Test 4: Repeat::All wrapping
    #[test]
    fn test_repeat_all_wrap() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 0);
        q.set_repeat(Repeat::All);

        assert_eq!(q.advance_auto(), Some(20));
        assert_eq!(q.advance_auto(), Some(30));
        assert_eq!(q.advance_auto(), Some(10)); // Wraps to first
        assert_eq!(q.advance_auto(), Some(20));
    }

    // Test 5: Repeat::One behavior
    #[test]
    fn test_repeat_one_advance_auto() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 1);
        q.set_repeat(Repeat::One);

        assert_eq!(q.current(), Some(20));
        assert_eq!(q.advance_auto(), Some(20)); // Same track
        assert_eq!(q.advance_auto(), Some(20)); // Still same
    }

    #[test]
    fn test_repeat_one_next_manual() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 1);
        q.set_repeat(Repeat::One);

        assert_eq!(q.current(), Some(20));
        assert_eq!(q.next_manual(), Some(30)); // Move past One
        assert_eq!(q.current(), Some(30));
        assert_eq!(q.next_manual(), None);
    }

    // Test 6: previous() behavior
    #[test]
    fn test_previous_at_first() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 0);

        assert_eq!(q.previous(), Some(10)); // Stay at first
        assert_eq!(q.current(), Some(10));
    }

    #[test]
    fn test_previous_mid_queue() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 2);

        assert_eq!(q.current(), Some(30));
        assert_eq!(q.previous(), Some(20));
        assert_eq!(q.current(), Some(20));
        assert_eq!(q.previous(), Some(10));
        assert_eq!(q.current(), Some(10));
        assert_eq!(q.previous(), Some(10));
    }

    // Test 7: Shuffle with current track staying current
    #[test]
    fn test_shuffle_current_stays() {
        fastrand::seed(42);
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 1);

        assert_eq!(q.current(), Some(20));
        q.set_shuffle(true);
        assert_eq!(q.current(), Some(20)); // Current track unchanged
        assert!(q.is_shuffled());
    }

    #[test]
    fn test_shuffle_visits_all_tracks() {
        fastrand::seed(42);
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 0);
        q.set_repeat(Repeat::All);

        q.set_shuffle(true);
        assert!(q.is_shuffled());

        // Collect all tracks via a full pass.
        let mut visited = HashSet::new();
        if let Some(track) = q.current() {
            visited.insert(track);
        }

        for _ in 0..10 {
            if let Some(track) = q.advance_auto() {
                visited.insert(track);
            }
        }

        // All 3 tracks should be visited (and repeating).
        assert_eq!(visited.len(), 3);
        assert!(visited.contains(&10));
        assert!(visited.contains(&20));
        assert!(visited.contains(&30));
    }

    #[test]
    fn test_shuffle_restore_linear_keeps_current() {
        fastrand::seed(42);
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 1);

        assert_eq!(q.current(), Some(20));
        q.set_shuffle(true);
        assert!(q.is_shuffled());
        assert_eq!(q.current(), Some(20));

        q.set_shuffle(false);
        assert!(!q.is_shuffled());
        assert_eq!(q.current(), Some(20)); // Should still be 20
        assert_eq!(q.pos, Some(1)); // Should be at linear index 1
    }

    /// Stage 3 Task 1 backlog fix: `set_shuffle`'s defensive `pos`-out-of-
    /// bounds guard must bail out BEFORE flipping `shuffled`, not after —
    /// otherwise an (unreachable under the struct's own invariant, but still
    /// worth pinning) early return would leave `shuffled` desynced from the
    /// order actually still being linear. Forces the invariant-violating
    /// state directly (same-module test, so private fields are reachable)
    /// rather than trying to construct it through the public API, which
    /// can't produce it at all.
    #[test]
    fn set_shuffle_guard_failure_does_not_desync_shuffled_flag() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 0);
        q.pos = Some(99); // out of bounds for `order` (len 3)

        q.set_shuffle(true);

        assert!(
            !q.is_shuffled(),
            "guard failure must leave `shuffled` false, matching the order \
             actually still being unshuffled"
        );
    }

    // Test 8: next_manual at end
    #[test]
    fn test_next_manual_end_repeat_off() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 2);
        q.set_repeat(Repeat::Off);

        assert_eq!(q.current(), Some(30));
        assert_eq!(q.next_manual(), None);
        assert_eq!(q.current(), None);
    }

    #[test]
    fn test_next_manual_end_repeat_all() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 2);
        q.set_repeat(Repeat::All);

        assert_eq!(q.current(), Some(30));
        assert_eq!(q.next_manual(), Some(10)); // Wraps
        assert_eq!(q.current(), Some(10));
    }

    // Test 9: set_tracks with out-of-range start_index
    #[test]
    fn test_set_tracks_out_of_range() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 100);
        assert_eq!(q.current(), Some(30)); // Clamped to last
    }

    #[test]
    fn test_set_tracks_empty() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20], 0);
        assert_eq!(q.len(), 2);

        q.set_tracks(vec![], 0);
        assert!(q.is_empty());
        assert_eq!(q.current(), None);
    }

    // Additional comprehensive tests
    #[test]
    fn test_repeat_modes() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 0);

        assert_eq!(q.repeat(), Repeat::Off);
        q.set_repeat(Repeat::All);
        assert_eq!(q.repeat(), Repeat::All);
        q.set_repeat(Repeat::One);
        assert_eq!(q.repeat(), Repeat::One);
    }

    #[test]
    fn test_multiple_shuffles() {
        fastrand::seed(42);
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 0);

        q.set_shuffle(true);
        assert!(q.is_shuffled());
        q.set_shuffle(false);
        assert!(!q.is_shuffled());
        q.set_shuffle(true);
        assert!(q.is_shuffled());
    }

    #[test]
    fn test_default_repeat_is_off() {
        let q = Queue::new();
        assert_eq!(q.repeat(), Repeat::Off);
    }

    // Stage 3 Task 5: `append_tracks` ("Add to queue" context-menu action).

    #[test]
    fn append_tracks_to_empty_queue_populates_it_at_index_zero() {
        let mut q = Queue::new();
        q.append_tracks(&[10, 20]);
        assert_eq!(q.len(), 2);
        assert_eq!(q.current(), Some(10));
        assert_eq!(q.ids_in_order(), vec![10, 20]);
    }

    #[test]
    fn append_tracks_appends_to_end_of_linear_order_without_moving_current() {
        let mut q = Queue::new();
        q.set_tracks(vec![1, 2, 3], 1);
        assert_eq!(q.current(), Some(2));

        q.append_tracks(&[4, 5]);

        assert_eq!(q.current(), Some(2), "current track must be unaffected");
        assert_eq!(q.ids_in_order(), vec![1, 2, 3, 4, 5]);
        assert_eq!(q.len(), 5);
    }

    #[test]
    fn append_tracks_appends_to_tail_of_shuffled_order() {
        fastrand::seed(7);
        let mut q = Queue::new();
        q.set_tracks(vec![1, 2, 3], 0);
        q.set_shuffle(true);
        let before = q.ids_in_order();
        let current_before = q.current();

        q.append_tracks(&[100, 200]);

        let after = q.ids_in_order();
        assert_eq!(after.len(), 5);
        // The first 3 entries (the pre-existing shuffled order) are
        // untouched; the two new ids land at the tail, in append order —
        // not woven into the shuffled prefix.
        assert_eq!(&after[..3], &before[..]);
        assert_eq!(&after[3..], &[100, 200]);
        assert_eq!(
            q.current(),
            current_before,
            "current track must be unaffected"
        );
        assert!(q.is_shuffled(), "shuffle state must be unaffected");
    }

    #[test]
    fn append_tracks_empty_slice_is_a_no_op() {
        let mut q = Queue::new();
        q.set_tracks(vec![1, 2], 1);
        assert_eq!(q.current(), Some(2));

        q.append_tracks(&[]);

        assert_eq!(q.len(), 2);
        assert_eq!(q.current(), Some(2));
    }

    #[test]
    fn append_tracks_on_empty_queue_with_empty_slice_stays_empty() {
        let mut q = Queue::new();
        q.append_tracks(&[]);
        assert!(q.is_empty());
        assert_eq!(q.current(), None);
    }

    #[test]
    fn append_tracks_after_exhaustion_does_not_resume_playback_state() {
        // pos == None can also happen mid-life (queue exhausted after
        // Repeat::Off ran out) — appending here must NOT resurrect `pos` to
        // point at the newly appended tail, since `ids` is non-empty
        // already; only the "was truly empty" case forces `pos = Some(0)`.
        let mut q = Queue::new();
        q.set_tracks(vec![1, 2], 0);
        q.set_repeat(Repeat::Off);
        q.advance_auto(); // -> 2
        q.advance_auto(); // -> None (exhausted)
        assert_eq!(q.current(), None);

        q.append_tracks(&[3, 4]);

        assert_eq!(
            q.current(),
            None,
            "an exhausted (but non-empty) queue's position must stay exhausted"
        );
        assert_eq!(q.ids_in_order(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn ids_in_order_matches_linear_order_when_unshuffled() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 0);
        assert_eq!(q.ids_in_order(), vec![10, 20, 30]);
    }

    #[test]
    fn ids_in_order_reflects_shuffle() {
        fastrand::seed(42);
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30, 40, 50], 0);
        q.set_shuffle(true);

        let shuffled = q.ids_in_order();
        assert_eq!(shuffled.len(), 5);
        let mut sorted = shuffled.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![10, 20, 30, 40, 50]);
    }

    #[test]
    fn ids_in_order_is_empty_for_an_empty_queue() {
        let q = Queue::new();
        assert!(q.ids_in_order().is_empty());
    }

    // Fix 1: Sticky shuffle across set_tracks
    #[test]
    fn test_shuffle_sticky_across_set_tracks() {
        fastrand::seed(42);
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 1);
        q.set_shuffle(true);
        assert!(q.is_shuffled());

        // set_tracks with shuffle still on should re-shuffle the new order
        // and keep the chosen start_index track as current
        q.set_tracks(vec![100, 200, 300, 400], 2);
        assert!(
            q.is_shuffled(),
            "shuffle should remain active after set_tracks"
        );
        assert_eq!(
            q.current(),
            Some(300),
            "should be at the chosen start_index track"
        );

        // Verify we can do a full pass through all 4 tracks
        q.set_repeat(Repeat::All);
        let mut visited = HashSet::new();
        if let Some(track) = q.current() {
            visited.insert(track);
        }
        // Collect exactly 4 advances (one full cycle of 4 tracks)
        for _ in 0..4 {
            if let Some(track) = q.advance_auto() {
                visited.insert(track);
            }
        }
        assert_eq!(
            visited.len(),
            4,
            "should visit all 4 tracks in exactly one pass"
        );
        assert!(visited.contains(&100));
        assert!(visited.contains(&200));
        assert!(visited.contains(&300));
        assert!(visited.contains(&400));
    }

    #[test]
    fn test_shuffle_sticky_then_disable() {
        fastrand::seed(42);
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 1);
        q.set_shuffle(true);
        assert!(q.is_shuffled());
        assert_eq!(q.current(), Some(20));

        q.set_shuffle(false);
        assert!(!q.is_shuffled());
        assert_eq!(
            q.current(),
            Some(20),
            "current track should be preserved after disabling shuffle"
        );
    }

    // Fix 2: previous() after exhaustion (pos == None)
    #[test]
    fn test_previous_after_exhaustion_non_empty() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 0);
        q.set_repeat(Repeat::Off);

        // Advance to the end and exhaust the queue
        q.advance_auto(); // -> 20
        q.advance_auto(); // -> 30
        q.advance_auto(); // -> None (exhausted)
        assert_eq!(q.current(), None);

        // previous() should resume at the LAST track of the current order
        let result = q.previous();
        assert_eq!(
            result,
            Some(30),
            "previous() after exhaustion should resume at the last track"
        );
        assert_eq!(q.current(), Some(30));
    }

    #[test]
    fn test_previous_after_exhaustion_empty_queue() {
        let mut q = Queue::new();
        q.set_tracks(vec![], 0);
        assert_eq!(q.current(), None);

        // previous() on empty queue should return None
        assert_eq!(q.previous(), None);
    }

    // Fix 5: Strengthen shuffle-visits-all test to check strict property
    // Stage 3 Task 6: `move_item` (queue drag-reorder).

    #[test]
    fn move_item_forward_keeps_current_track_current() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30, 40], 1); // current = 20
        assert_eq!(q.current(), Some(20));

        q.move_item(0, 2); // move 10 from index 0 to index 2
        assert_eq!(q.ids_in_order(), vec![20, 30, 10, 40]);
        assert_eq!(
            q.current(),
            Some(20),
            "current track must stay current even though its index shifted"
        );
    }

    #[test]
    fn move_item_backward_keeps_current_track_current() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30, 40], 3); // current = 40
        assert_eq!(q.current(), Some(40));

        q.move_item(3, 0); // move 40 from index 3 to index 0
        assert_eq!(q.ids_in_order(), vec![40, 10, 20, 30]);
        assert_eq!(q.current(), Some(40));
    }

    #[test]
    fn move_item_of_the_current_track_itself_keeps_it_current() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 1); // current = 20
        q.move_item(1, 2);
        assert_eq!(q.ids_in_order(), vec![10, 30, 20]);
        assert_eq!(
            q.current(),
            Some(20),
            "moving the current track itself must still leave it current"
        );
    }

    #[test]
    fn move_item_same_index_is_a_no_op() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 1);
        q.move_item(1, 1);
        assert_eq!(q.ids_in_order(), vec![10, 20, 30]);
        assert_eq!(q.current(), Some(20));
    }

    #[test]
    fn move_item_out_of_range_from_is_a_no_op() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 0);
        q.move_item(99, 1);
        assert_eq!(q.ids_in_order(), vec![10, 20, 30]);
    }

    #[test]
    fn move_item_out_of_range_to_is_a_no_op() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 0);
        q.move_item(0, 99);
        assert_eq!(q.ids_in_order(), vec![10, 20, 30]);
    }

    #[test]
    fn move_item_on_empty_queue_is_a_no_op() {
        let mut q = Queue::new();
        q.move_item(0, 1);
        assert!(q.is_empty());
        assert_eq!(q.current(), None);
    }

    #[test]
    fn move_item_preserves_current_when_queue_is_exhausted() {
        // pos == None (exhausted, not empty) must stay None after a move
        // that doesn't touch the current-track bookkeeping at all.
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 0);
        q.set_repeat(Repeat::Off);
        q.advance_auto(); // -> 20
        q.advance_auto(); // -> 30
        q.advance_auto(); // -> None (exhausted)
        assert_eq!(q.current(), None);

        q.move_item(0, 2);
        assert_eq!(q.ids_in_order(), vec![20, 30, 10]);
        assert_eq!(q.current(), None, "exhausted queue must stay exhausted");
    }

    #[test]
    fn move_item_reflects_in_shuffled_order_too() {
        fastrand::seed(42);
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30, 40, 50], 0);
        q.set_shuffle(true);
        let current_before = q.current();

        let before = q.ids_in_order();
        q.move_item(0, 4);
        let after = q.ids_in_order();

        assert_eq!(after.len(), 5);
        assert_eq!(after[4], before[0], "the moved track lands at index 4");
        assert_eq!(
            q.current(),
            current_before,
            "current track survives a move under shuffle too"
        );
    }

    #[test]
    fn test_shuffle_visits_all_exactly_once_per_pass() {
        fastrand::seed(42);
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30, 40, 50], 0);
        q.set_repeat(Repeat::All);
        q.set_shuffle(true);

        // Collect ids in a single pass: exactly n advances from current
        let n = 5;
        let mut ids_in_pass = Vec::new();

        if let Some(track) = q.current() {
            ids_in_pass.push(track);
        }

        // Advance exactly n-1 more times for a total of n tracks
        for _ in 0..(n - 1) {
            if let Some(track) = q.advance_auto() {
                ids_in_pass.push(track);
            }
        }

        // The pass should contain exactly n ids, each unique
        assert_eq!(
            ids_in_pass.len(),
            n,
            "should collect exactly {n} ids in one pass"
        );

        // Create the full expected multiset
        let expected: HashSet<i64> = vec![10, 20, 30, 40, 50].into_iter().collect();
        let collected: HashSet<i64> = ids_in_pass.into_iter().collect();
        assert_eq!(
            collected, expected,
            "should visit each track exactly once in a single pass"
        );
    }

    // Stage-3 close-out: `remove_ids` (hard-delete queue purge).

    #[test]
    fn remove_ids_on_empty_queue_is_a_no_op() {
        let mut q = Queue::new();
        assert!(!q.remove_ids(&[1, 2]));
        assert!(q.is_empty());
    }

    #[test]
    fn remove_ids_empty_slice_is_a_no_op() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 0);
        assert!(!q.remove_ids(&[]));
        assert_eq!(q.ids_in_order(), vec![10, 20, 30]);
    }

    #[test]
    fn remove_ids_matching_nothing_is_a_no_op() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 1);
        assert!(!q.remove_ids(&[999]));
        assert_eq!(q.ids_in_order(), vec![10, 20, 30]);
        assert_eq!(q.current(), Some(20));
    }

    #[test]
    fn remove_ids_removes_a_non_current_track_and_stays_gapless() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30, 40], 1); // current = 20
        assert!(q.remove_ids(&[30]));
        assert_eq!(q.ids_in_order(), vec![10, 20, 40]);
        assert_eq!(q.current(), Some(20), "untouched current track survives");
        assert_eq!(q.len(), 3);
    }

    #[test]
    fn remove_ids_removing_the_current_track_advances_to_the_next_surviving_track() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30, 40], 1); // current = 20
        assert!(q.remove_ids(&[20]));
        assert_eq!(q.ids_in_order(), vec![10, 30, 40]);
        assert_eq!(
            q.current(),
            Some(30),
            "advances to the next surviving track, never backward"
        );
    }

    #[test]
    fn remove_ids_removing_the_current_track_when_it_is_last_yields_none() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30], 2); // current = 30, last track
        assert!(q.remove_ids(&[30]));
        assert_eq!(q.ids_in_order(), vec![10, 20]);
        assert_eq!(
            q.current(),
            None,
            "no surviving track after the removed current track"
        );
    }

    #[test]
    fn remove_ids_removing_current_skips_over_multiple_removed_tracks_ahead() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30, 40, 50], 1); // current = 20
        assert!(q.remove_ids(&[20, 30, 40]));
        assert_eq!(q.ids_in_order(), vec![10, 50]);
        assert_eq!(q.current(), Some(50));
    }

    #[test]
    fn remove_ids_removing_every_track_empties_the_queue() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20], 0);
        assert!(q.remove_ids(&[10, 20]));
        assert!(q.is_empty());
        assert_eq!(q.current(), None);
    }

    #[test]
    fn remove_ids_removes_every_occurrence_of_a_duplicated_id() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 10, 30], 0); // id 10 queued twice
        assert!(q.remove_ids(&[10]));
        assert_eq!(
            q.ids_in_order(),
            vec![20, 30],
            "every occurrence of a hard-deleted id must be purged, not just one"
        );
    }

    #[test]
    fn remove_ids_leaves_an_exhausted_queue_exhausted() {
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20], 0);
        q.set_repeat(Repeat::Off);
        q.advance_auto(); // -> 20
        q.advance_auto(); // -> None (exhausted)
        assert_eq!(q.current(), None);

        assert!(q.remove_ids(&[10]));
        assert_eq!(
            q.current(),
            None,
            "a removal must never resurrect an exhausted queue's position"
        );
        assert_eq!(q.ids_in_order(), vec![20]);
    }

    #[test]
    fn remove_ids_preserves_current_under_shuffle() {
        fastrand::seed(42);
        let mut q = Queue::new();
        q.set_tracks(vec![10, 20, 30, 40, 50], 0);
        q.set_shuffle(true);
        let current_before = q.current().unwrap();
        // Remove a track that is not the current one.
        let victim = q
            .ids_in_order()
            .into_iter()
            .find(|&id| id != current_before)
            .unwrap();

        assert!(q.remove_ids(&[victim]));
        assert_eq!(q.current(), Some(current_before));
        assert_eq!(q.len(), 4);
        assert!(!q.ids_in_order().contains(&victim));
    }
}
