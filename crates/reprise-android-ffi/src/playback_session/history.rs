//! PLAY-14 history wiring for Android's Core-owned playback session.
//!
//! Android plays tracks only: episodes are filtered at the queue boundary.
//! This module therefore records stable track identities and the content URI
//! needed to replay an entry after its original context has been replaced.

use reprise_core::playback::PlaybackBackend;
use reprise_core::playback_history::{
    resolve_previous, HistoryEntry, PlaybackHistory, PreviousAction, HISTORY_CAPACITY,
};
use reprise_core::queue::Queue;
use reprise_core::up_next::QueueItem;

use super::{AndroidPlaybackError, AndroidPlaybackState, SessionInner, SessionState};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HistoryTarget {
    pub(super) entry: HistoryEntry,
    pub(super) uri: String,
}

#[derive(Debug, Default)]
pub(super) struct HistoryState {
    history: PlaybackHistory,
    navigating: bool,
    back_uris: Vec<String>,
    forward_uris: Vec<String>,
    current_uri: Option<String>,
    presented: Option<HistoryTarget>,
}

impl HistoryState {
    fn note(&mut self, entry: HistoryEntry, uri: String) {
        if std::mem::take(&mut self.navigating) {
            return;
        }
        self.presented = None;
        self.forward_uris.clear();
        if let Some(previous) = self.current_uri.replace(uri) {
            self.back_uris.push(previous);
            if self.back_uris.len() > HISTORY_CAPACITY {
                let overflow = self.back_uris.len() - HISTORY_CAPACITY;
                self.back_uris.drain(0..overflow);
            }
        }
        self.history.record(entry);
    }

    fn step_back_for_navigation(&mut self) -> Option<HistoryTarget> {
        let entry = self.history.step_back()?;
        let uri = self
            .back_uris
            .pop()
            .expect("playback history and its URI payload diverged");
        if let Some(leaving) = self.current_uri.replace(uri.clone()) {
            self.forward_uris.push(leaving);
        }
        self.navigating = true;
        Some(HistoryTarget { entry, uri })
    }

    fn step_forward_for_navigation(&mut self) -> Option<HistoryTarget> {
        let entry = self.history.step_forward()?;
        let uri = self
            .forward_uris
            .pop()
            .expect("playback history and its URI payload diverged");
        if let Some(leaving) = self.current_uri.replace(uri.clone()) {
            self.back_uris.push(leaving);
        }
        self.navigating = true;
        Some(HistoryTarget { entry, uri })
    }

    fn cancel_navigation(&mut self) {
        self.navigating = false;
    }

    pub(super) fn clear_presented(&mut self) {
        self.presented = None;
    }

    pub(super) fn presented(&self) -> Option<&HistoryTarget> {
        self.presented.as_ref()
    }

    fn present(&mut self, target: HistoryTarget) {
        self.presented = Some(target);
    }
}

fn entry_for(queue: &Queue, track_id: i64) -> HistoryEntry {
    HistoryEntry {
        item: QueueItem::Track(track_id),
        context_pos: queue.current_order_position(),
        sequence: queue.sequence_identity(),
        from_up_next: false,
    }
}

impl SessionState {
    pub(super) fn note_playback_started(&mut self) {
        let Some(track_id) = self.current_track_id() else {
            return;
        };
        let Some(uri) = self.current_uri() else {
            return;
        };
        let entry = entry_for(&self.queue, track_id);
        self.history.note(entry, uri);
    }

    fn history_back_target(&mut self) -> Option<HistoryTarget> {
        self.history.step_back_for_navigation()
    }

    fn history_forward_target(&mut self) -> Option<HistoryTarget> {
        self.history.step_forward_for_navigation()
    }

    fn adopt_history_target(&mut self, target: HistoryTarget) {
        self.history.present(target);
        self.snapshot.current_index = None;
        self.snapshot.position_ms = 0;
        self.snapshot.duration_ms = 0;
        self.snapshot.state = AndroidPlaybackState::Playing;
        self.current_loaded = false;
        self.snapshot.error = None;
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
        if matches!(action, PreviousAction::RestartCurrent) {
            return self.rewind_current();
        }
        let queue_to_save = {
            let mut state = self.lock()?;
            let Some(target) = state.history_back_target() else {
                drop(state);
                return self.rewind_current();
            };
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

    fn adopt_target(&self, state: &mut SessionState, target: HistoryTarget) {
        let sequence = state.queue.sequence_identity();
        if let Some(position) = target.entry.playhead_in(sequence) {
            state.queue.jump_to_order_position(position);
            state.adopt_current();
            state.history.present(target);
        } else {
            state.adopt_history_target(target);
        }
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

    #[test]
    fn play_14_a_history_navigation_start_is_not_recorded_again() {
        let mut queue = Queue::new();
        queue.set_tracks(vec![10, 20], 0);
        let mut state = HistoryState::default();
        state.note(entry_for(&queue, 10), "content://track/10".into());
        queue.jump_to_order_position(1);
        state.note(entry_for(&queue, 20), "content://track/20".into());

        let target = state
            .step_back_for_navigation()
            .expect("the first track is behind the current one");
        assert_eq!(target.entry.item, QueueItem::Track(10));
        state.note(entry_for(&queue, 10), target.uri);

        assert!(!state.navigating, "the navigation marker is one-shot");
        assert_eq!(state.history.back_len(), 0);
        assert!(state.history.can_go_forward());
    }

    #[test]
    fn play_14_androids_replay_uris_are_bounded_with_the_history() {
        let mut state = HistoryState::default();
        for track_id in 0..(HISTORY_CAPACITY as i64 + 50) {
            let mut queue = Queue::new();
            queue.set_tracks(vec![track_id], 0);
            state.note(
                entry_for(&queue, track_id),
                format!("content://track/{track_id}"),
            );
        }

        assert_eq!(state.back_uris.len(), HISTORY_CAPACITY);
        assert_eq!(state.current_uri.as_deref(), Some("content://track/249"));
        for _ in 0..HISTORY_CAPACITY {
            assert!(state.step_back_for_navigation().is_some());
            state.navigating = false;
        }
    }

    #[test]
    fn play_14_a_new_branch_keeps_every_uri_still_on_its_back_side() {
        let mut queue = Queue::new();
        queue.set_tracks(vec![10, 20, 30, 99], 0);
        let mut state = HistoryState::default();
        for (position, track_id) in [10, 20, 30].into_iter().enumerate() {
            queue.jump_to_order_position(position);
            state.note(
                entry_for(&queue, track_id),
                format!("content://track/{track_id}"),
            );
        }
        let target = state.step_back_for_navigation().expect("20 is behind 30");
        state.note(target.entry, target.uri);

        queue.jump_to_order_position(3);
        state.note(entry_for(&queue, 99), "content://track/99".into());

        assert_eq!(
            state.step_back_for_navigation().map(|target| target.uri),
            Some("content://track/20".into())
        );
        state.navigating = false;
        assert_eq!(
            state.step_back_for_navigation().map(|target| target.uri),
            Some("content://track/10".into())
        );
    }

    #[test]
    fn play_14_repeated_track_ids_keep_the_uri_of_each_heard_occurrence() {
        let mut queue = Queue::new();
        queue.set_tracks(vec![10, 20, 10], 0);
        let mut state = HistoryState::default();
        state.note(entry_for(&queue, 10), "content://first/10".into());
        queue.jump_to_order_position(1);
        state.note(entry_for(&queue, 20), "content://track/20".into());
        queue.jump_to_order_position(2);
        state.note(entry_for(&queue, 10), "content://second/10".into());

        let middle = state.step_back_for_navigation().expect("20 is behind 10");
        assert_eq!(middle.uri, "content://track/20");
        state.navigating = false;
        let first = state
            .step_back_for_navigation()
            .expect("the first 10 remains");
        assert_eq!(first.uri, "content://first/10");
    }
}
