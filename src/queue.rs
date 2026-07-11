/// Queue engine: a pure Rust module (no GTK, no DB) for track queuing, playback order,
/// shuffle, and repeat modes. Uses Fisher-Yates shuffle via fastrand for determinism.
use tracing::warn;

/// Repeat mode for the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
}
