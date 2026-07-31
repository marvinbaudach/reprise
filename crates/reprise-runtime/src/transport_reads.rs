//! Reading the queue: the snapshot, the identity change detection compares,
//! and one page of a section.
//!
//! Split out of `transport.rs` because that file reached the 800-line ceiling
//! the architecture lint enforces. These three belong together for a reason
//! that is easy to lose: the snapshot shows a window, the page reaches past
//! it, and the identity is what both are checked against. A change that moves
//! one without the other two is how a client ends up holding a position the
//! runtime no longer agrees about.

use reprise_core::up_next::QueueItem as CoreQueueItem;
use reprise_runtime_protocol::queue::{
    QueueItem as ProtocolQueueItem, QueueSection, QueueSnapshot,
};

use super::{QueueIdentity, Transport, QUEUE_WINDOW};

impl Transport {
    /// The queue facet, *unstamped*: the revision belongs to the runtime,
    /// which is the only place that can count observable changes to this
    /// facet without drifting from what a client actually saw. Leaving it at
    /// zero here also keeps the before/after comparison honest — a revision
    /// baked in on both sides would either always differ or never.
    pub(crate) fn queue_snapshot(&self) -> QueueSnapshot {
        QueueSnapshot {
            revision: 0,
            // What is *playing*, which is not always where the context
            // cursor stands: an explicitly queued track plays beside the
            // context without moving it.
            current_track_id: self.current.as_ref().and_then(|track| track.track_id),
            play_next_track_ids: self
                .up_next
                .ids()
                .iter()
                .filter_map(|item| item.track_id())
                .take(QUEUE_WINDOW)
                .collect(),
            play_next_items: Some(
                self.up_next
                    .ids()
                    .iter()
                    .copied()
                    .take(QUEUE_WINDOW)
                    .map(protocol_item)
                    .collect(),
            ),
            context_track_ids: self.queue.remaining_window(0, QUEUE_WINDOW),
            context_items: Some(
                self.queue
                    .remaining_window(0, QUEUE_WINDOW)
                    .into_iter()
                    .map(ProtocolQueueItem::track)
                    .collect(),
            ),
            play_next_total: self.up_next.len() as u64,
            context_total: self.queue.remaining_len() as u64,
        }
    }

    /// The whole queue's order, for deciding whether it changed.
    ///
    /// Not the snapshot: that shows a 200-entry window, so comparing it
    /// misses a reorder past the two-hundredth row entirely. That was
    /// harmless only while nothing could name those positions. A paged read
    /// hands them out, so the change has to be noticed here or a client
    /// holding position 4,000 acts on a row that has since moved.
    ///
    /// Derived rather than counted. A version number bumped by each mutating
    /// method is one more thing to keep in step with the mutations, and this
    /// file has already paid for that mistake twice.
    pub(crate) fn queue_identity(&self) -> QueueIdentity {
        QueueIdentity {
            current: self.current.as_ref().and_then(|loaded| loaded.track_id),
            play_next: self.up_next.ids().to_vec(),
            context: self.queue.remaining_after_current(),
        }
    }

    /// One page of a queue section, for a view whose viewport has moved
    /// beyond what the snapshot carries, and how long that section is.
    pub(crate) fn queue_page(
        &self,
        section: QueueSection,
        offset: usize,
        limit: usize,
    ) -> (Vec<i64>, Vec<ProtocolQueueItem>, usize) {
        match section {
            QueueSection::PlayNext => {
                let items: Vec<CoreQueueItem> = self
                    .up_next
                    .ids()
                    .iter()
                    .copied()
                    .skip(offset)
                    .take(limit)
                    .collect();
                let track_ids = items.iter().filter_map(|item| item.track_id()).collect();
                let typed_items = items.into_iter().map(protocol_item).collect();
                (track_ids, typed_items, self.up_next.len())
            }
            QueueSection::Context => {
                let track_ids = self.queue.remaining_window(offset, limit);
                let items = track_ids
                    .iter()
                    .copied()
                    .map(ProtocolQueueItem::track)
                    .collect();
                (track_ids, items, self.queue.remaining_len())
            }
        }
    }
}

fn protocol_item(item: CoreQueueItem) -> ProtocolQueueItem {
    match item {
        CoreQueueItem::Track(id) => ProtocolQueueItem::track(id),
        CoreQueueItem::Episode(id) => ProtocolQueueItem::episode(id),
    }
}
