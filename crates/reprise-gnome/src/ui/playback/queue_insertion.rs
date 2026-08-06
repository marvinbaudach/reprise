//! Manual queue insertion routes and their accepted-item feedback.

use crate::ui::player_controller::PlayerController;
use reprise_core::up_next::QueueItem;

impl PlayerController {
    /// Appends explicit user selections to visible Up Next without replacing
    /// or starting the hidden playback context. Duplicates remain meaningful
    /// user choices; an empty slice is a no-op.
    pub(in crate::ui) fn append_to_queue(&self, ids: &[i64]) -> usize {
        self.append_queue_items(&track_items(ids))
    }

    pub(in crate::ui) fn append_queue_items(&self, items: &[QueueItem]) -> usize {
        if items.is_empty() {
            tracing::debug!("append to queue: nothing to add; ignoring");
            return 0;
        }
        let accepted = self.up_next.borrow_mut().append(items);
        if accepted == 0 {
            tracing::debug!("append to queue: no queue-compatible items; ignoring");
            return 0;
        }
        self.notify_queue_changed();
        let queue_len = self.up_next.borrow().len();
        self.sync_transport_enabled(true);
        tracing::info!(added = accepted, queue_len, "items added to queue");
        accepted
    }

    /// QUE-3's "Play next": the given ids jump the manual line (front of
    /// Play Next), unlike `append_to_queue`'s back-of-line append.
    pub(in crate::ui) fn play_next(&self, ids: &[i64]) -> usize {
        self.play_next_items(&track_items(ids))
    }

    pub(in crate::ui) fn play_next_items(&self, items: &[QueueItem]) -> usize {
        if items.is_empty() {
            tracing::debug!("play next: nothing to add; ignoring");
            return 0;
        }
        let accepted = self.up_next.borrow_mut().prepend(items);
        if accepted == 0 {
            tracing::debug!("play next: no queue-compatible items; ignoring");
            return 0;
        }
        self.notify_queue_changed();
        self.sync_transport_enabled(true);
        tracing::info!(added = accepted, "items queued to play next");
        accepted
    }
}

pub(super) fn track_items(ids: &[i64]) -> Vec<QueueItem> {
    ids.iter().copied().map(QueueItem::Track).collect()
}
