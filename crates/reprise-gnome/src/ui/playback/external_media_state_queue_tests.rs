//! Typed manual-queue reconciliation tests for POD-21.

use reprise_core::up_next::QueueItem;

use super::*;

#[test]
fn pod_21_neighbours_follow_the_frozen_rendered_order_without_wrapping() {
    let mut rendered_ids = vec![11, 22, 33];
    let middle = NeighbourContext::for_episode(&rendered_ids, 22).unwrap();

    assert_eq!(
        middle.previous().map(|context| context.current_id()),
        Some(11)
    );
    assert_eq!(middle.next().map(|context| context.current_id()), Some(33));
    assert!(NeighbourContext::for_episode(&rendered_ids, 11)
        .unwrap()
        .previous()
        .is_none());
    assert!(NeighbourContext::for_episode(&rendered_ids, 33)
        .unwrap()
        .next()
        .is_none());

    rendered_ids.clear();
    assert_eq!(
        middle.previous().map(|context| context.current_id()),
        Some(11)
    );
    assert_eq!(middle.next().map(|context| context.current_id()), Some(33));
}

#[test]
fn pod_21_manual_queue_neighbours_preserve_typed_rows_and_colliding_ids() {
    let pending = [QueueItem::Track(7), QueueItem::Episode(9)];
    let middle = NeighbourContext::for_manual_queue(QueueItem::Episode(7), &pending).unwrap();

    assert_eq!(middle.current_item(), QueueItem::Episode(7));
    assert_eq!(
        middle.previous().map(|context| context.current_item()),
        None
    );
    assert_eq!(
        middle.next().map(|context| context.current_item()),
        Some(QueueItem::Track(7))
    );
    assert_eq!(
        middle
            .next()
            .and_then(|context| context.next())
            .map(|context| context.current_item()),
        Some(QueueItem::Episode(9))
    );
}

#[test]
fn direct_episode_context_jump_keeps_the_frozen_sequence() {
    let context = NeighbourContext::for_episode(&[7, 8, 9], 7).unwrap();
    let sequence = context.sequence;

    let target = context.upcoming_context(1).unwrap();

    assert_eq!(target.current_item(), QueueItem::Episode(9));
    assert_eq!(target.position(), 2);
    assert_eq!(target.sequence, sequence);
    assert!(context.upcoming_context(2).is_none());
}

#[test]
fn pod_21_manual_queue_skips_only_after_a_direct_episode_failure() {
    assert!(should_skip_manual_queue_after_failure(
        PodcastOrigin::ManualQueue,
        &PodcastFailureAction::Direct,
    ));
    assert!(!should_skip_manual_queue_after_failure(
        PodcastOrigin::ManualQueue,
        &PodcastFailureAction::Automatic(AdvanceFailure::Stop),
    ));
    assert!(!should_skip_manual_queue_after_failure(
        PodcastOrigin::Direct,
        &PodcastFailureAction::Direct,
    ));
}
