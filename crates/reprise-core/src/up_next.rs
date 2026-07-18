//! Ordered pending tracks explicitly added by the user.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UpNextQueue {
    ids: Vec<i64>,
}

impl UpNextQueue {
    pub fn append(&mut self, ids: &[i64]) {
        let limit = usize::try_from(crate::queries::QUEUE_LIMIT).unwrap_or(usize::MAX);
        let remaining = limit.saturating_sub(self.ids.len());
        self.ids.extend(ids.iter().copied().take(remaining));
    }

    pub fn ids(&self) -> &[i64] {
        &self.ids
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn pop_front(&mut self) -> Option<i64> {
        self.take_first_matching(|_| true)
    }

    /// Removes and returns the first available id while retaining rejected
    /// entries at their exact positions. This lets unavailable manual queue
    /// entries heal in place instead of an unmount destroying user intent.
    pub fn take_first_matching(
        &mut self,
        mut is_available: impl FnMut(i64) -> bool,
    ) -> Option<i64> {
        let position = self.ids.iter().position(|&id| is_available(id))?;
        Some(self.ids.remove(position))
    }

    pub fn first_matching(&self, mut is_available: impl FnMut(i64) -> bool) -> Option<i64> {
        self.ids.iter().copied().find(|&id| is_available(id))
    }

    /// Removes and returns exactly the selected pending entry.
    pub fn take_at(&mut self, position: usize) -> Option<i64> {
        (position < self.ids.len()).then(|| self.ids.remove(position))
    }

    pub fn move_item(&mut self, from: usize, to: usize) -> bool {
        if from == to || from >= self.ids.len() || to >= self.ids.len() {
            return false;
        }
        let id = self.ids.remove(from);
        self.ids.insert(to, id);
        true
    }

    pub fn remove_positions(&mut self, positions: &[usize]) -> usize {
        let positions: HashSet<usize> = positions.iter().copied().collect();
        let before = self.ids.len();
        let mut index = 0;
        self.ids.retain(|_| {
            let keep = !positions.contains(&index);
            index += 1;
            keep
        });
        before - self.ids.len()
    }

    pub fn remove_ids(&mut self, ids: &[i64]) -> bool {
        let ids: HashSet<i64> = ids.iter().copied().collect();
        let before = self.ids.len();
        self.ids.retain(|id| !ids.contains(id));
        before != self.ids.len()
    }

    pub fn truncate(&mut self, limit: usize) {
        self.ids.truncate(limit);
    }

    /// Inserts one id at `index` (clamped to the end) — QUE-3's drag of an
    /// Up-Next snapshot row into the Play Next section lands here. Respects
    /// the same capacity limit as `append`.
    pub fn insert(&mut self, index: usize, id: i64) {
        let limit = usize::try_from(crate::queries::QUEUE_LIMIT).unwrap_or(usize::MAX);
        if self.ids.len() >= limit {
            return;
        }
        let index = index.min(self.ids.len());
        self.ids.insert(index, id);
    }

    /// Puts `ids` at the FRONT in the given order — the "Play next" context
    /// action (QUE-3), as opposed to `append`'s "Add to queue". Capacity
    /// overflow drops from the back of the existing list, never the new
    /// front entries (the user's freshest intent wins).
    pub fn prepend(&mut self, ids: &[i64]) {
        let limit = usize::try_from(crate::queries::QUEUE_LIMIT).unwrap_or(usize::MAX);
        let mut merged = Vec::with_capacity((ids.len() + self.ids.len()).min(limit));
        merged.extend(ids.iter().copied().take(limit));
        let remaining = limit.saturating_sub(merged.len());
        merged.extend(self.ids.iter().copied().take(remaining));
        self.ids = merged;
    }

    /// Empties the manual list — the "Clear queue" button (QUE-3), which
    /// deliberately touches ONLY Play Next, never the playback snapshot.
    pub fn clear(&mut self) {
        self.ids.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::UpNextQueue;

    #[test]
    fn append_preserves_order_and_duplicates() {
        let mut queue = UpNextQueue::default();
        queue.append(&[3, 7, 3]);
        queue.append(&[9]);
        assert_eq!(queue.ids(), &[3, 7, 3, 9]);
        assert_eq!(queue.len(), 4);
        assert!(!queue.is_empty());
    }

    #[test]
    fn pop_and_take_at_consume_only_the_selected_entries() {
        let mut queue = UpNextQueue::default();
        queue.append(&[10, 20, 30, 40]);
        assert_eq!(queue.pop_front(), Some(10));
        assert_eq!(queue.take_at(1), Some(30));
        assert_eq!(queue.ids(), &[20, 40]);
        assert_eq!(queue.take_at(2), None);
        assert_eq!(queue.ids(), &[20, 40]);
    }

    #[test]
    fn que_5_jump_keeps_preceding_manual_entries() {
        let mut queue = UpNextQueue::default();
        queue.append(&[10, 20, 30, 40]);

        assert_eq!(queue.take_at(3), Some(40));
        assert_eq!(queue.ids(), &[10, 20, 30]);
    }

    #[test]
    fn move_item_reorders_only_valid_distinct_positions() {
        let mut queue = UpNextQueue::default();
        queue.append(&[1, 2, 3, 4]);
        assert!(queue.move_item(1, 3));
        assert_eq!(queue.ids(), &[1, 3, 4, 2]);
        assert!(!queue.move_item(3, 3));
        assert!(!queue.move_item(8, 0));
        assert!(!queue.move_item(0, 8));
        assert_eq!(queue.ids(), &[1, 3, 4, 2]);
    }

    #[test]
    fn remove_positions_is_stable_and_ignores_duplicates_and_bad_indices() {
        let mut queue = UpNextQueue::default();
        queue.append(&[10, 20, 30, 40, 50]);
        assert_eq!(queue.remove_positions(&[3, 1, 3, 99]), 2);
        assert_eq!(queue.ids(), &[10, 30, 50]);
        assert_eq!(queue.remove_positions(&[]), 0);
    }

    #[test]
    fn remove_ids_removes_all_occurrences() {
        let mut queue = UpNextQueue::default();
        queue.append(&[1, 2, 1, 3, 2]);
        assert!(queue.remove_ids(&[1, 9]));
        assert_eq!(queue.ids(), &[2, 3, 2]);
        assert!(!queue.remove_ids(&[8, 9]));
    }

    #[test]
    fn truncate_keeps_the_pending_prefix() {
        let mut queue = UpNextQueue::default();
        queue.append(&[1, 2, 3, 4]);
        queue.truncate(2);
        assert_eq!(queue.ids(), &[1, 2]);
        queue.truncate(8);
        assert_eq!(queue.ids(), &[1, 2]);
    }

    #[test]
    fn append_never_grows_past_the_shared_queue_limit() {
        let limit = usize::try_from(crate::queries::QUEUE_LIMIT).unwrap();
        let ids: Vec<_> = (0..=limit as i64).collect();
        let mut queue = UpNextQueue::default();
        queue.append(&ids);
        assert_eq!(queue.len(), limit);
        assert_eq!(queue.ids().last(), Some(&(limit as i64 - 1)));
    }
}

#[cfg(test)]
mod que3_tests {
    use super::*;

    #[test]
    fn insert_places_at_index_and_clamps_past_the_end() {
        let mut queue = UpNextQueue::default();
        queue.append(&[1, 3]);
        queue.insert(1, 2);
        assert_eq!(queue.ids(), &[1, 2, 3]);
        queue.insert(99, 4);
        assert_eq!(queue.ids(), &[1, 2, 3, 4]);
    }

    #[test]
    fn prepend_puts_ids_at_the_front_in_given_order() {
        let mut queue = UpNextQueue::default();
        queue.append(&[9]);
        queue.prepend(&[1, 2]);
        assert_eq!(queue.ids(), &[1, 2, 9]);
    }

    #[test]
    fn clear_empties_only_this_list() {
        let mut queue = UpNextQueue::default();
        queue.append(&[1, 2, 3]);
        queue.clear();
        assert!(queue.is_empty());
    }
}
