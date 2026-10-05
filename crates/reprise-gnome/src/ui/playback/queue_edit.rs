use std::rc::Rc;

use crate::ui::player_controller::PlayerController;
use reprise_core::queue::Queue;
use reprise_core::up_next::{QueueItem, UpNextQueue};

use super::queue_transport as projection;

pub(super) fn remove_direct_episode_now_playing(
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

pub(super) fn move_rows_to_front(
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
pub(super) fn apply_queue_reorder(
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
}
