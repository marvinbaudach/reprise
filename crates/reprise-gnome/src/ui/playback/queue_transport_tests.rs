//! Tests for queue_transport.rs (extracted to keep the source under the 800-line gate).

use super::*;

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
fn que_8_drag_from_continuing_materialises_one_entry() {
    let mut context = context(&[1, 2, 3, 4], 0);
    let mut manual = pending(&[10, 11]);

    let moved = apply_queue_reorder(
        &mut context,
        &mut manual,
        crate::ui::track_list::queue_row_mapping::QueueReorderOp::PromoteUpNext {
            up_next_offset: 1,
            insert_at: 0,
        },
    );

    assert!(moved);
    assert_eq!(manual.ids(), &[3, 10, 11]);
    assert_eq!(context.remaining_after_current(), [2, 4]);
}

#[test]
fn browse_8_loaded_deleted_track_is_deferred_while_future_entries_are_purged() {
    let plan = queue_purge_plan(&[10, 20, 10, 30], Some(10));

    assert_eq!(plan.immediate, vec![20, 30]);
    assert_eq!(plan.after_loaded_track, Some(10));
}

#[test]
fn queue_purge_without_a_loaded_deleted_track_is_immediate() {
    let plan = queue_purge_plan(&[20, 30], Some(10));

    assert_eq!(plan.immediate, vec![20, 30]);
    assert_eq!(plan.after_loaded_track, None);
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

#[test]
fn move_to_top_promotes_up_next_row_to_front_of_play_next() {
    use crate::ui::track_list::queue_row_mapping::QueueRow;

    let mut context = context(&[10, 20, 30, 40], 0);
    let mut pending = pending(&[101, 102]);
    let moved = move_rows_to_front(&mut context, &mut pending, &[QueueRow::UpNext(2)]);

    assert_eq!(moved, 1);
    assert_eq!(pending.ids(), &[40, 101, 102]);
    assert_eq!(context.remaining_after_current(), [20, 30]);
}

#[test]
fn move_to_top_reorders_play_next_and_skips_now_playing() {
    use crate::ui::track_list::queue_row_mapping::QueueRow;

    let mut context = context(&[10, 20], 0);
    let mut pending = pending(&[101, 102, 103]);
    let moved = move_rows_to_front(
        &mut context,
        &mut pending,
        &[QueueRow::NowPlaying, QueueRow::PlayNext(2)],
    );

    assert_eq!(moved, 1);
    assert_eq!(pending.ids(), &[103, 101, 102]);
}

#[test]
fn move_to_top_preserves_multi_row_selection_order() {
    use crate::ui::track_list::queue_row_mapping::QueueRow;

    let mut context = context(&[10, 20, 30, 40], 0);
    let mut pending = pending(&[101, 102]);
    // UpNext(1) resolves to snapshot id 30; PlayNext(0) is pending id 101.
    let moved = move_rows_to_front(
        &mut context,
        &mut pending,
        &[QueueRow::UpNext(1), QueueRow::PlayNext(0)],
    );

    assert_eq!(moved, 2);
    // The two moved ids lead in selection order, then the survivor (102).
    assert_eq!(pending.ids(), &[30, 101, 102]);
    // The promoted snapshot row (30) left the context so it can't play
    // twice; the untouched snapshot rows keep their order.
    assert_eq!(context.remaining_after_current(), [20, 40]);
}

#[test]
fn move_to_top_without_current_track_is_a_noop() {
    use crate::ui::track_list::queue_row_mapping::QueueRow;

    // No current track -> `current_order_position` is None, so an Up Next
    // row can't resolve to a snapshot position.
    let mut context = Queue::new();
    let mut pending = pending(&[101, 102]);
    let moved = move_rows_to_front(&mut context, &mut pending, &[QueueRow::UpNext(0)]);

    assert_eq!(moved, 0);
    assert_eq!(pending.ids(), &[101, 102]);
    assert!(context.remaining_after_current().is_empty());

    // An empty selection is a no-op regardless of state.
    assert_eq!(move_rows_to_front(&mut context, &mut pending, &[]), 0);
    assert_eq!(pending.ids(), &[101, 102]);
}
