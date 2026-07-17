//! `queue.rs`'s test suite, split into its own file purely to keep
//! `queue.rs` under the project's 800-line rule — `queue.rs` declares this
//! via `#[cfg(test)] #[path = "queue_tests.rs"] mod tests;`, so this
//! file's contents are still the crate-private `crate::queue::tests`
//! module, with the exact same tests, unchanged, that used to live inline
//! (a pure move, not a rewrite).

use super::*;
use std::collections::HashSet;

// Test 1: Empty queue
#[test]
fn test_empty_queue() {
    let q = Queue::new();
    assert!(q.is_empty());
    assert_eq!(q.len(), 0);
    assert_eq!(q.current(), None);
}

#[test]
fn test_empty_queue_operations() {
    let mut q = Queue::new();
    assert_eq!(q.advance_auto(), None);
    assert_eq!(q.next_manual(), None);
    assert_eq!(q.previous(), None);
}

// Test 2: set_tracks with valid start_index
#[test]
fn test_set_tracks_middle() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 1);
    assert_eq!(q.current(), Some(20));
    assert_eq!(q.len(), 3);
}

// Test 3: Linear advance_auto with Repeat::Off
#[test]
fn test_linear_advance_repeat_off() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 0);
    q.set_repeat(Repeat::Off);

    assert_eq!(q.current(), Some(10));
    assert_eq!(q.advance_auto(), Some(20));
    assert_eq!(q.current(), Some(20));
    assert_eq!(q.advance_auto(), Some(30));
    assert_eq!(q.current(), Some(30));
    assert_eq!(q.advance_auto(), None);
    assert_eq!(q.current(), None);
}

// Test 4: Repeat::All wrapping
#[test]
fn test_repeat_all_wrap() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 0);
    q.set_repeat(Repeat::All);

    assert_eq!(q.advance_auto(), Some(20));
    assert_eq!(q.advance_auto(), Some(30));
    assert_eq!(q.advance_auto(), Some(10)); // Wraps to first
    assert_eq!(q.advance_auto(), Some(20));
}

#[test]
fn peek_auto_matches_advance_auto_without_mutating() {
    // Across Off / All / One, peek_auto must predict exactly what advance_auto
    // returns, and must leave the queue position untouched.
    for repeat in [Repeat::Off, Repeat::All, Repeat::One] {
        for start in 0..3 {
            let mut peeker = Queue::new();
            peeker.set_tracks(vec![10, 20, 30], start);
            peeker.set_repeat(repeat);
            let before = peeker.current();

            let predicted = peeker.peek_auto();
            // peek_auto did not move the position.
            assert_eq!(peeker.current(), before, "peek mutated at start={start}");

            let mut advancer = Queue::new();
            advancer.set_tracks(vec![10, 20, 30], start);
            advancer.set_repeat(repeat);
            assert_eq!(
                predicted,
                advancer.advance_auto(),
                "peek != advance at repeat={repeat:?} start={start}"
            );
        }
    }
}

#[test]
fn peek_auto_at_end_without_repeat_is_none() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 2);
    assert_eq!(q.peek_auto(), None);
    assert_eq!(q.current(), Some(30)); // unchanged
}

// Test 5: Repeat::One behavior
#[test]
fn test_repeat_one_advance_auto() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 1);
    q.set_repeat(Repeat::One);

    assert_eq!(q.current(), Some(20));
    assert_eq!(q.advance_auto(), Some(20)); // Same track
    assert_eq!(q.advance_auto(), Some(20)); // Still same
}

#[test]
fn test_repeat_one_next_manual() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 1);
    q.set_repeat(Repeat::One);

    assert_eq!(q.current(), Some(20));
    assert_eq!(q.next_manual(), Some(30)); // Move past One
    assert_eq!(q.current(), Some(30));
    assert_eq!(q.next_manual(), None);
}

// Test 6: previous() behavior
#[test]
fn test_previous_at_first() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 0);

    assert_eq!(q.previous(), Some(10)); // Stay at first
    assert_eq!(q.current(), Some(10));
}

#[test]
fn test_previous_mid_queue() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 2);

    assert_eq!(q.current(), Some(30));
    assert_eq!(q.previous(), Some(20));
    assert_eq!(q.current(), Some(20));
    assert_eq!(q.previous(), Some(10));
    assert_eq!(q.current(), Some(10));
    assert_eq!(q.previous(), Some(10));
}

// Test 7: Shuffle with current track staying current
#[test]
fn test_shuffle_current_stays() {
    fastrand::seed(42);
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 1);

    assert_eq!(q.current(), Some(20));
    q.set_shuffle(true);
    assert_eq!(q.current(), Some(20)); // Current track unchanged
    assert!(q.is_shuffled());
}

#[test]
fn test_shuffle_visits_all_tracks() {
    fastrand::seed(42);
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 0);
    q.set_repeat(Repeat::All);

    q.set_shuffle(true);
    assert!(q.is_shuffled());

    // Collect all tracks via a full pass.
    let mut visited = HashSet::new();
    if let Some(track) = q.current() {
        visited.insert(track);
    }

    for _ in 0..10 {
        if let Some(track) = q.advance_auto() {
            visited.insert(track);
        }
    }

    // All 3 tracks should be visited (and repeating).
    assert_eq!(visited.len(), 3);
    assert!(visited.contains(&10));
    assert!(visited.contains(&20));
    assert!(visited.contains(&30));
}

#[test]
fn test_shuffle_restore_linear_keeps_current() {
    fastrand::seed(42);
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 1);

    assert_eq!(q.current(), Some(20));
    q.set_shuffle(true);
    assert!(q.is_shuffled());
    assert_eq!(q.current(), Some(20));

    q.set_shuffle(false);
    assert!(!q.is_shuffled());
    assert_eq!(q.current(), Some(20)); // Should still be 20
    assert_eq!(q.pos, Some(1)); // Should be at linear index 1
}

/// Stage 3 Task 1 backlog fix: `set_shuffle`'s defensive `pos`-out-of-
/// bounds guard must bail out BEFORE flipping `shuffled`, not after —
/// otherwise an (unreachable under the struct's own invariant, but still
/// worth pinning) early return would leave `shuffled` desynced from the
/// order actually still being linear. Forces the invariant-violating
/// state directly (same-module test, so private fields are reachable)
/// rather than trying to construct it through the public API, which
/// can't produce it at all.
#[test]
fn set_shuffle_guard_failure_does_not_desync_shuffled_flag() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 0);
    q.pos = Some(99); // out of bounds for `order` (len 3)

    q.set_shuffle(true);

    assert!(
        !q.is_shuffled(),
        "guard failure must leave `shuffled` false, matching the order \
         actually still being unshuffled"
    );
}

// Test 8: next_manual at end
#[test]
fn test_next_manual_end_repeat_off() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 2);
    q.set_repeat(Repeat::Off);

    assert_eq!(q.current(), Some(30));
    assert_eq!(q.next_manual(), None);
    assert_eq!(q.current(), None);
}

#[test]
fn test_next_manual_end_repeat_all() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 2);
    q.set_repeat(Repeat::All);

    assert_eq!(q.current(), Some(30));
    assert_eq!(q.next_manual(), Some(10)); // Wraps
    assert_eq!(q.current(), Some(10));
}

// Test 9: set_tracks with out-of-range start_index
#[test]
fn test_set_tracks_out_of_range() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 100);
    assert_eq!(q.current(), Some(30)); // Clamped to last
}

#[test]
fn test_set_tracks_empty() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20], 0);
    assert_eq!(q.len(), 2);

    q.set_tracks(vec![], 0);
    assert!(q.is_empty());
    assert_eq!(q.current(), None);
}

// Additional comprehensive tests
#[test]
fn test_repeat_modes() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 0);

    assert_eq!(q.repeat(), Repeat::Off);
    q.set_repeat(Repeat::All);
    assert_eq!(q.repeat(), Repeat::All);
    q.set_repeat(Repeat::One);
    assert_eq!(q.repeat(), Repeat::One);
}

#[test]
fn test_multiple_shuffles() {
    fastrand::seed(42);
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 0);

    q.set_shuffle(true);
    assert!(q.is_shuffled());
    q.set_shuffle(false);
    assert!(!q.is_shuffled());
    q.set_shuffle(true);
    assert!(q.is_shuffled());
}

#[test]
fn test_default_repeat_is_off() {
    let q = Queue::new();
    assert_eq!(q.repeat(), Repeat::Off);
}

// Stage 3 Task 5: `append_tracks` ("Add to queue" context-menu action).

#[test]
fn append_tracks_to_empty_queue_populates_it_at_index_zero() {
    let mut q = Queue::new();
    q.append_tracks(&[10, 20]);
    assert_eq!(q.len(), 2);
    assert_eq!(q.current(), Some(10));
    assert_eq!(q.ids_in_order(), vec![10, 20]);
}

#[test]
fn append_tracks_appends_to_end_of_linear_order_without_moving_current() {
    let mut q = Queue::new();
    q.set_tracks(vec![1, 2, 3], 1);
    assert_eq!(q.current(), Some(2));

    q.append_tracks(&[4, 5]);

    assert_eq!(q.current(), Some(2), "current track must be unaffected");
    assert_eq!(q.ids_in_order(), vec![1, 2, 3, 4, 5]);
    assert_eq!(q.len(), 5);
}

#[test]
fn append_tracks_appends_to_tail_of_shuffled_order() {
    fastrand::seed(7);
    let mut q = Queue::new();
    q.set_tracks(vec![1, 2, 3], 0);
    q.set_shuffle(true);
    let before = q.ids_in_order();
    let current_before = q.current();

    q.append_tracks(&[100, 200]);

    let after = q.ids_in_order();
    assert_eq!(after.len(), 5);
    // The first 3 entries (the pre-existing shuffled order) are
    // untouched; the two new ids land at the tail, in append order —
    // not woven into the shuffled prefix.
    assert_eq!(&after[..3], &before[..]);
    assert_eq!(&after[3..], &[100, 200]);
    assert_eq!(
        q.current(),
        current_before,
        "current track must be unaffected"
    );
    assert!(q.is_shuffled(), "shuffle state must be unaffected");
}

#[test]
fn append_tracks_empty_slice_is_a_no_op() {
    let mut q = Queue::new();
    q.set_tracks(vec![1, 2], 1);
    assert_eq!(q.current(), Some(2));

    q.append_tracks(&[]);

    assert_eq!(q.len(), 2);
    assert_eq!(q.current(), Some(2));
}

#[test]
fn append_tracks_on_empty_queue_with_empty_slice_stays_empty() {
    let mut q = Queue::new();
    q.append_tracks(&[]);
    assert!(q.is_empty());
    assert_eq!(q.current(), None);
}

#[test]
fn append_tracks_after_exhaustion_does_not_resume_playback_state() {
    // pos == None can also happen mid-life (queue exhausted after
    // Repeat::Off ran out) — appending here must NOT resurrect `pos` to
    // point at the newly appended tail, since `ids` is non-empty
    // already; only the "was truly empty" case forces `pos = Some(0)`.
    let mut q = Queue::new();
    q.set_tracks(vec![1, 2], 0);
    q.set_repeat(Repeat::Off);
    q.advance_auto(); // -> 2
    q.advance_auto(); // -> None (exhausted)
    assert_eq!(q.current(), None);

    q.append_tracks(&[3, 4]);

    assert_eq!(
        q.current(),
        None,
        "an exhausted (but non-empty) queue's position must stay exhausted"
    );
    assert_eq!(q.ids_in_order(), vec![1, 2, 3, 4]);
}

#[test]
fn ids_in_order_matches_linear_order_when_unshuffled() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 0);
    assert_eq!(q.ids_in_order(), vec![10, 20, 30]);
}

#[test]
fn ids_in_order_reflects_shuffle() {
    fastrand::seed(42);
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30, 40, 50], 0);
    q.set_shuffle(true);

    let shuffled = q.ids_in_order();
    assert_eq!(shuffled.len(), 5);
    let mut sorted = shuffled.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, vec![10, 20, 30, 40, 50]);
}

#[test]
fn ids_in_order_is_empty_for_an_empty_queue() {
    let q = Queue::new();
    assert!(q.ids_in_order().is_empty());
}

// Fix 1: Sticky shuffle across set_tracks
#[test]
fn test_shuffle_sticky_across_set_tracks() {
    fastrand::seed(42);
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 1);
    q.set_shuffle(true);
    assert!(q.is_shuffled());

    // set_tracks with shuffle still on should re-shuffle the new order
    // and keep the chosen start_index track as current
    q.set_tracks(vec![100, 200, 300, 400], 2);
    assert!(
        q.is_shuffled(),
        "shuffle should remain active after set_tracks"
    );
    assert_eq!(
        q.current(),
        Some(300),
        "should be at the chosen start_index track"
    );

    // Verify we can do a full pass through all 4 tracks
    q.set_repeat(Repeat::All);
    let mut visited = HashSet::new();
    if let Some(track) = q.current() {
        visited.insert(track);
    }
    // Collect exactly 4 advances (one full cycle of 4 tracks)
    for _ in 0..4 {
        if let Some(track) = q.advance_auto() {
            visited.insert(track);
        }
    }
    assert_eq!(
        visited.len(),
        4,
        "should visit all 4 tracks in exactly one pass"
    );
    assert!(visited.contains(&100));
    assert!(visited.contains(&200));
    assert!(visited.contains(&300));
    assert!(visited.contains(&400));
}

#[test]
fn test_shuffle_sticky_then_disable() {
    fastrand::seed(42);
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 1);
    q.set_shuffle(true);
    assert!(q.is_shuffled());
    assert_eq!(q.current(), Some(20));

    q.set_shuffle(false);
    assert!(!q.is_shuffled());
    assert_eq!(
        q.current(),
        Some(20),
        "current track should be preserved after disabling shuffle"
    );
}

// Fix 2: previous() after exhaustion (pos == None)
#[test]
fn test_previous_after_exhaustion_non_empty() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 0);
    q.set_repeat(Repeat::Off);

    // Advance to the end and exhaust the queue
    q.advance_auto(); // -> 20
    q.advance_auto(); // -> 30
    q.advance_auto(); // -> None (exhausted)
    assert_eq!(q.current(), None);

    // previous() should resume at the LAST track of the current order
    let result = q.previous();
    assert_eq!(
        result,
        Some(30),
        "previous() after exhaustion should resume at the last track"
    );
    assert_eq!(q.current(), Some(30));
}

#[test]
fn test_previous_after_exhaustion_empty_queue() {
    let mut q = Queue::new();
    q.set_tracks(vec![], 0);
    assert_eq!(q.current(), None);

    // previous() on empty queue should return None
    assert_eq!(q.previous(), None);
}

// Fix 5: Strengthen shuffle-visits-all test to check strict property
// Stage 3 Task 6: `move_item` (queue drag-reorder).

#[test]
fn move_item_forward_keeps_current_track_current() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30, 40], 1); // current = 20
    assert_eq!(q.current(), Some(20));

    q.move_item(0, 2); // move 10 from index 0 to index 2
    assert_eq!(q.ids_in_order(), vec![20, 30, 10, 40]);
    assert_eq!(
        q.current(),
        Some(20),
        "current track must stay current even though its index shifted"
    );
}

#[test]
fn move_item_backward_keeps_current_track_current() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30, 40], 3); // current = 40
    assert_eq!(q.current(), Some(40));

    q.move_item(3, 0); // move 40 from index 3 to index 0
    assert_eq!(q.ids_in_order(), vec![40, 10, 20, 30]);
    assert_eq!(q.current(), Some(40));
}

#[test]
fn move_item_of_the_current_track_itself_keeps_it_current() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 1); // current = 20
    q.move_item(1, 2);
    assert_eq!(q.ids_in_order(), vec![10, 30, 20]);
    assert_eq!(
        q.current(),
        Some(20),
        "moving the current track itself must still leave it current"
    );
}

#[test]
fn move_item_same_index_is_a_no_op() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 1);
    q.move_item(1, 1);
    assert_eq!(q.ids_in_order(), vec![10, 20, 30]);
    assert_eq!(q.current(), Some(20));
}

#[test]
fn move_item_out_of_range_from_is_a_no_op() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 0);
    q.move_item(99, 1);
    assert_eq!(q.ids_in_order(), vec![10, 20, 30]);
}

#[test]
fn move_item_out_of_range_to_is_a_no_op() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 0);
    q.move_item(0, 99);
    assert_eq!(q.ids_in_order(), vec![10, 20, 30]);
}

#[test]
fn move_item_on_empty_queue_is_a_no_op() {
    let mut q = Queue::new();
    q.move_item(0, 1);
    assert!(q.is_empty());
    assert_eq!(q.current(), None);
}

#[test]
fn move_item_preserves_current_when_queue_is_exhausted() {
    // pos == None (exhausted, not empty) must stay None after a move
    // that doesn't touch the current-track bookkeeping at all.
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 0);
    q.set_repeat(Repeat::Off);
    q.advance_auto(); // -> 20
    q.advance_auto(); // -> 30
    q.advance_auto(); // -> None (exhausted)
    assert_eq!(q.current(), None);

    q.move_item(0, 2);
    assert_eq!(q.ids_in_order(), vec![20, 30, 10]);
    assert_eq!(q.current(), None, "exhausted queue must stay exhausted");
}

#[test]
fn move_item_reflects_in_shuffled_order_too() {
    fastrand::seed(42);
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30, 40, 50], 0);
    q.set_shuffle(true);
    let current_before = q.current();

    let before = q.ids_in_order();
    q.move_item(0, 4);
    let after = q.ids_in_order();

    assert_eq!(after.len(), 5);
    assert_eq!(after[4], before[0], "the moved track lands at index 4");
    assert_eq!(
        q.current(),
        current_before,
        "current track survives a move under shuffle too"
    );
}

#[test]
fn test_shuffle_visits_all_exactly_once_per_pass() {
    fastrand::seed(42);
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30, 40, 50], 0);
    q.set_repeat(Repeat::All);
    q.set_shuffle(true);

    // Collect ids in a single pass: exactly n advances from current
    let n = 5;
    let mut ids_in_pass = Vec::new();

    if let Some(track) = q.current() {
        ids_in_pass.push(track);
    }

    // Advance exactly n-1 more times for a total of n tracks
    for _ in 0..(n - 1) {
        if let Some(track) = q.advance_auto() {
            ids_in_pass.push(track);
        }
    }

    // The pass should contain exactly n ids, each unique
    assert_eq!(
        ids_in_pass.len(),
        n,
        "should collect exactly {n} ids in one pass"
    );

    // Create the full expected multiset
    let expected: HashSet<i64> = vec![10, 20, 30, 40, 50].into_iter().collect();
    let collected: HashSet<i64> = ids_in_pass.into_iter().collect();
    assert_eq!(
        collected, expected,
        "should visit each track exactly once in a single pass"
    );
}

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

// QUE-3: single-occurrence removal by order position + playhead jumps.

#[test]
fn remove_order_positions_removes_single_occurrences_in_play_order() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 10, 30], 0);
    // Remove the SECOND occurrence of 10 (order position 2) only.
    assert!(q.remove_order_positions(&[2]));
    assert_eq!(q.ids_in_order(), vec![10, 20, 30]);
    assert_eq!(q.current(), Some(10));
}

#[test]
fn remove_order_positions_on_current_advances_forward() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 1);
    assert!(q.remove_order_positions(&[1]));
    assert_eq!(q.current(), Some(30));
    assert_eq!(q.ids_in_order(), vec![10, 30]);
}

#[test]
fn remove_order_positions_ignores_out_of_range_and_reports_no_change() {
    let mut q = Queue::new();
    q.set_tracks(vec![10], 0);
    assert!(!q.remove_order_positions(&[5]));
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
