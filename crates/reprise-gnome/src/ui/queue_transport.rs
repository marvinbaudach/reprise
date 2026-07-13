//! Playback-context and manual Up Next transport methods, split out of
//! `player_controller.rs` to keep that file under the project's file-size
//! limit. The hidden `queue` remains the selected Library/playlist context;
//! the visible Queue source is backed only by `up_next`.
//!
//! These are `pub(super)` so `mpris_mirror.rs`'s `handle_mpris_command` (and,
//! for `queue_ids_snapshot`, `track_list.rs`) can call them too — shared with
//! the bar's own button clicks so a physical media key and the on-screen
//! control run exactly one code path (DRY).
//!
//! Borrow discipline: every `queue` access here follows `player_controller.
//! rs`'s `## Queue borrow discipline` doc section — each borrow runs inside
//! its own statement/block, dropped before any call that could re-enter this
//! controller.

use std::rc::Rc;

use crate::ui::player_controller::PlayerController;
use crate::ui::up_next_transport::AdvanceReason;
use reprise_core::media_integration::MprisPlaybackStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToggleAction {
    StartCurrent,
    StartPending,
    TogglePipeline,
    Noop,
}

fn toggle_action(
    status: MprisPlaybackStatus,
    current_track: Option<i64>,
    has_pending: bool,
) -> ToggleAction {
    match (status, current_track, has_pending) {
        (MprisPlaybackStatus::Stopped, Some(_), _) => ToggleAction::StartCurrent,
        (MprisPlaybackStatus::Stopped, None, true) => ToggleAction::StartPending,
        (MprisPlaybackStatus::Stopped, None, false) => ToggleAction::Noop,
        (MprisPlaybackStatus::Playing | MprisPlaybackStatus::Paused, _, _) => {
            ToggleAction::TogglePipeline
        }
    }
}

impl PlayerController {
    pub(super) fn set_on_queue_changed(&self, callback: impl Fn() + 'static) {
        *self.queue_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn notify_queue_changed(&self) {
        tracing::info!(up_next_len = self.up_next.borrow().len(), "up next changed");
        let callback = self.queue_changed.borrow().clone();
        if let Some(callback) = callback {
            callback();
        }
    }

    /// Starts the restored queue's current track while stopped; otherwise
    /// toggles the already-loaded pipeline. Shared by the bar, Space, and
    /// MPRIS PlayPause, without ever introducing startup autoplay.
    pub(super) fn toggle_pause(&self) {
        let status = self
            .mpris_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status;
        let current = self
            .current_up_next
            .get()
            .or_else(|| self.queue.borrow().current());
        let has_pending = !self.up_next.borrow().is_empty();
        match toggle_action(status, current, has_pending) {
            ToggleAction::StartCurrent => {
                if let Some(id) = current {
                    self.play_track_id(id);
                }
            }
            ToggleAction::StartPending => self.advance_playback(AdvanceReason::Manual),
            ToggleAction::TogglePipeline => {
                if let Err(error) = self.player.toggle_pause() {
                    tracing::error!(%error, "toggle play/pause failed");
                }
            }
            ToggleAction::Noop => tracing::debug!("play/pause: queue is empty; nothing to play"),
        }
    }

    /// Steps the queue to the previous track and plays it (or resets to
    /// stopped if there is none) — shared by the bar's previous button and
    /// MPRIS's `Previous` method. Borrow discipline: `previous()` runs
    /// inside this one `let` statement, so the borrow drops before
    /// `play_track_id`/`reset_to_stopped` run.
    pub(super) fn previous(&self) {
        self.previous_with_up_next();
    }

    /// Steps the queue to the next track and plays it (or resets to stopped
    /// if there is none) — shared by the bar's next button and MPRIS's
    /// `Next` method. Same borrow discipline as `previous`.
    pub(super) fn next(&self) {
        self.advance_playback(AdvanceReason::Manual);
    }

    /// Appends explicit user selections to visible Up Next without replacing
    /// or starting the hidden playback context. Duplicates remain meaningful
    /// user choices; an empty slice is a no-op.
    pub(super) fn append_to_queue(&self, ids: &[i64]) {
        if ids.is_empty() {
            tracing::debug!("append to queue: nothing to add; ignoring");
            return;
        }
        self.up_next.borrow_mut().append(ids);
        self.notify_queue_changed();
        let queue_len = self.up_next.borrow().len();
        self.sync_transport_enabled(true);
        tracing::info!(added = ids.len(), queue_len, "tracks added to queue");
    }

    /// Snapshot of pending manual ids in stable visible order. The Queue view
    /// asks for a fresh owned value on each reload, so consumption, removal,
    /// and drag reorder cannot expose the hidden context or a stale list.
    pub(super) fn queue_ids_snapshot(&self) -> Vec<i64> {
        self.up_next.borrow().ids().to_vec()
    }

    pub(super) fn up_next_len(&self) -> usize {
        self.up_next.borrow().len()
    }

    pub(super) fn remove_up_next_positions(&self, positions: &[usize]) -> usize {
        let removed = self.up_next.borrow_mut().remove_positions(positions);
        if removed > 0 {
            self.notify_queue_changed();
        }
        removed
    }

    /// Reorders pending manual entries only. The caller reloads Queue after a
    /// successful mutation; invalid and no-op positions return `false`.
    pub(super) fn move_queue_item(&self, from: usize, to: usize) -> bool {
        self.up_next.borrow_mut().move_item(from, to)
    }

    /// Purges hard-deleted track ids from the queue (Stage-3 close-out):
    /// "Remove from library" (`queries::remove_missing_tracks`) deletes
    /// `tracks` rows outright — without this, a queued id that no longer
    /// resolves to a row desyncs `Queue::len`/`ids_in_order` from what
    /// `ViewSource::Queue`'s window query can actually render (see
    /// `queries.rs`'s module doc, `Queue` section, and `query_track_count`'s
    /// `Queue` arm). Called from `ui::track_list_context_menu::handle_
    /// remove_from_library` with exactly the ids `remove_missing_tracks`
    /// reports as actually deleted — never the raw requested selection,
    /// which could include ids that turned out not to be missing any more
    /// and so were never deleted. A no-op for an empty slice (no `queue`
    /// borrow taken at all).
    pub(super) fn purge_queue_ids(&self, ids: &[i64]) {
        if ids.is_empty() {
            return;
        }
        let context_changed = self.queue.borrow_mut().remove_ids(ids);
        let pending_changed = self.up_next.borrow_mut().remove_ids(ids);
        if self
            .current_up_next
            .get()
            .is_some_and(|id| ids.contains(&id))
        {
            self.current_up_next.set(None);
        }
        if context_changed || pending_changed {
            tracing::info!(
                removed = ids.len(),
                queue_len = self.up_next.borrow().len(),
                "queue purged of hard-deleted track ids"
            );
            if pending_changed {
                self.notify_queue_changed();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopped_toggle_starts_current_queue_track_without_autoplay() {
        assert_eq!(
            toggle_action(MprisPlaybackStatus::Stopped, Some(42), false),
            ToggleAction::StartCurrent
        );
        assert_eq!(
            toggle_action(MprisPlaybackStatus::Stopped, None, true),
            ToggleAction::StartPending
        );
        assert_eq!(
            toggle_action(MprisPlaybackStatus::Stopped, None, false),
            ToggleAction::Noop
        );
    }
}
