//! PLAY-14 history wiring for Android's Core-owned playback session.
//!
//! Android plays tracks only: episodes are filtered at the queue boundary.
//! This module therefore records stable track identities and the content URI
//! needed to replay an entry after its original context has been replaced.

use reprise_core::playback::PlaybackBackend;
use reprise_core::playback_history::{
    resolve_previous, HistoryEntry, PlaybackHistory, PreviousAction,
};
use reprise_core::queue::Queue;
use reprise_core::up_next::QueueItem;

use super::{AndroidPlaybackError, AndroidPlaybackState, SessionInner, SessionState};

#[derive(Debug)]
enum PendingNavigation {
    Back(HistoryEntry),
    Forward(HistoryEntry),
}

#[derive(Debug, Default)]
pub(super) struct HistoryState {
    history: PlaybackHistory,
    pending: Option<PendingNavigation>,
    presented: Option<HistoryEntry>,
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
        if self.history.current().as_ref() == Some(&entry) {
            return;
        }
        self.presented = None;
        self.history.record(entry);
    }

    #[cfg(test)]
    fn back_target_for_navigation(&mut self) -> Option<HistoryEntry> {
        let target = self.history.peek_back()?;
        self.begin_back_navigation(target.clone());
        Some(target)
    }

    fn begin_back_navigation(&mut self, target: HistoryEntry) {
        self.pending = Some(PendingNavigation::Back(target));
    }

    fn forward_target_for_navigation(&mut self) -> Option<HistoryEntry> {
        let target = self.history.peek_forward()?;
        self.pending = Some(PendingNavigation::Forward(target.clone()));
        Some(target)
    }

    fn cancel_navigation(&mut self) {
        self.pending = None;
        self.presented = None;
    }

    pub(super) fn clear_presented(&mut self) {
        self.presented = None;
    }

    pub(super) fn presented(&self) -> Option<&HistoryEntry> {
        self.presented.as_ref()
    }

    fn present(&mut self, target: HistoryEntry) {
        self.presented = Some(target);
    }
}

fn entry_for(queue: &Queue, track_id: i64, replay_uri: String) -> HistoryEntry {
    HistoryEntry {
        item: QueueItem::Track(track_id),
        replay_uri: Some(replay_uri),
        context_pos: queue.current_order_position(),
        sequence: queue.sequence_identity(),
        from_up_next: false,
    }
}

fn history_target_playhead(queue: &Queue, target: &HistoryEntry) -> Option<usize> {
    let position = target.playhead_in(queue.sequence_identity())?;
    (queue.id_at_order_position(position) == target.item.track_id()).then_some(position)
}

impl SessionState {
    pub(super) fn history_entry_for_started(&self, track_id: i64, uri: String) -> HistoryEntry {
        self.history
            .presented()
            .filter(|target| {
                target.item.track_id() == Some(track_id)
                    && target.replay_uri.as_deref() == Some(uri.as_str())
            })
            .cloned()
            .unwrap_or_else(|| entry_for(&self.queue, track_id, uri))
    }

    pub(super) fn note_playback_started(&mut self, entry: HistoryEntry) {
        self.history.note(entry);
    }

    fn history_forward_target(&mut self) -> Option<HistoryEntry> {
        self.history.forward_target_for_navigation()
    }

    fn adopt_history_target(&mut self, target: HistoryEntry) {
        self.reset_fault_run();
        self.history.present(target);
        self.snapshot.current_index = None;
        self.snapshot.position_ms = 0;
        self.snapshot.duration_ms = 0;
        self.snapshot.state = AndroidPlaybackState::Playing;
        self.current_loaded = false;
        self.max_position_ms = 0;
        self.play_recorded = false;
    }
}

impl SessionInner {
    pub(super) fn previous_from_history(&self) -> Result<(), AndroidPlaybackError> {
        let action = {
            let state = self.lock()?;
            resolve_previous(state.snapshot.position_ms, &state.history.history)
        };
        let PreviousAction::GoTo(target) = action else {
            return self.rewind_current();
        };
        let queue_to_save = {
            let mut state = self.lock()?;
            state.history.begin_back_navigation(target.clone());
            self.adopt_target(&mut state, target);
            state.queue.clone()
        };
        self.persist_queue(&queue_to_save)?;
        self.start_navigation()
    }

    pub(super) fn forward_from_history(&self) -> Result<bool, AndroidPlaybackError> {
        let queue_to_save = {
            let mut state = self.lock()?;
            let Some(target) = state.history_forward_target() else {
                return Ok(false);
            };
            self.adopt_target(&mut state, target);
            state.queue.clone()
        };
        self.persist_queue(&queue_to_save)?;
        self.start_navigation()?;
        Ok(true)
    }

    fn adopt_target(&self, state: &mut SessionState, target: HistoryEntry) {
        if let Some(position) = history_target_playhead(&state.queue, &target) {
            if state.queue.jump_to_order_position(position).is_some() {
                state.adopt_current_for_play_intent();
                state.history.present(target);
                return;
            }
        }
        state.adopt_history_target(target);
    }

    fn start_navigation(&self) -> Result<(), AndroidPlaybackError> {
        if let Err(error) = self.start_current() {
            if let Ok(mut state) = self.state.lock() {
                state.history.cancel_navigation();
            }
            return Err(error);
        }
        Ok(())
    }

    fn rewind_current(&self) -> Result<(), AndroidPlaybackError> {
        self.backend()?
            .seek_to(0)
            .map_err(|error| AndroidPlaybackError::Backend {
                detail: error.to_string(),
            })?;
        self.lock()?.snapshot.position_ms = 0;
        self.notify();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::playback_history::HISTORY_CAPACITY;

    #[test]
    fn play_14_android_keeps_replay_uris_in_the_core_history_entry_only() {
        let source = include_str!("history.rs");
        for parallel_field in [
            ["back", "_uris: Vec"].concat(),
            ["forward", "_uris: Vec"].concat(),
            ["current", "_uri: Option"].concat(),
        ] {
            assert!(
                !source.contains(&parallel_field),
                "parallel URI history field remains: {parallel_field}"
            );
        }
    }

    #[test]
    fn play_14_a_history_navigation_start_is_not_recorded_again() {
        let mut queue = Queue::new();
        queue.set_tracks(vec![10, 20], 0);
        let mut state = HistoryState::default();
        state.note(entry_for(&queue, 10, "content://track/10".into()));
        queue.jump_to_order_position(1);
        state.note(entry_for(&queue, 20, "content://track/20".into()));

        let target = state
            .back_target_for_navigation()
            .expect("the first track is behind the current one");
        assert_eq!(target.item, QueueItem::Track(10));
        state.note(target);

        assert!(state.pending.is_none(), "navigation is one-shot state");
        assert_eq!(state.history.back_len(), 0);
        assert!(state.history.can_go_forward());
    }

    #[test]
    fn play_14_a_failed_android_history_start_keeps_the_real_predecessor() {
        let mut queue = Queue::new();
        queue.set_tracks(vec![10, 20, 99], 0);
        let mut state = HistoryState::default();
        state.note(entry_for(&queue, 10, "content://track/10".into()));
        queue.jump_to_order_position(1);
        state.note(entry_for(&queue, 20, "content://track/20".into()));

        assert_eq!(
            state.back_target_for_navigation().map(|entry| entry.item),
            Some(QueueItem::Track(10))
        );
        state.cancel_navigation();
        queue.jump_to_order_position(2);
        state.note(entry_for(&queue, 99, "content://track/99".into()));

        assert_eq!(
            state.back_target_for_navigation().map(|entry| entry.item),
            Some(QueueItem::Track(20))
        );
    }

    #[test]
    fn stopping_clears_a_presented_history_target_from_the_snapshot() {
        let mut state = SessionState::new();
        state.set_tracks(vec![10], vec!["content://track/10".into()], 0);
        assert_eq!(state.queue.advance_auto(), None);
        state
            .history
            .present(entry_for(&state.queue, 99, "content://history/99".into()));

        state.stop();

        let snapshot = state.presented_snapshot();
        assert_eq!(snapshot.state, AndroidPlaybackState::Stopped);
        assert_eq!(snapshot.current_track_id, None);
        assert_eq!(snapshot.current_track_uri, None);
    }

    #[test]
    fn an_invalid_recorded_playhead_falls_back_to_the_history_payload() {
        let mut queue = Queue::new();
        queue.set_tracks(vec![10, 20], 0);
        let mut target = entry_for(&queue, 10, "content://history/10".into());
        target.context_pos = Some(99);

        assert_eq!(history_target_playhead(&queue, &target), None);
    }

    #[test]
    fn a_started_uri_is_recorded_with_the_identity_captured_before_unlock() {
        let mut state = SessionState::new();
        state.set_tracks(vec![10], vec!["content://track/10".into()], 0);
        let started = state.history_entry_for_started(10, "content://track/10".into());

        state.set_tracks(vec![99], vec!["content://track/99".into()], 0);
        state.note_playback_started(started);

        let recorded = state.history.history.current().expect("recorded start");
        assert_eq!(recorded.item, QueueItem::Track(10));
        assert_eq!(recorded.replay_uri.as_deref(), Some("content://track/10"));
    }

    #[test]
    fn play_14_androids_replay_uris_are_bounded_with_the_history() {
        let mut state = HistoryState::default();
        for track_id in 0..(HISTORY_CAPACITY as i64 + 50) {
            let mut queue = Queue::new();
            queue.set_tracks(vec![track_id], 0);
            state.note(entry_for(
                &queue,
                track_id,
                format!("content://track/{track_id}"),
            ));
        }

        assert_eq!(state.history.back_len(), HISTORY_CAPACITY);
        assert_eq!(
            state.history.current().and_then(|entry| entry.replay_uri),
            Some("content://track/249".into())
        );
        for _ in 0..HISTORY_CAPACITY {
            let target = state
                .back_target_for_navigation()
                .expect("bounded back target");
            state.note(target);
        }
    }

    #[test]
    fn play_14_a_new_branch_keeps_every_uri_still_on_its_back_side() {
        let mut queue = Queue::new();
        queue.set_tracks(vec![10, 20, 30, 99], 0);
        let mut state = HistoryState::default();
        for (position, track_id) in [10, 20, 30].into_iter().enumerate() {
            queue.jump_to_order_position(position);
            state.note(entry_for(
                &queue,
                track_id,
                format!("content://track/{track_id}"),
            ));
        }
        let target = state.back_target_for_navigation().expect("20 is behind 30");
        state.note(target.clone());
        state.note(target);

        queue.jump_to_order_position(3);
        state.note(entry_for(&queue, 99, "content://track/99".into()));

        assert_eq!(
            state
                .back_target_for_navigation()
                .and_then(|target| target.replay_uri),
            Some("content://track/20".into())
        );
        let target = state
            .back_target_for_navigation()
            .expect("20 remains pending");
        state.note(target);
        assert_eq!(
            state
                .back_target_for_navigation()
                .and_then(|target| target.replay_uri),
            Some("content://track/10".into())
        );
    }

    #[test]
    fn play_14_repeated_track_ids_keep_the_uri_of_each_heard_occurrence() {
        let mut queue = Queue::new();
        queue.set_tracks(vec![10, 20, 10], 0);
        let mut state = HistoryState::default();
        state.note(entry_for(&queue, 10, "content://first/10".into()));
        queue.jump_to_order_position(1);
        state.note(entry_for(&queue, 20, "content://track/20".into()));
        queue.jump_to_order_position(2);
        state.note(entry_for(&queue, 10, "content://second/10".into()));

        let middle = state.back_target_for_navigation().expect("20 is behind 10");
        assert_eq!(middle.replay_uri.as_deref(), Some("content://track/20"));
        state.note(middle);
        let first = state
            .back_target_for_navigation()
            .expect("the first 10 remains");
        assert_eq!(first.replay_uri.as_deref(), Some("content://first/10"));
    }

    #[test]
    fn play_14_repeat_one_does_not_displace_androids_real_history() {
        let mut queue = Queue::new();
        queue.set_tracks(vec![10, 20], 0);
        let mut state = HistoryState::default();
        state.note(entry_for(&queue, 10, "content://track/10".into()));
        queue.jump_to_order_position(1);
        state.note(entry_for(&queue, 20, "content://track/20".into()));
        queue.set_repeat(reprise_core::queue::Repeat::One);

        for _ in 0..(reprise_core::playback_history::HISTORY_CAPACITY + 1) {
            assert_eq!(queue.advance_auto(), Some(20));
            state.note(entry_for(&queue, 20, "content://track/20".into()));
        }

        assert_eq!(state.history.back_len(), 1);
        assert_eq!(
            state.back_target_for_navigation().map(|entry| entry.item),
            Some(QueueItem::Track(10))
        );
    }
}
