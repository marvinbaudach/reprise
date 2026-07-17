//! Playback-context and manual Up Next transport methods, split out of
//! `player_controller.rs` to keep that file under the project's file-size
//! limit. The hidden `queue` remains the selected Library/playlist context;
//! the visible Queue source is backed only by `up_next`.
//!
//! These are `pub(in crate::ui)` so `mpris_mirror.rs`'s `handle_mpris_command` (and,
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

/// The Queue view's composite parts (QUE-1), in display order. Produced by
/// `PlayerController::queue_view_sections`, composed into the visible model
/// by `ui::track_list::queue_sections::compose`.
pub(crate) struct QueueViewSections {
    pub now_playing: Option<i64>,
    pub play_next: Vec<i64>,
    pub up_next_rest: Vec<i64>,
    pub origin_label: Option<String>,
}

impl PlayerController {
    pub(in crate::ui) fn set_on_queue_changed(&self, callback: impl Fn() + 'static) {
        *self.queue_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn notify_queue_changed(&self) {
        tracing::info!(up_next_len = self.up_next.borrow().len(), "up next changed");
        let callback = self.queue_changed.borrow().clone();
        if let Some(callback) = callback {
            callback();
        }
        // The up-next front / queue order may have changed, so the upcoming
        // track changed: re-feed the gapless next. All up-next edits funnel
        // through here. `feed_next` only takes short, sequential borrows, and
        // every caller of `notify_queue_changed` holds no live borrow across
        // it (see `## Queue borrow discipline`).
        self.feed_next();
    }

    /// Starts the restored queue's current track while stopped; otherwise
    /// toggles the already-loaded pipeline. Shared by the bar, Space, and
    /// MPRIS PlayPause, without ever introducing startup autoplay.
    pub(in crate::ui) fn toggle_pause(&self) {
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
    pub(in crate::ui) fn previous(&self) {
        self.previous_with_up_next();
    }

    /// Steps the queue to the next track and plays it (or resets to stopped
    /// if there is none) — shared by the bar's next button and MPRIS's
    /// `Next` method. Same borrow discipline as `previous`.
    pub(in crate::ui) fn next(&self) {
        self.advance_playback(AdvanceReason::Manual);
    }

    /// Appends explicit user selections to visible Up Next without replacing
    /// or starting the hidden playback context. Duplicates remain meaningful
    /// user choices; an empty slice is a no-op.
    pub(in crate::ui) fn append_to_queue(&self, ids: &[i64]) {
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

    /// The Queue view's three parts in display order (QUE-1): the playing
    /// track, pending manual entries, and the snapshot's play-order tail —
    /// plus the origin label for the "Up Next · from <label>" title. Each
    /// list is cloned out in its own statement (borrow discipline).
    pub(in crate::ui) fn queue_view_sections(&self) -> QueueViewSections {
        let now_playing = self.now_playing.borrow().as_ref().map(|np| np.id);
        let play_next = self.up_next.borrow().ids().to_vec();
        let up_next_rest = self.queue.borrow().remaining_after_current();
        let origin_label = self
            .play_origin
            .borrow()
            .as_ref()
            .map(|origin| origin.label.clone());
        QueueViewSections {
            now_playing,
            play_next,
            up_next_rest,
            origin_label,
        }
    }

    /// QUE-5: the sidebar's "Queue · N" — pending manual entries plus the
    /// snapshot tracks still ahead of the playhead, NOT the total snapshot.
    pub(in crate::ui) fn queue_pending_len(&self) -> usize {
        let pending = self.up_next.borrow().len();
        let remaining = self.queue.borrow().remaining_len();
        pending + remaining
    }

    /// QUE-3's "Play next": the given ids jump the manual line (front of
    /// Play Next), unlike `append_to_queue`'s back-of-line append.
    pub(in crate::ui) fn play_next(&self, ids: &[i64]) {
        if ids.is_empty() {
            tracing::debug!("play next: nothing to add; ignoring");
            return;
        }
        self.up_next.borrow_mut().prepend(ids);
        self.notify_queue_changed();
        self.sync_transport_enabled(true);
        tracing::info!(added = ids.len(), "tracks queued to play next");
    }

    /// QUE-3's "Clear queue" button: empties ONLY the manual Play Next
    /// list; the playback snapshot survives until stop or a new context.
    pub(in crate::ui) fn clear_play_next(&self) {
        let had_any = {
            let mut up_next = self.up_next.borrow_mut();
            let had_any = !up_next.is_empty();
            up_next.clear();
            had_any
        };
        if had_any {
            self.notify_queue_changed();
            tracing::info!("play next cleared");
        }
    }

    /// QUE-3 remove: each composite row is removed from ITS list — manual
    /// entries from Play Next, snapshot rows (single occurrence) from the
    /// context. Removing the Now Playing row skips ahead: the snapshot drops
    /// it and playback continues with the next target (or stops cleanly).
    /// Returns how many rows were removed (for the toast).
    pub(in crate::ui) fn remove_queue_rows(
        &self,
        rows: &[crate::ui::track_list::queue_row_mapping::QueueRow],
    ) -> usize {
        use crate::ui::track_list::queue_row_mapping::QueueRow;

        let mut play_next_indices = Vec::new();
        let mut up_next_offsets = Vec::new();
        let mut remove_current = false;
        for row in rows {
            match row {
                QueueRow::PlayNext(index) => play_next_indices.push(*index),
                QueueRow::UpNext(offset) => up_next_offsets.push(*offset),
                QueueRow::NowPlaying => remove_current = true,
            }
        }

        let mut removed = 0;
        if !play_next_indices.is_empty() {
            removed += self
                .up_next
                .borrow_mut()
                .remove_positions(&play_next_indices);
        }
        if !up_next_offsets.is_empty() {
            let did_remove = {
                let mut queue = self.queue.borrow_mut();
                match queue.current_order_position() {
                    Some(base) => {
                        let positions: Vec<usize> = up_next_offsets
                            .iter()
                            .map(|offset| base + 1 + offset)
                            .collect();
                        queue.remove_order_positions(&positions)
                    }
                    None => false,
                }
            };
            if did_remove {
                removed += up_next_offsets.len();
            }
        }
        if remove_current {
            removed += 1;
            if self.current_up_next.get().is_some() {
                // The playing track is a consumed manual entry — nothing to
                // drop from any list; removing it just means "skip it now".
                self.next();
            } else {
                // Drop the current snapshot row (the playhead advances to
                // the next survivor) and continue playback there.
                let next = {
                    let mut queue = self.queue.borrow_mut();
                    match queue.current_order_position() {
                        Some(position) => {
                            queue.remove_order_positions(&[position]);
                            queue.current()
                        }
                        None => None,
                    }
                };
                match next {
                    Some(id) => self.play_track_id(id),
                    None => self.reset_to_stopped(),
                }
            }
        }

        if removed > 0 {
            self.notify_queue_changed();
        }
        removed
    }

    /// QUE-3 drag semantics over the composite view: reorder within Play
    /// Next, or promote an Up Next snapshot row into Play Next (removed
    /// from the snapshot so it can't play twice).
    pub(in crate::ui) fn reorder_queue_rows(
        &self,
        op: crate::ui::track_list::queue_row_mapping::QueueReorderOp,
    ) -> bool {
        use crate::ui::track_list::queue_row_mapping::QueueReorderOp;

        let moved = match op {
            QueueReorderOp::WithinPlayNext { from, to } => {
                self.up_next.borrow_mut().move_item(from, to)
            }
            QueueReorderOp::PromoteUpNext {
                up_next_offset,
                insert_at,
            } => {
                let promoted = {
                    let mut queue = self.queue.borrow_mut();
                    match queue.current_order_position() {
                        Some(base) => {
                            let position = base + 1 + up_next_offset;
                            let id = queue.id_at_order_position(position);
                            if let Some(id) = id {
                                queue.remove_order_positions(&[position]);
                                Some(id)
                            } else {
                                None
                            }
                        }
                        None => None,
                    }
                };
                match promoted {
                    Some(id) => {
                        self.up_next.borrow_mut().insert(insert_at, id);
                        true
                    }
                    None => false,
                }
            }
        };
        if moved {
            self.notify_queue_changed();
        }
        moved
    }

    /// QUE-3 double-click on a queue row: move the playhead there — no
    /// context rebuild. A Play Next row drains the manual line through it
    /// (`play_up_next_at`); an Up Next row jumps the snapshot playhead; the
    /// Now Playing row restarts itself.
    pub(in crate::ui) fn jump_to_queue_row(
        &self,
        row: crate::ui::track_list::queue_row_mapping::QueueRow,
    ) {
        use crate::ui::track_list::queue_row_mapping::QueueRow;

        match row {
            QueueRow::PlayNext(index) => self.play_up_next_at(index),
            QueueRow::UpNext(offset) => {
                let target = {
                    let mut queue = self.queue.borrow_mut();
                    match queue.current_order_position() {
                        Some(base) => queue.jump_to_order_position(base + 1 + offset),
                        None => None,
                    }
                };
                let Some(id) = target else {
                    tracing::warn!(offset, "queue jump target vanished; ignoring");
                    return;
                };
                self.current_up_next.set(None);
                self.notify_queue_changed();
                self.play_track_id(id);
            }
            QueueRow::NowPlaying => {
                let current = self
                    .current_up_next
                    .get()
                    .or_else(|| self.queue.borrow().current());
                if let Some(id) = current {
                    self.play_track_id(id);
                }
            }
        }
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
    pub(in crate::ui) fn purge_queue_ids(&self, ids: &[i64]) {
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
