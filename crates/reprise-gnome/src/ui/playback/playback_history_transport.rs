//! PLAY-14 playback-history wiring for `PlayerController`.
//!
//! The pure state and decision live in `reprise_core::playback_history`. This
//! module owns only the frontend seam: where playback is recorded, how a
//! history navigation differs from an ordinary transition, and how a target
//! is presented.
//!
//! There are two recording funnels and one decision. `present_track` records
//! tracks, while `begin_podcast` records episodes because that path does not
//! pass through `present_track`; both call `note_playback_started`.
//!
//! The same borrow discipline as `player_controller.rs` applies: no history
//! or queue borrow survives a `present_*` call.

use reprise_core::playback_history::{HistoryEntry, PlaybackHistory};
use reprise_core::queue::Queue;
use reprise_core::up_next::QueueItem;

use crate::ui::player_controller::PlayerController;

#[derive(Debug, Default)]
pub(in crate::ui) struct HistoryState {
    pub(in crate::ui) history: PlaybackHistory,
    /// One-shot flag consumed by the recorder after a history navigation.
    pub(in crate::ui) navigating: bool,
}

pub(in crate::ui) fn note(state: &mut HistoryState, entry: HistoryEntry) {
    if std::mem::take(&mut state.navigating) {
        return;
    }
    state.history.record(entry);
}

pub(in crate::ui) fn entry_for(
    context: &Queue,
    item: QueueItem,
    from_up_next: bool,
) -> HistoryEntry {
    HistoryEntry {
        item,
        context_pos: (!from_up_next)
            .then(|| context.current_order_position())
            .flatten(),
        sequence: context.sequence_identity(),
        from_up_next,
    }
}

impl PlayerController {
    /// Records playback through the track and episode funnels without keeping
    /// either queue or history borrowed across a re-entrant controller call.
    pub(in crate::ui) fn note_playback_started(&self, item: QueueItem, from_up_next: bool) {
        let entry = {
            let context = self.queue.borrow();
            entry_for(&context, item, from_up_next)
        };
        let mut state = self.history.borrow_mut();
        note(&mut state, entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(ids: &[i64], start: usize) -> Queue {
        let mut queue = Queue::new();
        queue.set_tracks(ids.to_vec(), start);
        queue
    }

    #[test]
    fn play_14_a_history_jump_is_not_recorded_as_new_playback() {
        let queue = context(&[10, 20, 30], 0);
        let mut state = HistoryState::default();
        note(&mut state, entry_for(&queue, QueueItem::Track(10), false));
        note(&mut state, entry_for(&queue, QueueItem::Track(20), false));
        assert_eq!(state.history.back_len(), 1);

        state.navigating = true;
        note(&mut state, entry_for(&queue, QueueItem::Track(10), false));
        assert!(!state.navigating, "navigation is a one-shot flag");
        assert_eq!(
            state.history.back_len(),
            1,
            "a history jump must not record itself as a transition"
        );
    }

    #[test]
    fn play_14_an_entry_carries_the_playhead_and_the_context_generation() {
        let queue = context(&[10, 20, 30], 1);
        let entry = entry_for(&queue, QueueItem::Track(20), false);
        assert_eq!(entry.item, QueueItem::Track(20));
        assert_eq!(entry.context_pos, queue.current_order_position());
        assert_eq!(entry.playhead_in(queue.sequence_identity()), Some(1));
        assert!(!entry.from_up_next);

        let pending = entry_for(&queue, QueueItem::Track(99), true);
        assert!(pending.from_up_next);
        assert_eq!(pending.context_pos, None);
    }

    #[test]
    fn play_14_a_reseeded_context_invalidates_an_older_entry() {
        let mut queue = context(&[10, 20, 30], 1);
        let entry = entry_for(&queue, QueueItem::Track(20), false);
        queue.set_tracks(vec![40, 50], 0);
        assert_eq!(entry.playhead_in(queue.sequence_identity()), None);
    }

    #[test]
    fn play_14_a_context_switch_leaves_the_history_intact() {
        let mut queue = context(&[10, 20], 0);
        let mut state = HistoryState::default();
        note(&mut state, entry_for(&queue, QueueItem::Track(10), false));
        note(&mut state, entry_for(&queue, QueueItem::Track(20), false));
        queue.set_tracks(vec![70, 80], 0);
        note(&mut state, entry_for(&queue, QueueItem::Track(70), false));
        assert_eq!(state.history.back_len(), 2);
    }
}
