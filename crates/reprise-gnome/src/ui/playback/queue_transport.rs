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
use crate::ui::track_list::queue_sections::{compose_virtual, QueueViewModel, VirtualContextTail};
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

#[derive(Debug, PartialEq, Eq)]
struct QueuePurgePlan {
    immediate: Vec<i64>,
    after_loaded_track: Option<i64>,
}

/// Separates a loaded catalog tombstone from every future deletion. The
/// loaded id stays as the queue playhead until playback leaves it, which
/// keeps ordinary next/previous/gapless calculations exact. Other ids are
/// purged immediately; duplicate slots of the loaded id are handled by
/// `Queue::remove_ids_except_current`.
fn queue_purge_plan(ids: &[i64], loaded: Option<i64>) -> QueuePurgePlan {
    let after_loaded_track = loaded.filter(|id| ids.contains(id));
    let mut immediate = Vec::new();
    for id in ids.iter().copied() {
        if Some(id) != after_loaded_track && !immediate.contains(&id) {
            immediate.push(id);
        }
    }
    QueuePurgePlan {
        immediate,
        after_loaded_track,
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

fn move_rows_to_front(
    context: &mut Queue,
    pending: &mut UpNextQueue,
    rows: &[crate::ui::track_list::queue_row_mapping::QueueRow],
) -> usize {
    use crate::ui::track_list::queue_row_mapping::QueueRow;

    let base = context.current_order_position();
    let mut ids = Vec::new();
    let mut play_next_positions = Vec::new();
    let mut snapshot_positions = Vec::new();
    for row in rows {
        match *row {
            QueueRow::PlayNext(position) => {
                if let Some(id) = pending.ids().get(position).copied() {
                    ids.push(id);
                    play_next_positions.push(position);
                }
            }
            QueueRow::UpNext(offset) => {
                let Some(position) = base.map(|base| base + 1 + offset) else {
                    continue;
                };
                if let Some(id) = context.id_at_order_position(position) {
                    ids.push(id);
                    snapshot_positions.push(position);
                }
            }
            QueueRow::NowPlaying => {}
        }
    }

    pending.remove_positions(&play_next_positions);
    context.remove_order_positions(&snapshot_positions);
    pending.prepend(&ids);
    ids.len()
}

fn apply_queue_reorder(
    context: &mut Queue,
    manual: &mut UpNextQueue,
    op: crate::ui::track_list::queue_row_mapping::QueueReorderOp,
) -> bool {
    use crate::ui::track_list::queue_row_mapping::QueueReorderOp;

    match op {
        QueueReorderOp::WithinPlayNext { from, to } => manual.move_item(from, to),
        QueueReorderOp::PromoteUpNext {
            up_next_offset,
            insert_at,
        } => {
            let Some(base) = context.current_order_position() else {
                return false;
            };
            let position = base + 1 + up_next_offset;
            let Some(id) = context.id_at_order_position(position) else {
                return false;
            };
            context.remove_order_positions(&[position]);
            manual.insert(insert_at, id);
            true
        }
    }
}

impl PlayerController {
    pub(in crate::ui) fn add_on_queue_changed(&self, callback: impl Fn() + 'static) {
        self.queue_changed.borrow_mut().push(Rc::new(callback));
    }

    /// Returns every live playback-model id rejected by the core retention
    /// predicate after a scan. The caller feeds these ids into the same
    /// purge path as hard deletes and auto-clean.
    pub(in crate::ui) fn scan_queue_purge_ids(&self) -> Vec<i64> {
        let mut candidates = self.queue.borrow().ids_in_order();
        candidates.extend_from_slice(self.up_next.borrow().ids());
        if let Some(id) = self.current_up_next.get() {
            candidates.push(id);
        }
        let result = {
            let conn = self.conn.borrow();
            reprise_core::queries::query_queue_purge_track_ids(&conn, &candidates)
        };
        match result {
            Ok(ids) => ids,
            Err(error) => {
                tracing::warn!(%error, "could not reconcile scan-detected queue removals");
                Vec::new()
            }
        }
    }

    pub(in crate::ui) fn notify_queue_changed(&self) {
        tracing::info!(up_next_len = self.up_next.borrow().len(), "up next changed");
        self.update_agent_queue_mirror();
        let callbacks = self.queue_changed.borrow().clone();
        for callback in callbacks {
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
                    let playable = {
                        let conn = self.conn.borrow();
                        reprise_core::queries::query_live_track_ids(&conn)
                            .map(|ids| ids.contains(&id))
                    };
                    match playable {
                        Ok(true) => self.play_track_id_with_change(
                            id,
                            crate::ui::current_track_selection::CurrentTrackChange::ExplicitTransport,
                        ),
                        Ok(false) => self.advance_playback(AdvanceReason::Manual),
                        Err(error) => {
                            tracing::error!(%error, id, "could not validate restored current track; trying it directly");
                            self.play_track_id_with_change(
                                id,
                                crate::ui::current_track_selection::CurrentTrackChange::ExplicitTransport,
                            );
                        }
                    }
                }
            }
            ToggleAction::StartPending => self.advance_playback(AdvanceReason::Manual),
            ToggleAction::TogglePipeline => {
                if let Err(error) = self.player.toggle_pause() {
                    tracing::error!(%error, "toggle play/pause failed");
                } else if status == reprise_core::media_integration::MprisPlaybackStatus::Paused {
                    self.notify_current_track(
                        crate::ui::current_track_selection::CurrentTrackChange::ExplicitTransport,
                    );
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
        self.deferred_queue_purge_id.set(None);

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
    /// track, pending manual entries, and virtual metadata for the snapshot's
    /// play-order tail. The tail provider clones only requested windows.
    pub(in crate::ui) fn queue_view_model(self: &Rc<Self>) -> QueueViewModel {
        let deferred = self.deferred_queue_purge_id.get();
        let now_playing = self
            .now_playing
            .borrow()
            .as_ref()
            .map(|np| np.id)
            .filter(|id| Some(*id) != deferred);
        let play_next = self.up_next.borrow().ids().to_vec();
        let context_count = self.queue.borrow().remaining_len();
        let origin_label = self
            .play_origin
            .borrow()
            .as_ref()
            .map(|origin| origin.label.clone());
        let player = Rc::downgrade(self);
        let context = (context_count > 0).then(|| {
            VirtualContextTail::new(
                context_count,
                Rc::new(move |offset, limit| {
                    player.upgrade().map_or_else(Vec::new, |player| {
                        player.queue.borrow().remaining_window(offset, limit)
                    })
                }),
            )
        });
        compose_virtual(now_playing, &play_next, context, origin_label.as_deref())
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

    /// Moves selected composite Queue rows to the start of Play Next in
    /// selection order. Snapshot rows are promoted out of their context;
    /// Now Playing is intentionally skipped.
    pub(in crate::ui) fn move_queue_rows_to_top(
        &self,
        rows: &[crate::ui::track_list::queue_row_mapping::QueueRow],
    ) -> usize {
        let moved = {
            let mut context = self.queue.borrow_mut();
            let mut pending = self.up_next.borrow_mut();
            move_rows_to_front(&mut context, &mut pending, rows)
        };
        if moved > 0 {
            self.notify_queue_changed();
        }
        moved
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
                    Some(id) => self.play_track_id_with_change(
                        id,
                        crate::ui::current_track_selection::CurrentTrackChange::ExplicitTransport,
                    ),
                    None => self.reset_to_stopped(),
                }
            }
        }

        if removed > 0 {
            self.notify_queue_changed();
        }
        removed
    }

    /// QUE-8 drag semantics: reorder within Play Next or promote one Up Next
    /// snapshot row into it (removed from the snapshot so it cannot play
    /// twice). The virtual context is never reordered in place.
    pub(in crate::ui) fn reorder_queue_rows(
        &self,
        op: crate::ui::track_list::queue_row_mapping::QueueReorderOp,
    ) -> bool {
        let moved = {
            let mut context = self.queue.borrow_mut();
            let mut manual = self.up_next.borrow_mut();
            apply_queue_reorder(&mut context, &mut manual, op)
        };
        if moved {
            self.notify_queue_changed();
        }
        moved
    }

    /// QUE-3 double-click on a queue row: start that track now — no context
    /// rebuild. A Play Next row drains the manual line through it
    /// (`play_up_next_at`); an Up Next row *promotes* the track to play now
    /// while keeping every track it passed upcoming, in order
    /// (`Queue::play_order_position_now`) — so a click never drops the rest of
    /// the queue out of view; the Now Playing row restarts itself.
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
                        // QUE double-click keeps the rest of the queue: the
                        // clicked track jumps the line to play now, every track
                        // it passed stays upcoming in order (see
                        // `Queue::play_order_position_now`) — NOT
                        // `jump_to_order_position`, which fast-forwards past
                        // them and drops them out of the forward tail.
                        Some(base) => queue.play_order_position_now(base + 1 + offset),
                        None => None,
                    }
                };
                let Some(id) = target else {
                    tracing::warn!(offset, "queue jump target vanished; ignoring");
                    return;
                };
                self.current_up_next.set(None);
                self.notify_queue_changed();
                self.play_track_id_with_change(
                    id,
                    crate::ui::current_track_selection::CurrentTrackChange::ExplicitTransport,
                );
            }
            QueueRow::NowPlaying => {
                let current = self
                    .current_up_next
                    .get()
                    .or_else(|| self.queue.borrow().current());
                if let Some(id) = current {
                    self.play_track_id_with_change(
                        id,
                        crate::ui::current_track_selection::CurrentTrackChange::ExplicitTransport,
                    );
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
    /// A loaded deleted track is intentionally different: its player-owned
    /// metadata and already-open audio continue until a natural or explicit
    /// transport transition. The context retains exactly its current slot as
    /// a tombstone so next/previous and gapless prediction keep their normal
    /// cursor semantics; the Queue view omits that unresolvable row. Every
    /// duplicate/future occurrence is still removed immediately.
    pub(in crate::ui) fn purge_queue_ids(&self, ids: &[i64]) {
        if ids.is_empty() {
            return;
        }
        let playing = self.now_playing.borrow().as_ref().map(|track| track.id);
        let plan = queue_purge_plan(ids, playing);
        let playing_from_up_next = plan.after_loaded_track.is_some()
            && self.current_up_next.get() == plan.after_loaded_track;
        let context_changed = if playing_from_up_next {
            self.queue.borrow_mut().remove_ids(ids)
        } else {
            let mut queue = self.queue.borrow_mut();
            let immediate = queue.remove_ids(&plan.immediate);
            let duplicates = plan
                .after_loaded_track
                .is_some_and(|id| queue.remove_ids_except_current(&[id]));
            immediate || duplicates
        };
        let pending_changed = self.up_next.borrow_mut().remove_ids(ids);
        if self
            .current_up_next
            .get()
            .is_some_and(|id| ids.contains(&id))
            && !playing_from_up_next
        {
            self.current_up_next.set(None);
        }
        if let Some(id) = plan.after_loaded_track {
            self.deferred_queue_purge_id.set(Some(id));
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
        } else if plan.after_loaded_track.is_some() {
            // The Queue projection changed even when there were no future
            // entries to remove: its dead Now Playing row is now omitted.
            self.notify_queue_changed();
        }
        if let Some(id) = plan.after_loaded_track {
            tracing::info!(
                deleted = id,
                "loaded track left playing from its owned snapshot after catalog deletion"
            );
        }
    }
}

#[cfg(test)]
#[path = "queue_transport_tests.rs"]
mod tests;
