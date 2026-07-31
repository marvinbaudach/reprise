//! Ordered pending queue items explicitly added by the user.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum QueueItem {
    Track(i64),
    Episode(i64),
}

impl QueueItem {
    pub fn id(self) -> i64 {
        match self {
            Self::Track(id) | Self::Episode(id) => id,
        }
    }

    pub fn track_id(self) -> Option<i64> {
        match self {
            Self::Track(id) => Some(id),
            Self::Episode(_) => None,
        }
    }

    pub fn episode_id(self) -> Option<i64> {
        match self {
            Self::Episode(id) => Some(id),
            Self::Track(_) => None,
        }
    }
}

impl From<i64> for QueueItem {
    fn from(id: i64) -> Self {
        Self::Track(id)
    }
}

impl PartialEq<i64> for QueueItem {
    fn eq(&self, other: &i64) -> bool {
        *self == Self::Track(*other)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UpNextQueue {
    ids: Vec<QueueItem>,
}

impl UpNextQueue {
    pub fn append(&mut self, items: &[QueueItem]) {
        let limit = usize::try_from(crate::queries::QUEUE_LIMIT).unwrap_or(usize::MAX);
        let remaining = limit.saturating_sub(self.ids.len());
        self.ids.extend(items.iter().copied().take(remaining));
    }

    pub fn ids(&self) -> &[QueueItem] {
        &self.ids
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn pop_front(&mut self) -> Option<QueueItem> {
        self.take_first_matching(|_| true)
    }

    /// Removes and returns the first available item while retaining rejected
    /// entries at their exact positions. This lets unavailable manual queue
    /// entries heal in place instead of an unmount destroying user intent.
    pub fn take_first_matching(
        &mut self,
        mut is_available: impl FnMut(QueueItem) -> bool,
    ) -> Option<QueueItem> {
        let position = self.ids.iter().position(|&item| is_available(item))?;
        Some(self.ids.remove(position))
    }

    pub fn first_matching(
        &self,
        mut is_available: impl FnMut(QueueItem) -> bool,
    ) -> Option<QueueItem> {
        self.ids.iter().copied().find(|&item| is_available(item))
    }

    /// Removes and returns exactly the selected pending entry.
    pub fn take_at(&mut self, position: usize) -> Option<QueueItem> {
        (position < self.ids.len()).then(|| self.ids.remove(position))
    }

    pub fn move_item(&mut self, from: usize, to: usize) -> bool {
        if from == to || from >= self.ids.len() || to >= self.ids.len() {
            return false;
        }
        let item = self.ids.remove(from);
        self.ids.insert(to, item);
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

    /// Returns how many entries went, not merely whether any did: the same
    /// track may sit in the queue more than once, and a caller reporting the
    /// removal to a user needs the number it can show.
    pub fn remove_ids(&mut self, items: &[QueueItem]) -> usize {
        let items: HashSet<QueueItem> = items.iter().copied().collect();
        let before = self.ids.len();
        self.ids.retain(|item| !items.contains(item));
        before - self.ids.len()
    }

    pub fn truncate(&mut self, limit: usize) {
        self.ids.truncate(limit);
    }

    /// Inserts one item at `index` (clamped to the end) — QUE-3's drag of an
    /// Up-Next snapshot row into the Play Next section lands here. Respects
    /// the same capacity limit as `append`.
    pub fn insert(&mut self, index: usize, item: QueueItem) {
        let limit = usize::try_from(crate::queries::QUEUE_LIMIT).unwrap_or(usize::MAX);
        if self.ids.len() >= limit {
            return;
        }
        let index = index.min(self.ids.len());
        self.ids.insert(index, item);
    }

    /// Puts `items` at the FRONT in the given order — the "Play next" context
    /// action (QUE-3), as opposed to `append`'s "Add to queue". Capacity
    /// overflow drops from the back of the existing list, never the new
    /// front entries (the user's freshest intent wins).
    pub fn prepend(&mut self, items: &[QueueItem]) {
        let limit = usize::try_from(crate::queries::QUEUE_LIMIT).unwrap_or(usize::MAX);
        let mut merged = Vec::with_capacity((items.len() + self.ids.len()).min(limit));
        merged.extend(items.iter().copied().take(limit));
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
    use super::{QueueItem, UpNextQueue};

    fn tracks(ids: &[i64]) -> Vec<QueueItem> {
        ids.iter().copied().map(QueueItem::Track).collect()
    }

    #[test]
    fn que_9_manual_queue_preserves_track_and_episode_identity() {
        let mut queue = UpNextQueue::default();
        queue.append(&[
            QueueItem::Track(7),
            QueueItem::Episode(7),
            QueueItem::Track(7),
        ]);

        assert_eq!(
            queue.ids(),
            &[
                QueueItem::Track(7),
                QueueItem::Episode(7),
                QueueItem::Track(7),
            ]
        );
        assert_eq!(queue.pop_front(), Some(QueueItem::Track(7)));
        assert_eq!(queue.pop_front(), Some(QueueItem::Episode(7)));
    }

    #[test]
    fn append_preserves_order_and_duplicates() {
        let mut queue = UpNextQueue::default();
        queue.append(&tracks(&[3, 7, 3]));
        queue.append(&tracks(&[9]));
        assert_eq!(queue.ids(), &[3, 7, 3, 9]);
        assert_eq!(queue.len(), 4);
        assert!(!queue.is_empty());
    }

    #[test]
    fn pop_and_take_at_consume_only_the_selected_entries() {
        let mut queue = UpNextQueue::default();
        queue.append(&tracks(&[10, 20, 30, 40]));
        assert_eq!(queue.pop_front(), Some(QueueItem::Track(10)));
        assert_eq!(queue.take_at(1), Some(QueueItem::Track(30)));
        assert_eq!(queue.ids(), &[20, 40]);
        assert_eq!(queue.take_at(2), None);
        assert_eq!(queue.ids(), &[20, 40]);
    }

    #[test]
    fn que_5_jump_keeps_preceding_manual_entries() {
        let mut queue = UpNextQueue::default();
        queue.append(&tracks(&[10, 20, 30, 40]));

        assert_eq!(queue.take_at(3), Some(QueueItem::Track(40)));
        assert_eq!(queue.ids(), &[10, 20, 30]);
    }

    #[test]
    fn que_3_played_manual_entries_removed() {
        let mut queue = UpNextQueue::default();
        queue.append(&tracks(&[10, 20, 30]));

        assert_eq!(queue.pop_front(), Some(QueueItem::Track(10)));
        assert_eq!(queue.ids(), &[20, 30]);
    }

    #[test]
    fn move_item_reorders_only_valid_distinct_positions() {
        let mut queue = UpNextQueue::default();
        queue.append(&tracks(&[1, 2, 3, 4]));
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
        queue.append(&tracks(&[10, 20, 30, 40, 50]));
        assert_eq!(queue.remove_positions(&[3, 1, 3, 99]), 2);
        assert_eq!(queue.ids(), &[10, 30, 50]);
        assert_eq!(queue.remove_positions(&[]), 0);
    }

    #[test]
    fn remove_ids_removes_all_occurrences() {
        let mut queue = UpNextQueue::default();
        queue.append(&tracks(&[1, 2, 1, 3, 2]));
        assert_eq!(
            queue.remove_ids(&tracks(&[1, 9])),
            2,
            "both occurrences of 1 went; 9 was never there"
        );
        assert_eq!(queue.ids(), &[2, 3, 2]);
        assert_eq!(queue.remove_ids(&tracks(&[8, 9])), 0);
    }

    #[test]
    fn truncate_keeps_the_pending_prefix() {
        let mut queue = UpNextQueue::default();
        queue.append(&tracks(&[1, 2, 3, 4]));
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
        queue.append(&tracks(&ids));
        assert_eq!(queue.len(), limit);
        assert_eq!(
            queue.ids().last(),
            Some(&QueueItem::Track(limit as i64 - 1))
        );
    }
}

#[cfg(test)]
mod que3_tests {
    use super::*;

    fn tracks(ids: &[i64]) -> Vec<QueueItem> {
        ids.iter().copied().map(QueueItem::Track).collect()
    }

    #[test]
    fn insert_places_at_index_and_clamps_past_the_end() {
        let mut queue = UpNextQueue::default();
        queue.append(&tracks(&[1, 3]));
        queue.insert(1, QueueItem::Track(2));
        assert_eq!(queue.ids(), &[1, 2, 3]);
        queue.insert(99, QueueItem::Track(4));
        assert_eq!(queue.ids(), &[1, 2, 3, 4]);
    }

    #[test]
    fn prepend_puts_ids_at_the_front_in_given_order() {
        let mut queue = UpNextQueue::default();
        queue.append(&tracks(&[9]));
        queue.prepend(&tracks(&[1, 2]));
        assert_eq!(queue.ids(), &[1, 2, 9]);
    }

    #[test]
    fn clear_empties_only_this_list() {
        let mut queue = UpNextQueue::default();
        queue.append(&tracks(&[1, 2, 3]));
        queue.clear();
        assert!(queue.is_empty());
    }
}
