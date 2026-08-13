//! PLAY-14 history wiring for the toolkit-neutral runtime transport.
//!
//! The former `interrupted` branch in `skip_back` becomes ordinary history:
//! a Play Next track sits after the context entry it interrupted, carrying
//! that context entry's playhead and source classification with it.

use reprise_core::playback_history::{HistoryEntry, PlaybackHistory};
use reprise_core::queue::Queue;
use reprise_core::up_next::QueueItem;

use super::{Source, Transport};

#[derive(Default)]
pub(super) struct HistoryState {
    history: PlaybackHistory,
    navigating: bool,
}

impl HistoryState {
    fn note(&mut self, entry: HistoryEntry) {
        if std::mem::take(&mut self.navigating) {
            return;
        }
        self.history.record(entry);
    }

    fn step_back_for_navigation(&mut self) -> Option<HistoryEntry> {
        let target = self.history.step_back();
        self.navigating = target.is_some();
        target
    }

    fn step_forward_for_navigation(&mut self) -> Option<HistoryEntry> {
        let target = self.history.step_forward();
        self.navigating = target.is_some();
        target
    }
}

fn entry_for(queue: &Queue, track_id: i64, source: Source) -> HistoryEntry {
    HistoryEntry {
        item: QueueItem::Track(track_id),
        replay_uri: None,
        context_pos: (source == Source::Context)
            .then(|| queue.current_order_position())
            .flatten(),
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

    pub(super) fn history_back_target(&mut self) -> Option<HistoryEntry> {
        self.history.step_back_for_navigation()
    }

    pub(super) fn history_forward_target(&mut self) -> Option<HistoryEntry> {
        self.history.step_forward_for_navigation()
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
            .step_back_for_navigation()
            .expect("interrupted context entry");
        assert_eq!(back.item, QueueItem::Track(20));
        assert_eq!(back.playhead_in(queue.sequence_identity()), Some(1));
        assert!(!back.from_up_next);
    }
}
