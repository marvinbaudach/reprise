//! Queue ordering regressions split from `queue_tests.rs` for the code-file size gate.

use super::*;

// Stage-3 close-out: `remove_ids` (hard-delete queue purge).

// QUE-1: the Queue view's "Up Next" section — everything after the current
// track in play order.

#[test]
fn remaining_after_current_returns_the_play_order_tail() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30, 40], 1);
    assert_eq!(q.remaining_after_current(), vec![30, 40]);
    assert_eq!(q.remaining_len(), 2);
}

#[test]
fn remaining_after_current_matches_shuffled_order() {
    let mut q = Queue::new();
    q.set_tracks(vec![1, 2, 3, 4, 5, 6, 7, 8], 0);
    q.set_shuffle(true);
    let in_order = q.ids_in_order();
    // Current stays in place on shuffle; the rest must equal the shuffled
    // play order after it — exactly what auto-advance will play (QUE-2).
    assert_eq!(q.remaining_after_current(), in_order[1..].to_vec());
}

#[test]
fn remaining_after_current_is_empty_at_end_or_unseeded() {
    let mut q = Queue::new();
    assert_eq!(q.remaining_after_current(), Vec::<i64>::new());
    assert_eq!(q.remaining_len(), 0);
    q.set_tracks(vec![7, 8], 1);
    assert_eq!(q.remaining_after_current(), Vec::<i64>::new());
    assert_eq!(q.remaining_len(), 0);
}

#[test]
fn remaining_window_reads_only_the_requested_context_slice() {
    let mut queue = Queue::new();
    queue.set_tracks(vec![10, 20, 30, 40, 50], 1);

    assert_eq!(queue.remaining_window(1, 2), vec![40, 50]);
    assert!(queue.remaining_window(3, 2).is_empty());
}

// QUE-3: single-occurrence removal by order position + playhead jumps.

#[test]
fn remove_order_positions_removes_single_occurrences_in_play_order() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 10, 30], 0);
    // Remove the SECOND occurrence of 10 (order position 2) only.
    assert_eq!(q.remove_order_positions(&[2]), 1);
    assert_eq!(q.ids_in_order(), vec![10, 20, 30]);
    assert_eq!(q.current(), Some(10));
}

#[test]
fn remove_order_positions_on_current_advances_forward() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 1);
    assert_eq!(q.remove_order_positions(&[1]), 1);
    assert_eq!(q.current(), Some(30));
    assert_eq!(q.ids_in_order(), vec![10, 30]);
}

#[test]
fn remove_order_positions_ignores_out_of_range_and_reports_no_change() {
    let mut q = Queue::new();
    q.set_tracks(vec![10], 0);
    assert_eq!(q.remove_order_positions(&[5]), 0);
    assert_eq!(q.ids_in_order(), vec![10]);
}

#[test]
fn jump_to_order_position_moves_the_playhead_without_rebuilding() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30, 40], 0);
    assert_eq!(q.jump_to_order_position(2), Some(30));
    assert_eq!(q.current(), Some(30));
    // The context is untouched — advancing continues from the new spot.
    assert_eq!(q.advance_auto(), Some(40));
    assert_eq!(q.jump_to_order_position(9), None);
}

#[test]
fn play_order_position_now_promotes_the_target_and_keeps_the_rest_upcoming() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30, 40, 50], 0); // current 10 (pos 0)

    // Double-click the 4th upcoming track (id 40, order position 3): it jumps
    // the line to play now, and the tracks it passed stay queued in order.
    assert_eq!(q.play_order_position_now(3), Some(40));
    assert_eq!(q.current(), Some(40));
    // After 40 comes 20, 30 (the passed tracks), then 50 — nothing skipped.
    assert_eq!(q.remaining_after_current(), vec![20, 30, 50]);
    // The full queue still holds every track (10 is now behind the playhead).
    assert_eq!(q.ids_in_order(), vec![10, 40, 20, 30, 50]);
    assert_eq!(q.len(), 5);
}

#[test]
fn play_order_position_now_on_the_immediate_next_needs_no_reorder() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 0); // current 10
    assert_eq!(q.play_order_position_now(1), Some(20)); // 20 is already next
    assert_eq!(q.current(), Some(20));
    assert_eq!(q.remaining_after_current(), vec![30]);
    assert_eq!(q.ids_in_order(), vec![10, 20, 30]); // order unchanged
}

#[test]
fn play_order_position_now_out_of_range_or_at_end_is_a_noop() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20], 0);
    assert_eq!(q.play_order_position_now(9), None);
    assert_eq!(q.current(), Some(10)); // unchanged
    assert_eq!(q.ids_in_order(), vec![10, 20]);
    // Current track is the last one: no room after it to promote into.
    q.set_tracks(vec![10, 20], 1);
    assert_eq!(q.play_order_position_now(1), None);
    assert_eq!(q.current(), Some(20));
}
