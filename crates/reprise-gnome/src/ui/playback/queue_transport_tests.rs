//! Tests for queue_transport.rs (extracted to keep the source under the 800-line gate).

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
