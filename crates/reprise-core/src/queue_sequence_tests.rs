use super::Queue;

#[test]
fn sequence_identity_changes_for_order_mutations_but_not_playhead_moves() {
    let mut queue = Queue::new();
    let empty = queue.sequence_identity();

    queue.set_tracks(vec![10, 20, 30], 0);
    let seeded = queue.sequence_identity();
    assert_ne!(seeded, empty);

    assert_eq!(queue.next_manual(), Some(20));
    assert_eq!(queue.sequence_identity(), seeded);
    assert_eq!(queue.previous(), Some(10));
    assert_eq!(queue.sequence_identity(), seeded);

    queue.append_tracks(&[40]);
    let appended = queue.sequence_identity();
    assert_ne!(appended, seeded);

    assert!(queue.move_item(3, 1));
    assert_ne!(queue.sequence_identity(), appended);
}
