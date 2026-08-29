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

use crate::ui::current_track_selection::CurrentTrackChange;
use crate::ui::player_controller::PlayerController;
use crate::ui::up_next_transport::AdvanceReason;
use reprise_core::db::Db;
use reprise_core::media_integration::MprisPlaybackStatus;
use reprise_core::queue::Queue;
use reprise_core::up_next::{QueueItem, UpNextQueue};

use super::queue_insertion::track_items;

#[path = "queue_transport_projection.rs"]
mod projection;

#[path = "queue_context_window.rs"]
mod queue_context_window;
pub(in crate::ui) use queue_context_window::QueueContextWindow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToggleAction {
    /// Carries the reveal the loaded track earns when it starts — the one
    /// decision that separates a cold start from every later Play (START-3).
    StartCurrent(CurrentTrackChange),
    StartPending,
    StartRandom,
    TogglePipeline,
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

fn should_advance_after_user_delete(ids: &[i64], loaded: Option<i64>) -> bool {
    loaded.is_some_and(|id| ids.contains(&id))
}

fn remove_direct_episode_now_playing(
    direct_episode: bool,
    rows: &[crate::ui::track_list::queue_row_mapping::QueueRow],
    stop: impl FnOnce(),
) -> usize {
    use crate::ui::track_list::queue_row_mapping::QueueRow;

    if direct_episode && rows.contains(&QueueRow::NowPlaying) {
        stop();
        1
    } else {
        0
    }
}

/// `restored_placement_intact` says the loaded track is still exactly where a
/// normal start put it: selected and centered, never played (START-3). Its
/// first Play only starts the audio, because the viewport is already the one
/// the reveal would scroll to — and a glide onto the value the list already
/// holds is the second visible centering this bug report is about. Every other
/// start from Stopped keeps NAV-10b's explicit-transport reveal.
fn toggle_action(
    status: MprisPlaybackStatus,
    current_track: Option<QueueItem>,
    has_pending: bool,
    restored_placement_intact: bool,
) -> ToggleAction {
    match (status, current_track, has_pending) {
        (MprisPlaybackStatus::Stopped, Some(_), _) => {
            ToggleAction::StartCurrent(restored_start_change(restored_placement_intact))
        }
        (MprisPlaybackStatus::Stopped, None, true) => ToggleAction::StartPending,
        (MprisPlaybackStatus::Stopped, None, false) => ToggleAction::StartRandom,
        (MprisPlaybackStatus::Playing | MprisPlaybackStatus::Paused, _, _) => {
            ToggleAction::TogglePipeline
        }
    }
}

pub(super) fn restored_start_change(
    restored_placement_intact: bool,
) -> CurrentTrackChange {
    if restored_placement_intact {
        CurrentTrackChange::PlaybackStarted
    } else {
        CurrentTrackChange::ExplicitTransport
    }
}

pub(super) fn initial_library_availability(db: &Db) -> bool {
    reprise_core::queries::query_has_live_tracks(db)
        .inspect_err(
            |error| tracing::warn!(%error, "could not determine idle playback availability"),
        )
        .unwrap_or(false)
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
                    ids.push(QueueItem::Track(id));
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
            manual.insert(insert_at, QueueItem::Track(id));
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
        candidates.extend(
            self.up_next
                .borrow()
                .ids()
                .iter()
                .filter_map(|item| item.track_id()),
        );
        if let Some(id) = self.current_up_next.get().and_then(QueueItem::track_id) {
            candidates.push(id);
        }
        let result = {
            let conn = &self.conn;
            reprise_core::queries::query_queue_purge_track_ids(conn, &candidates)
        };
        match result {
            Ok(ids) => ids,
            Err(error) => {
                tracing::warn!(%error, "could not reconcile scan-detected queue removals");
                Vec::new()
            }
        }
    }

    pub(in crate::ui) fn purge_unavailable_episodes(&self) -> usize {
        let available = match reprise_core::queries::query_available_episode_ids(&self.conn) {
            Ok(ids) => ids,
            Err(error) => {
                tracing::warn!(%error, "could not reconcile unsubscribed queued episodes");
                return 0;
            }
        };
        let unavailable = self
            .up_next
            .borrow()
            .ids()
            .iter()
            .copied()
            .filter(|item| item.episode_id().is_some_and(|id| !available.contains(&id)))
            .collect::<Vec<_>>();
        let removed = self.up_next.borrow_mut().remove_ids(&unavailable);
        let current_removed = self
            .current_up_next
            .get()
            .is_some_and(|item| item.episode_id().is_some_and(|id| !available.contains(&id)));
        if current_removed {
            self.current_up_next.set(None);
        }
        let changed = removed + usize::from(current_removed);
        if changed > 0 {
            self.notify_queue_changed();
        }
        changed
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

    pub(in crate::ui) fn start_current_item(
        self: &Rc<Self>,
        item: QueueItem,
        change: CurrentTrackChange,
    ) {
        let id = item.id();
        let playable = match item {
            QueueItem::Track(id) => reprise_core::queries::query_live_track_ids(&self.conn)
                .map(|ids| ids.contains(&id)),
            QueueItem::Episode(id) => reprise_core::queries::query_available_episode_ids(&self.conn)
                .map(|ids| ids.contains(&id)),
        };
        match playable {
            Ok(true) => self.present_queue_item(
                item,
                crate::ui::player_controller::StartPlayback::Yes,
                change,
            ),
            Ok(false) => self.advance_playback(AdvanceReason::Manual),
            Err(error) => {
                tracing::error!(%error, id, "could not validate restored current track; trying it directly");
                self.present_queue_item(
                    item,
                    crate::ui::player_controller::StartPlayback::Yes,
                    change,
                );
            }
        }
    }

    /// Starts the restored queue's current track while stopped; otherwise
    /// toggles the already-loaded pipeline. Shared by the bar, Space, and
    /// MPRIS PlayPause, without ever introducing startup autoplay.
    pub(in crate::ui) fn toggle_pause(self: &Rc<Self>) {
        if self.toggle_external_pause() {
            return;
        }
        let status = self
            .mpris_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status;
        let current = self
            .current_up_next
            .get()
            .or_else(|| self.queue.borrow().current().map(QueueItem::Track));
        let has_pending = !self.up_next.borrow().is_empty();
        match toggle_action(
            status,
            current,
            has_pending,
            self.restored_placement_intact.get(),
        ) {
            ToggleAction::StartCurrent(change) => {
                if let Some(item) = current {
                    self.start_current_item(item, change);
                }
            }
            ToggleAction::StartPending => self.advance_playback(AdvanceReason::Manual),
            ToggleAction::StartRandom => {
                let snapshot = {
                    let conn = &self.conn;
                    reprise_core::queries::query_random_live_track_ids(conn)
                };
                match snapshot {
                    Ok(ids) if ids.is_empty() => {
                        self.library_has_tracks.set(false);
                        self.sync_transport_enabled(false);
                        tracing::debug!("play/pause: library is empty; nothing to play");
                    }
                    Ok(ids) => {
                        self.library_has_tracks.set(true);
                        self.play_from_view(ids, 0, super::play_origin::PlayOrigin::library());
                    }
                    Err(error) => {
                        tracing::error!(%error, "could not build random library playback snapshot");
                    }
                }
            }
            ToggleAction::TogglePipeline => {
                if let Err(error) = self.player.toggle_pause() {
                    tracing::error!(%error, "toggle play/pause failed");
                } else if status == reprise_core::media_integration::MprisPlaybackStatus::Paused {
                    self.notify_current_track(
                        crate::ui::current_track_selection::CurrentTrackChange::ExplicitTransport,
                    );
                }
            }
        }
    }

    /// Refreshes whether the idle Play action can seed a library snapshot.
    /// Track-list reloads call this after scans and library mutations.
    pub(in crate::ui) fn refresh_library_availability(&self) {
        let available = {
            let conn = &self.conn;
            reprise_core::queries::query_has_live_tracks(conn)
        };
        let available = match available {
            Ok(available) => available,
            Err(error) => {
                tracing::warn!(%error, "could not refresh idle playback availability");
                return;
            }
        };
        self.library_has_tracks.set(available);
        let queue_has_tracks = self.current_up_next.get().is_some()
            || !self.queue.borrow().is_empty()
            || !self.up_next.borrow().is_empty();
        self.sync_transport_enabled(queue_has_tracks);
    }

    /// PLAY-14 Previous follows playback history in every mode. Episode
    /// neighbour priority is handled by `transport_previous`; reaching this
    /// method means history is the answer.
    pub(in crate::ui) fn previous(self: &Rc<Self>) {
        self.previous_with_up_next();
    }

    /// Steps the queue to the next track and plays it (or resets to stopped
    /// if there is none) — shared by the bar's next button and MPRIS's
    /// `Next` method. Same borrow discipline as `previous`.
    pub(in crate::ui) fn next(self: &Rc<Self>) {
        if self.forward_from_history() {
            return;
        }
        if self.playback_mode() != super::preview::PlaybackMode::Queue {
            return;
        }
        self.advance_playback(AdvanceReason::Manual);
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
        self: &Rc<Self>,
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

    /// Moves selected composite Queue rows to the start of Play Next in
    /// selection order. Snapshot rows are promoted out of their context;
    /// Now Playing is intentionally skipped.
    pub(in crate::ui) fn move_queue_rows_to_top(
        &self,
        rows: &[crate::ui::track_list::queue_row_mapping::QueueRow],
    ) -> usize {
        use crate::ui::track_list::queue_row_mapping::QueueRow;

        let direct_episode = projection::has_direct_episode_projection(&self.external.borrow());
        let editable_rows = rows
            .iter()
            .copied()
            .filter(|row| !direct_episode || matches!(row, QueueRow::PlayNext(_)))
            .collect::<Vec<_>>();
        if editable_rows.len() != rows.len() {
            tracing::debug!(
                ignored = rows.len() - editable_rows.len(),
                "episode context rows cannot move to Play Next; ignoring"
            );
        }
        let moved = {
            let mut context = self.queue.borrow_mut();
            let mut pending = self.up_next.borrow_mut();
            move_rows_to_front(&mut context, &mut pending, &editable_rows)
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
        self: &Rc<Self>,
        rows: &[crate::ui::track_list::queue_row_mapping::QueueRow],
    ) -> usize {
        use crate::ui::track_list::queue_row_mapping::QueueRow;

        let direct_episode = projection::has_direct_episode_projection(&self.external.borrow());
        let direct_episode_removed =
            remove_direct_episode_now_playing(direct_episode, rows, || self.stop_external());
        let mut play_next_indices = Vec::new();
        let mut up_next_offsets = Vec::new();
        let mut remove_current = false;
        let mut ignored = 0;
        for row in rows {
            match row {
                QueueRow::PlayNext(index) => play_next_indices.push(*index),
                QueueRow::UpNext(offset) if !direct_episode => up_next_offsets.push(*offset),
                QueueRow::NowPlaying if !direct_episode => remove_current = true,
                QueueRow::NowPlaying => {}
                QueueRow::UpNext(_) => ignored += 1,
            }
        }
        if ignored > 0 {
            tracing::debug!(ignored, "episode context rows cannot be removed; ignoring");
        }

        let mut removed = direct_episode_removed;
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
                        queue.remove_order_positions(&positions) > 0
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
        } else if !rows.is_empty() {
            tracing::warn!(
                requested = rows.len(),
                ?rows,
                "queue row removal removed no entries"
            );
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
        if projection::has_direct_episode_projection(&self.external.borrow())
            && matches!(
                op,
                crate::ui::track_list::queue_row_mapping::QueueReorderOp::PromoteUpNext { .. }
            )
        {
            tracing::debug!("episode context rows cannot be reordered; ignoring");
            return false;
        }
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
        self: &Rc<Self>,
        row: crate::ui::track_list::queue_row_mapping::QueueRow,
    ) {
        use crate::ui::track_list::queue_row_mapping::QueueRow;

        match row {
            QueueRow::PlayNext(index) => self.play_up_next_at(index),
            QueueRow::UpNext(offset) => {
                if self.jump_to_direct_episode_context(offset) {
                    return;
                }
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
                if projection::has_direct_episode_projection(&self.external.borrow()) {
                    tracing::debug!("direct episode Now Playing row cannot mutate music; ignoring");
                    return;
                }
                let current = self
                    .current_up_next
                    .get()
                    .or_else(|| self.queue.borrow().current().map(QueueItem::Track));
                if let Some(item) = current {
                    self.present_queue_item(
                        item,
                        crate::ui::player_controller::StartPlayback::Yes,
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
            && self.current_up_next.get().and_then(QueueItem::track_id) == plan.after_loaded_track;
        let context_changed = if playing_from_up_next {
            self.queue.borrow_mut().remove_ids(ids)
        } else {
            let mut queue = self.queue.borrow_mut();
            let immediate = queue.remove_ids(&plan.immediate);
            let duplicates = plan
                .after_loaded_track
                .is_some_and(|id| queue.remove_ids_except_current(&[id]) > 0);
            immediate || duplicates
        };
        let items = track_items(ids);
        let pending_changed = self.up_next.borrow_mut().remove_ids(&items) > 0;
        if self
            .current_up_next
            .get()
            .and_then(QueueItem::track_id)
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

    /// Explicit Remove/Trash is a transport action when it deleted the
    /// loaded track. Background purge callers intentionally use only
    /// `purge_queue_ids`, preserving PLAY-5a/PLAY-5b's no-interruption rule.
    pub(in crate::ui) fn advance_after_user_catalog_delete(self: &Rc<Self>, ids: &[i64]) {
        let loaded = self.now_playing.borrow().as_ref().map(|track| track.id);
        if !should_advance_after_user_delete(ids, loaded) {
            return;
        }
        tracing::info!(deleted = ?loaded, "user deleted the loaded track; advancing playback");
        self.advance_playback(AdvanceReason::Automatic);
    }
}

#[cfg(test)]
#[path = "queue_transport_tests.rs"]
mod tests;
