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

    /// Non-mutating preview of what `advance_auto` would return next, without
    /// moving the position. Used by the gapless pre-feed (`feed_next`) to learn
    /// the upcoming track ahead of time; the actual advance still runs through
    /// `advance_auto` on the real transition. Must mirror `advance_auto`'s
    /// branching exactly.
    pub fn peek_auto(&self) -> Option<i64> {
        let idx = self.pos?;
        if self.repeat == Repeat::One {
            return self.current();
        }
        let next_idx = idx + 1;
        if next_idx < self.order.len() {
            self.order
                .get(next_idx)
                .and_then(|&track_idx| self.ids.get(track_idx).copied())
        } else if self.repeat == Repeat::All {
            self.order
                .first()
                .and_then(|&track_idx| self.ids.get(track_idx).copied())
        } else {
            None
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
#[path = "queue_tests.rs"]
mod tests;
