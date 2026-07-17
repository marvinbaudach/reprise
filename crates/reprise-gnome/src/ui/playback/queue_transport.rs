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

use crate::ui::player_controller::{PlayerController, StartPlayback};
use crate::ui::up_next_transport::AdvanceReason;
use reprise_core::media_integration::MprisPlaybackStatus;
use reprise_core::queue::Queue;
use reprise_core::up_next::UpNextQueue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToggleAction {
    StartCurrent,
    StartPending,
    TogglePipeline,
    Noop,
}

/// The track that should take over when the *currently playing* one was
/// hard-deleted out from under the pipeline (see `purge_queue_ids`). Runs
/// AFTER the purge has already mutated both queues, which is the one thing
/// that keeps it from mirroring `up_next_transport.rs`'s `next_target`:
///
/// - Pending Up Next wins, exactly as it would on a natural end-of-track.
/// - Otherwise the context takes over, and *how* depends on where the deleted
///   track came from. Playing from the context: `Queue::remove_ids` has
///   already stepped the cursor onto the next survivor, so `current()` IS the
///   successor and advancing again would skip a track. Playing from Up Next:
///   the cursor still sits on the context track that played *before* the
///   interjection, so it must step forward like `next_target` does.
///
/// Repeat mode deliberately gets no say: `Repeat::One` cannot loop a track
/// that no longer exists, and `advance_auto` still honours `Repeat::All` for
/// the Up Next case.
fn successor_after_purge(
    context: &mut Queue,
    pending: &mut UpNextQueue,
    current_pending: &mut Option<i64>,
    was_playing_from_up_next: bool,
) -> Option<i64> {
    if let Some(next) = pending.pop_front() {
        *current_pending = Some(next);
        return Some(next);
    }
    *current_pending = None;
    if was_playing_from_up_next {
        context.advance_auto()
    } else {
        context.current()
    }
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

    /// Starts playback of `ids[start_index]` and loads the rest of `ids` into
    /// the queue as what auto-advance/previous/next step through. Row
    /// activation lands here — see `ui::track_list`'s `queue_ids_for_
    /// activation` for how `ids`/`start_index` are built from the currently
    /// visible sort/filter view. An empty `ids` (nothing to play) resets to
    /// stopped instead of calling `play_track_id`.
    ///
    /// Borrow discipline: `set_tracks` and `current()` each run inside their
    /// own statement, so their `queue` borrows drop before `play_track_id`/
    /// `reset_to_stopped` run — see the module's `## Queue borrow
    /// discipline` doc section.
    pub fn play_from_view(
        &self,
        ids: Vec<i64>,
        start_index: usize,
        origin: super::play_origin::PlayOrigin,
    ) {
        self.queue.borrow_mut().set_tracks(ids, start_index);
        self.current_up_next.set(None);

        let queue_len = self.queue.borrow().len();
        // An empty seed (nothing to play) resets to stopped below and must
        // not claim an origin for a context that does not exist.
        *self.play_origin.borrow_mut() = (queue_len > 0).then_some(origin);

        tracing::info!(queue_len, start_index, "queue set from view");

        let has_transport = !self.queue.borrow().is_empty() || !self.up_next.borrow().is_empty();
        self.sync_transport_enabled(has_transport);

        let current = self.queue.borrow().current();
        match current {
            Some(id) => self.play_track_id(id),
            None => self.reset_to_stopped(),
        }
        // The Queue view and sidebar counter render the snapshot (QUE-1/
        // QUE-5), so a reseeded context is a queue change for them.
        self.notify_queue_changed();
    }

    /// The current playback context's origin, if any — clone-out so no
    /// borrow escapes (see `## Queue borrow discipline`).
    pub(in crate::ui) fn current_play_origin(&self) -> Option<super::play_origin::PlayOrigin> {
        self.play_origin.borrow().clone()
    }

    /// The Queue view's three parts in display order (QUE-1): the playing
    /// track, pending manual entries, and the snapshot's play-order tail —
    /// plus the origin label for the `Up Next · from <label>` title. Each
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
    /// Next, reorder within the Up Next snapshot tail (both positions stay
    /// strictly after the playhead, so `Queue::move_item` can't move the
    /// currently playing track), or promote an Up Next snapshot row into
    /// Play Next (removed from the snapshot so it can't play twice).
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
            QueueReorderOp::WithinUpNext { from, to } => {
                let mut queue = self.queue.borrow_mut();
                match queue.current_order_position() {
                    Some(base) => queue.move_item(base + 1 + from, base + 1 + to),
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
    ///
    /// Purging the models is only half the job: when the deleted id is the
    /// track the pipeline is *currently playing*, this also hands playback to
    /// its successor (`successor_after_purge`), or stops when nothing
    /// survives. Nothing else would — deleting never attempts a `play()`, so
    /// `handle_unplayable_track`'s skip path can't fire, and the audio itself
    /// is unaffected by the file going away (trashing is a rename, and even a
    /// real unlink leaves an open descriptor's inode playable). Without this,
    /// a trashed track keeps playing to its end.
    pub(in crate::ui) fn purge_queue_ids(&self, ids: &[i64]) {
        if ids.is_empty() {
            return;
        }
        // Read before the purge below clears `current_up_next`: which track
        // the user is actually hearing, and whether it came from Up Next
        // rather than the playback context. `now_playing` is the loaded
        // track's identity (the same source `present_track` compares its
        // `previous_id` against); the borrow ends inside this statement.
        let playing = self.now_playing.borrow().as_ref().map(|track| track.id);
        let playing_purged = playing.is_some_and(|id| ids.contains(&id));
        let played_from_up_next = playing_purged && self.current_up_next.get() == playing;

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
            // Both lists are visible now (composite Queue view + QUE-5
            // pending counter), so a context-only purge must refresh too
            // (adversarial review, queue+nav plan, finding 3).
            self.notify_queue_changed();
        }
        // The loaded track itself was purged: the successor logic below
        // (merged from feat/queue-dnd) both skips playback ahead AND keeps
        // the composite Queue view's Now Playing row from pointing at a
        // dead id (adversarial review finding 4) — `present_track` reloads
        // `now_playing`, the stop path clears it.
        if !playing_purged {
            return;
        }
        // The pipeline is still playing the deleted file — trashing is a
        // rename, and an already-open descriptor keeps its inode alive — so
        // nothing stops on its own. Take over: hand playback to the
        // successor, or stop when nothing survives.
        let before = self.up_next.borrow().len();
        let mut current_pending = self.current_up_next.get();
        let successor = {
            let mut context = self.queue.borrow_mut();
            let mut pending = self.up_next.borrow_mut();
            successor_after_purge(
                &mut context,
                &mut pending,
                &mut current_pending,
                played_from_up_next,
            )
        };
        self.current_up_next.set(current_pending);
        if self.up_next.borrow().len() != before {
            self.notify_queue_changed();
        }
        match successor {
            Some(next) => {
                tracing::info!(
                    deleted = ?playing,
                    next,
                    "the playing track was deleted; skipping to its successor"
                );
                self.present_track(next, StartPlayback::Yes);
            }
            None => {
                tracing::info!(
                    deleted = ?playing,
                    "the playing track was deleted and nothing follows it; stopping"
                );
                self.consecutive_skips.set(0);
                self.failure_skip_limit.set(0);
                self.reset_to_stopped();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::queue::Repeat;

    /// Context queue seeded with `ids`, currently playing the one at
    /// `start_index` — the ordinary "playing from the Library view" shape.
    fn context(ids: &[i64], start_index: usize) -> Queue {
        let mut queue = Queue::new();
        queue.set_tracks(ids.to_vec(), start_index);
        queue
    }

    fn pending(ids: &[i64]) -> UpNextQueue {
        let mut up_next = UpNextQueue::default();
        up_next.append(ids);
        up_next
    }

    #[test]
    fn purging_the_playing_context_track_plays_the_next_surviving_one() {
        let mut queue = context(&[10, 20, 30], 0);
        let mut up_next = pending(&[]);
        let mut current_pending = None;
        // `purge_queue_ids` runs this first; it already steps the cursor onto
        // the next survivor, which is why the successor must NOT advance again.
        queue.remove_ids(&[10]);

        let next = successor_after_purge(&mut queue, &mut up_next, &mut current_pending, false);

        assert_eq!(next, Some(20));
        assert_eq!(current_pending, None);
    }

    #[test]
    fn purging_the_playing_context_track_prefers_a_pending_up_next_track() {
        let mut queue = context(&[10, 20], 0);
        let mut up_next = pending(&[99]);
        let mut current_pending = None;
        queue.remove_ids(&[10]);

        let next = successor_after_purge(&mut queue, &mut up_next, &mut current_pending, false);

        assert_eq!(next, Some(99));
        assert_eq!(current_pending, Some(99));
        assert!(up_next.is_empty());
    }

    #[test]
    fn purging_the_playing_up_next_track_steps_the_context_forward() {
        // The context cursor still sits on 10 — the track that played before
        // the up-next interjection — so resuming it would replay it.
        let mut queue = context(&[10, 20], 0);
        let mut up_next = pending(&[]);
        let mut current_pending = None;

        let next = successor_after_purge(&mut queue, &mut up_next, &mut current_pending, true);

        assert_eq!(next, Some(20));
        assert_eq!(current_pending, None);
    }

    #[test]
    fn purging_the_last_surviving_track_stops_playback() {
        let mut queue = context(&[10], 0);
        let mut up_next = pending(&[]);
        let mut current_pending = None;
        queue.remove_ids(&[10]);

        let next = successor_after_purge(&mut queue, &mut up_next, &mut current_pending, false);

        assert_eq!(next, None);
    }

    #[test]
    fn purging_the_playing_track_under_repeat_one_moves_on_instead_of_looping() {
        // Repeat::One cannot repeat a track that no longer exists, so the
        // deleted track's successor wins over the repeat mode.
        let mut queue = context(&[10, 20], 0);
        queue.set_repeat(Repeat::One);
        let mut up_next = pending(&[]);
        let mut current_pending = None;
        queue.remove_ids(&[10]);

        let next = successor_after_purge(&mut queue, &mut up_next, &mut current_pending, false);

        assert_eq!(next, Some(20));
    }

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
