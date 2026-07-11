/// Queue engine: a pure Rust module (no GTK, no DB) for track queuing, playback order,
/// shuffle, and repeat modes. Uses Fisher-Yates shuffle via fastrand for determinism.
///
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
    pub fn previous(&mut self) -> Option<i64> {
        match self.pos {
            None => None,
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
            self.shuffled = true;
            if let Some(current_pos) = self.pos {
                let current_id_idx = self.order[current_pos];

                // Fisher-Yates shuffle: permute indices, but skip current track.
                let n = self.order.len();
                for i in (1..n).rev() {
                    let j = fastrand::usize(0..=i);
                    self.order.swap(i, j);
                }

                // Move current track back to its position.
                if let Some(pos) = self.order.iter().position(|&idx| idx == current_id_idx) {
                    self.order.swap(current_pos, pos);
                }
            } else {
                // No current track; just shuffle normally.
                let n = self.order.len();
                for i in (1..n).rev() {
                    let j = fastrand::usize(0..=i);
                    self.order.swap(i, j);
                }
            }
        } else if !on && self.shuffled {
            // Currently shuffled; restore linear order.
            self.shuffled = false;
            let current_id_idx = self.pos.and_then(|idx| self.order.get(idx).copied());

            // Restore linear order.
            self.order = (0..self.ids.len()).collect();

            // Update position to follow the current track's linear index.
            if let Some(id_idx) = current_id_idx {
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
}
