//! Typed manual-queue reconciliation tests for POD-21.

use reprise_core::up_next::QueueItem;

use super::*;

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
