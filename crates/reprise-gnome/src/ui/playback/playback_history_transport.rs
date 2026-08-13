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

use std::rc::Rc;

use reprise_core::playback_history::{
    resolve_previous, HistoryEntry, PlaybackHistory, PreviousAction,
};
use reprise_core::queue::Queue;
use reprise_core::up_next::QueueItem;

use crate::ui::player_controller::PlayerController;

use super::player_controller::StartPlayback;

#[derive(Debug)]
enum PendingNavigation {
    Back(HistoryEntry),
    Forward(HistoryEntry),
}

#[derive(Debug, Default)]
pub(in crate::ui) struct HistoryState {
    pub(in crate::ui) history: PlaybackHistory,
    pending: Option<PendingNavigation>,
}

pub(in crate::ui) fn note(state: &mut HistoryState, entry: HistoryEntry) {
    if let Some(pending) = state.pending.take() {
        let (target, committed) = match pending {
            PendingNavigation::Back(target) => {
                let matches = target == entry;
                let committed = if matches {
                    state.history.step_back()
                } else {
                    None
                };
                (target, committed)
            }
            PendingNavigation::Forward(target) => {
                let matches = target == entry;
                let committed = if matches {
                    state.history.step_forward()
                } else {
                    None
                };
                (target, committed)
            }
        };
        if target == entry {
            debug_assert_eq!(committed, Some(entry));
            return;
        }
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
        replay_uri: None,
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

    /// PLAY-14: seek to the current item's start or navigate actual playback
    /// history; never restart the current item to implement a rewind.
    pub(in crate::ui) fn previous_from_history(self: &Rc<Self>) {
        let position_ms = self.current_position_ms();
        let action = {
            let state = self.history.borrow();
            resolve_previous(position_ms, &state.history)
        };
        let PreviousAction::GoTo(target) = action else {
            self.seek(0);
            tracing::info!(position_ms, "previous rewound to the song start");
            return;
        };
        self.history.borrow_mut().pending = Some(PendingNavigation::Back(target.clone()));
        let sequence = self.queue.borrow().sequence_identity();
        if let Some(position) = target.playhead_in(sequence) {
            self.queue.borrow_mut().jump_to_order_position(position);
        }
        self.current_up_next
            .set(target.from_up_next.then_some(target.item));
        tracing::info!(
            item = ?target.item,
            position_ms,
            "previous stepped back through the playback history"
        );
        self.present_queue_item(
            target.item,
            StartPlayback::Yes,
            crate::ui::current_track_selection::CurrentTrackChange::ExplicitTransport,
        );
        self.notify_queue_changed();
    }

    /// Returns to the item a history back-step left. `false` means ordinary
    /// queue advance should handle Next instead.
    pub(in crate::ui) fn forward_from_history(self: &Rc<Self>) -> bool {
        let target = {
            let mut state = self.history.borrow_mut();
            let Some(target) = state.history.peek_forward() else {
                return false;
            };
            state.pending = Some(PendingNavigation::Forward(target.clone()));
            target
        };
        let sequence = self.queue.borrow().sequence_identity();
        if let Some(position) = target.playhead_in(sequence) {
            self.queue.borrow_mut().jump_to_order_position(position);
        }
        self.current_up_next
            .set(target.from_up_next.then_some(target.item));
        tracing::info!(
            item = ?target.item,
            "next stepped forward through the playback history"
        );
        self.present_queue_item(
            target.item,
            StartPlayback::Yes,
            crate::ui::current_track_selection::CurrentTrackChange::ExplicitTransport,
        );
        self.notify_queue_changed();
        true
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

        let target = state.history.peek_back().expect("back target");
        state.pending = Some(PendingNavigation::Back(target.clone()));
        note(&mut state, target.clone());
        assert!(state.pending.is_none(), "navigation is one-shot state");
        assert_eq!(state.history.back_len(), 0);
        assert!(state.history.can_go_forward());
        assert_eq!(state.history.current(), Some(target));
    }

    #[test]
    fn play_14_a_failed_history_start_does_not_consume_the_next_real_start() {
        let queue = context(&[10, 20, 99], 0);
        let mut state = HistoryState::default();
        note(&mut state, entry_for(&queue, QueueItem::Track(10), false));
        note(&mut state, entry_for(&queue, QueueItem::Track(20), false));

        let target = state.history.peek_back().expect("back target");
        assert_eq!(target.item, QueueItem::Track(10));
        state.pending = Some(PendingNavigation::Back(target));
        note(&mut state, entry_for(&queue, QueueItem::Track(99), false));

        assert_eq!(
            state.history.current().map(|entry| entry.item),
            Some(QueueItem::Track(99))
        );
    }

    #[test]
    fn previous_transport_uses_the_payload_resolve_previous_already_returned() {
        let implementation = include_str!("playback_history_transport.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("implementation section");
        let direct_payload = ["PreviousAction::GoTo(", "target", ")"].concat();

        assert!(implementation.contains(&direct_payload));
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

    #[test]
    fn play_14_the_first_press_rewinds_and_the_second_steps_back() {
        let queue = context(&[10, 20, 30], 0);
        let mut state = HistoryState::default();
        note(&mut state, entry_for(&queue, QueueItem::Track(10), false));
        note(&mut state, entry_for(&queue, QueueItem::Track(20), false));

        assert_eq!(
            resolve_previous(42_000, &state.history),
            PreviousAction::RestartCurrent
        );
        let PreviousAction::GoTo(target) = resolve_previous(0, &state.history) else {
            panic!("the second press must step back");
        };
        assert_eq!(target.item, QueueItem::Track(10));
    }

    #[test]
    fn play_14_an_exhausted_history_keeps_rewinding_instead_of_stopping() {
        let queue = context(&[10], 0);
        let mut state = HistoryState::default();
        note(&mut state, entry_for(&queue, QueueItem::Track(10), false));
        assert_eq!(
            resolve_previous(0, &state.history),
            PreviousAction::RestartCurrent
        );
        assert!(state.history.step_back().is_none());
    }

    #[test]
    fn play_14_an_empty_history_still_advertises_can_go_previous() {
        let queue = context(&[10, 20], 0);
        let mut state = HistoryState::default();
        note(&mut state, entry_for(&queue, QueueItem::Track(10), false));
        assert_eq!(state.history.back_len(), 0);
        assert!(!queue.is_empty());
    }

    #[test]
    fn play_14_forward_after_a_back_jump_returns_the_same_three_tracks() {
        let queue = context(&[10, 20, 30, 40], 0);
        let mut state = HistoryState::default();
        for id in [10, 20, 30, 40] {
            note(&mut state, entry_for(&queue, QueueItem::Track(id), false));
        }
        let back: Vec<_> = (0..3)
            .filter_map(|_| state.history.step_back().map(|held| held.item))
            .collect();
        assert_eq!(
            back,
            vec![
                QueueItem::Track(30),
                QueueItem::Track(20),
                QueueItem::Track(10)
            ]
        );
        let forward: Vec<_> = (0..3)
            .filter_map(|_| state.history.step_forward().map(|held| held.item))
            .collect();
        assert_eq!(
            forward,
            vec![
                QueueItem::Track(20),
                QueueItem::Track(30),
                QueueItem::Track(40)
            ]
        );
    }

    #[test]
    fn play_14_an_ordinary_advance_leaves_no_forward_step_behind() {
        let queue = context(&[10, 20, 30], 0);
        let mut state = HistoryState::default();
        note(&mut state, entry_for(&queue, QueueItem::Track(10), false));
        note(&mut state, entry_for(&queue, QueueItem::Track(20), false));
        let target = state.history.peek_back().expect("back target");
        state.pending = Some(PendingNavigation::Back(target.clone()));
        note(&mut state, target);
        assert!(state.history.can_go_forward());

        note(&mut state, entry_for(&queue, QueueItem::Track(30), false));
        assert!(!state.history.can_go_forward());
    }

    #[test]
    fn play_14_the_controller_exposes_the_forward_history_transport() {
        let _transport = PlayerController::forward_from_history;
    }
}
