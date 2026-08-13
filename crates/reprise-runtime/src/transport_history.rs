//! PLAY-14 history wiring for the toolkit-neutral runtime transport.
//!
//! The former `interrupted` branch in `skip_back` becomes ordinary history:
//! a Play Next track sits after the context entry it interrupted, carrying
//! that context entry's playhead and source classification with it.

use reprise_core::playback_history::{HistoryEntry, PlaybackHistory};
use reprise_core::queue::Queue;
use reprise_core::up_next::QueueItem;

use super::{Source, Transport};

enum PendingNavigation {
    Back(HistoryEntry),
    Forward(HistoryEntry),
}

#[derive(Default)]
pub(super) struct HistoryState {
    history: PlaybackHistory,
    pending: Option<PendingNavigation>,
}

impl HistoryState {
    fn note(&mut self, entry: HistoryEntry) {
        if let Some(pending) = self.pending.take() {
            let (target, committed) = match pending {
                PendingNavigation::Back(target) => {
                    let committed = if target.same_replay_target(&entry) {
                        self.history.step_back()
                    } else {
                        None
                    };
                    (target, committed)
                }
                PendingNavigation::Forward(target) => {
                    let committed = if target.same_replay_target(&entry) {
                        self.history.step_forward()
                    } else {
                        None
                    };
                    (target, committed)
                }
            };
            if target.same_replay_target(&entry) {
                debug_assert_eq!(committed, Some(target));
                return;
            }
        }
        self.history.record(entry);
    }

    fn begin_back_navigation(&mut self, target: HistoryEntry) {
        self.pending = Some(PendingNavigation::Back(target));
    }

    fn forward_target(&mut self) -> Option<HistoryEntry> {
        let target = self.history.peek_forward()?;
        self.pending = Some(PendingNavigation::Forward(target.clone()));
        Some(target)
    }
}

fn entry_for(queue: &Queue, track_id: i64, source: Source) -> HistoryEntry {
    HistoryEntry {
        item: QueueItem::Track(track_id),
        replay_uri: None,
        context_pos: if source == Source::Context {
            queue.current_order_position()
        } else {
            None
        },
        sequence: queue.sequence_identity(),
        from_up_next: source == Source::PlayNext,
    }
}

impl Transport {
    pub(super) fn note_playback_started(&mut self, track_id: i64, source: Source) {
        let entry = entry_for(&self.queue, track_id, source);
        self.history.note(entry);
    }

    pub(super) fn history_view(&self) -> &PlaybackHistory {
        &self.history.history
    }

    pub(super) fn begin_history_back_navigation(&mut self, target: HistoryEntry) {
        self.history.begin_back_navigation(target);
    }

    pub(super) fn history_forward_target(&mut self) -> Option<HistoryEntry> {
        self.history.forward_target()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_14_a_play_next_track_leaves_the_interrupted_entry_one_step_back() {
        let mut queue = Queue::new();
        queue.set_tracks(vec![10, 20], 0);
        let mut state = HistoryState::default();
        state.note(entry_for(&queue, 10, Source::Context));
        queue.jump_to_order_position(1);
        state.note(entry_for(&queue, 20, Source::Context));
        state.note(entry_for(&queue, 99, Source::PlayNext));

        let back = state
            .history
            .peek_back()
            .expect("interrupted context entry");
        assert_eq!(back.item, QueueItem::Track(20));
        assert_eq!(back.playhead_in(queue.sequence_identity()), Some(1));
        assert!(!back.from_up_next);
    }

    #[test]
    fn play_14_a_failed_history_start_does_not_consume_the_next_real_start() {
        let mut queue = Queue::new();
        queue.set_tracks(vec![10, 20, 99], 0);
        let mut state = HistoryState::default();
        state.note(entry_for(&queue, 10, Source::Context));
        state.note(entry_for(&queue, 20, Source::Context));

        let target = state.history.peek_back().expect("back target");
        assert_eq!(target.item, QueueItem::Track(10));
        state.begin_back_navigation(target);
        state.note(entry_for(&queue, 99, Source::Context));

        assert_eq!(
            state.history.current().map(|entry| entry.item),
            Some(QueueItem::Track(99))
        );
    }

    #[test]
    fn play_14_a_confirmed_history_start_survives_stale_context_metadata() {
        let mut queue = Queue::new();
        queue.set_tracks(vec![10, 20], 0);
        let mut state = HistoryState::default();
        state.note(entry_for(&queue, 10, Source::Context));
        queue.jump_to_order_position(1);
        state.note(entry_for(&queue, 20, Source::Context));

        let target = state.history.peek_back().expect("back target");
        state.begin_back_navigation(target.clone());
        queue.set_shuffle(true);
        state.note(entry_for(&queue, 10, Source::Context));

        assert_eq!(state.history.back_len(), 0);
        assert_eq!(state.history.current(), Some(target));
        assert!(state.history.can_go_forward());
    }

    #[test]
    fn previous_transport_uses_the_payload_resolve_previous_already_returned() {
        let implementation = include_str!("transport_controls.rs");
        let direct_payload = ["PreviousAction::GoTo(", "target", ")"].concat();

        assert!(implementation.contains(&direct_payload));
    }

    #[test]
    fn context_position_uses_an_explicit_source_branch() {
        let implementation = include_str!("transport_history.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("implementation section");
        let chained_option = [".then(", "|| queue.current_order_position())"].concat();

        assert!(!implementation.contains(&chained_option));
    }
}
