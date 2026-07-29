use super::*;

#[test]
fn remove_ids_on_empty_queue_is_a_no_op() {
    let mut q = Queue::new();
    assert!(!q.remove_ids(&[1, 2]));
    assert!(q.is_empty());
}

#[test]
fn remove_ids_empty_slice_is_a_no_op() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 0);
    assert!(!q.remove_ids(&[]));
    assert_eq!(q.ids_in_order(), vec![10, 20, 30]);
}

#[test]
fn remove_ids_matching_nothing_is_a_no_op() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 1);
    assert!(!q.remove_ids(&[999]));
    assert_eq!(q.ids_in_order(), vec![10, 20, 30]);
    assert_eq!(q.current(), Some(20));
}

#[test]
fn remove_ids_removes_a_non_current_track_and_stays_gapless() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30, 40], 1); // current = 20
    assert!(q.remove_ids(&[30]));
    assert_eq!(q.ids_in_order(), vec![10, 20, 40]);
    assert_eq!(q.current(), Some(20), "untouched current track survives");
    assert_eq!(q.len(), 3);
}

#[test]
fn remove_ids_removing_the_current_track_advances_to_the_next_surviving_track() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30, 40], 1); // current = 20
    assert!(q.remove_ids(&[20]));
    assert_eq!(q.ids_in_order(), vec![10, 30, 40]);
    assert_eq!(
        q.current(),
        Some(30),
        "advances to the next surviving track, never backward"
    );
}

#[test]
fn remove_ids_removing_the_current_track_when_it_is_last_yields_none() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30], 2); // current = 30, last track
    assert!(q.remove_ids(&[30]));
    assert_eq!(q.ids_in_order(), vec![10, 20]);
    assert_eq!(
        q.current(),
        None,
        "no surviving track after the removed current track"
    );
}

#[test]
fn remove_ids_removing_current_skips_over_multiple_removed_tracks_ahead() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30, 40, 50], 1); // current = 20
    assert!(q.remove_ids(&[20, 30, 40]));
    assert_eq!(q.ids_in_order(), vec![10, 50]);
    assert_eq!(q.current(), Some(50));
}

#[test]
fn remove_ids_removing_every_track_empties_the_queue() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20], 0);
    assert!(q.remove_ids(&[10, 20]));
    assert!(q.is_empty());
    assert_eq!(q.current(), None);
}

#[test]
fn remove_ids_removes_every_occurrence_of_a_duplicated_id() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 10, 30], 0); // id 10 queued twice
    assert!(q.remove_ids(&[10]));
    assert_eq!(
        q.ids_in_order(),
        vec![20, 30],
        "every occurrence of a hard-deleted id must be purged, not just one"
    );
}

#[test]
fn remove_ids_leaves_an_exhausted_queue_exhausted() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20], 0);
    q.set_repeat(Repeat::Off);
    q.advance_auto(); // -> 20
    q.advance_auto(); // -> None (exhausted)
    assert_eq!(q.current(), None);

    assert!(q.remove_ids(&[10]));
    assert_eq!(
        q.current(),
        None,
        "a removal must never resurrect an exhausted queue's position"
    );
    assert_eq!(q.ids_in_order(), vec![20]);
}

#[test]
fn remove_ids_preserves_current_under_shuffle() {
    fastrand::seed(42);
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30, 40, 50], 0);
    q.set_shuffle(true);
    let current_before = q.current().unwrap();
    // Remove a track that is not the current one.
    let victim = q
        .ids_in_order()
        .into_iter()
        .find(|&id| id != current_before)
        .unwrap();

    assert!(q.remove_ids(&[victim]));
    assert_eq!(q.current(), Some(current_before));
    assert_eq!(q.len(), 4);
    assert!(!q.ids_in_order().contains(&victim));
}

#[test]
fn remove_ids_except_current_keeps_only_the_loaded_slot_of_a_deleted_id() {
    let mut queue = Queue::new();
    queue.set_tracks(vec![10, 20, 10, 30, 10], 2);

    assert_eq!(queue.remove_ids_except_current(&[10]), 2);

    assert_eq!(queue.ids_in_order(), vec![20, 10, 30]);
    assert_eq!(queue.current(), Some(10));
    assert_eq!(queue.advance_auto(), Some(30));
}
