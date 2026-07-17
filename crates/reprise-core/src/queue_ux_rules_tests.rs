// Rule-named acceptance tests for docs/ux-rules.md. Each [aktiv] rule in
// the rulebook has at least one test here; scripts/check-ux-traceability.sh
// gates the mapping. One primary rule ID per test name.

use super::*;

// UX PLAY-2: double-click plays the row and appends the rest of the visible
// list from that position onto the queue (activation snapshot).
#[test]
fn play_2_activation_snapshot_starts_at_clicked_row() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30, 40], 2);
    assert_eq!(q.current(), Some(30));
    assert_eq!(q.advance_auto(), Some(40));
    assert_eq!(
        q.advance_auto(),
        None,
        "tracks before the clicked row never follow automatically (Repeat::Off)"
    );
}

// UX PLAY-3a: the queue is a snapshot of the filtered hits; shuffle permutes
// exactly those hits (queue = hit set, no track from outside).
#[test]
fn play_3a_shuffle_stays_inside_filtered_snapshot() {
    let mut q = Queue::new();
    let hits = vec![11, 22, 33, 44, 55];
    q.set_tracks(hits.clone(), 0);
    q.set_shuffle(true);
    let mut queue_ids = q.ids_in_order();
    queue_ids.sort_unstable();
    assert_eq!(queue_ids, hits);
    assert_eq!(
        q.current(),
        Some(11),
        "the current track stays put when shuffle is toggled"
    );
}

// UX PLAY-5a: externally deleted tracks leave the queue silently; the
// playing track stays untouched.
#[test]
fn play_5a_deleted_tracks_leave_queue_silently() {
    let mut q = Queue::new();
    q.set_tracks(vec![1, 2, 3, 4], 1);
    assert!(q.remove_ids(&[3]));
    assert_eq!(q.ids_in_order(), vec![1, 2, 4]);
    assert_eq!(
        q.current(),
        Some(2),
        "background removal never stops the playing track"
    );
}

// UX QUE-1 [geplant] — demo of the activation workflow. The three-section
// queue itself shipped on main (c5200e1), but this core stub cannot prove
// the sections; the flip needs a [gtk] test that can. Whoever writes it
// removes the #[ignore] and flips QUE-1 to [aktiv] in the same commit.
#[test]
#[ignore = "UX QUE-1 [geplant] — needs a [gtk] section test; this core stub cannot prove the three sections"]
fn que_1_queue_is_never_empty_while_playing() {
    let mut q = Queue::new();
    q.set_tracks(vec![7, 8, 9], 0);
    assert!(
        !q.is_empty(),
        "while something is playing the queue is never empty"
    );
}
