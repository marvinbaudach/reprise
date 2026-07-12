//! Validated serialization boundary for Queue's private invariant state.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{Queue, Repeat};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueSnapshot {
    pub ids: Vec<i64>,
    pub order: Vec<usize>,
    pub position: Option<usize>,
    pub repeat: Repeat,
    pub shuffled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QueueSnapshotError {
    #[error("queue snapshot ids and order lengths differ")]
    LengthMismatch,
    #[error("queue snapshot order is not a permutation")]
    InvalidOrder,
    #[error("queue snapshot position is out of range")]
    InvalidPosition,
}

impl Queue {
    #[must_use]
    pub fn snapshot(&self) -> QueueSnapshot {
        QueueSnapshot {
            ids: self.ids.clone(),
            order: self.order.clone(),
            position: self.pos,
            repeat: self.repeat,
            shuffled: self.shuffled,
        }
    }

    pub fn restore_snapshot(&mut self, snapshot: QueueSnapshot) -> Result<(), QueueSnapshotError> {
        let len = snapshot.ids.len();
        if snapshot.order.len() != len {
            return Err(QueueSnapshotError::LengthMismatch);
        }
        let mut seen = vec![false; len];
        for &index in &snapshot.order {
            let Some(slot) = seen.get_mut(index) else {
                return Err(QueueSnapshotError::InvalidOrder);
            };
            if *slot {
                return Err(QueueSnapshotError::InvalidOrder);
            }
            *slot = true;
        }
        if snapshot.position.is_some_and(|position| position >= len) {
            return Err(QueueSnapshotError::InvalidPosition);
        }

        self.ids = snapshot.ids;
        self.order = snapshot.order;
        self.pos = snapshot.position;
        self.repeat = snapshot.repeat;
        self.shuffled = snapshot.shuffled;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_shuffled_reordered_snapshot_round_trips() {
        let mut original = Queue::new();
        original.set_tracks(vec![10, 20, 30, 40], 2);
        original.set_shuffle(true);
        original.set_repeat(Repeat::All);
        assert!(original.move_item(0, 3));
        let snapshot = original.snapshot();

        let mut restored = Queue::new();
        restored.restore_snapshot(snapshot.clone()).unwrap();
        assert_eq!(restored.snapshot(), snapshot);
    }

    #[test]
    fn invalid_order_is_rejected_without_mutating_queue() {
        let mut queue = Queue::new();
        queue.set_tracks(vec![1, 2], 0);
        let before = queue.snapshot();
        let mut invalid = before.clone();
        invalid.order = vec![0, 0];

        assert_eq!(
            queue.restore_snapshot(invalid),
            Err(QueueSnapshotError::InvalidOrder)
        );
        assert_eq!(queue.snapshot(), before);
    }

    #[test]
    fn invalid_position_is_rejected_without_mutating_queue() {
        let mut queue = Queue::new();
        queue.set_tracks(vec![1, 2], 0);
        let before = queue.snapshot();
        let mut invalid = before.clone();
        invalid.position = Some(2);

        assert_eq!(
            queue.restore_snapshot(invalid),
            Err(QueueSnapshotError::InvalidPosition)
        );
        assert_eq!(queue.snapshot(), before);
    }
}
