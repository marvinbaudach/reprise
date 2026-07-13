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
        self.ids.extend_from_slice(ids);
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
        if self.ids.is_empty() {
            None
        } else {
            Some(self.ids.remove(0))
        }
    }

    pub fn take_through(&mut self, position: usize) -> Option<i64> {
        let selected = self.ids.get(position).copied()?;
        self.ids.drain(..=position);
        Some(selected)
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
    fn pop_and_take_through_consume_expected_prefixes() {
        let mut queue = UpNextQueue::default();
        queue.append(&[10, 20, 30, 40]);
        assert_eq!(queue.pop_front(), Some(10));
        assert_eq!(queue.take_through(1), Some(30));
        assert_eq!(queue.ids(), &[40]);
        assert_eq!(queue.take_through(1), None);
        assert_eq!(queue.ids(), &[40]);
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
}
