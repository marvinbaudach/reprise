// Rule-named acceptance tests for docs/ux-rules.md PLAY-14, plus the
// container invariants they depend on.

use super::*;

fn entry(id: i64, pos: usize) -> HistoryEntry {
    HistoryEntry {
        item: QueueItem::Track(id),
        context_pos: Some(pos),
        sequence: (1, 1),
        from_up_next: false,
    }
}

fn queued(id: i64) -> HistoryEntry {
    HistoryEntry {
        item: QueueItem::Track(id),
        context_pos: None,
        sequence: (1, 1),
        from_up_next: true,
    }
}

fn played(entries: &[HistoryEntry]) -> PlaybackHistory {
    let mut history = PlaybackHistory::default();
    for held in entries {
        history.record(*held);
    }
    history
}

#[test]
fn play_14_a_long_played_track_rewinds_to_its_start() {
    let history = played(&[entry(10, 0), entry(20, 1)]);
    assert_eq!(
        resolve_previous(3_001, &history),
        PreviousAction::RestartCurrent
    );
}

#[test]
fn play_14_an_early_press_walks_back_one_track() {
    let history = played(&[entry(10, 0), entry(20, 1)]);
    assert_eq!(
        resolve_previous(1_200, &history),
        PreviousAction::GoTo(entry(10, 0))
    );
    assert_eq!(
        resolve_previous(PREVIOUS_RESTART_THRESHOLD_MS, &history),
        PreviousAction::GoTo(entry(10, 0))
    );
}

#[test]
fn play_14_an_empty_history_rewinds_instead_of_doing_nothing() {
    let history = played(&[entry(10, 0)]);
    assert_eq!(
        resolve_previous(500, &history),
        PreviousAction::RestartCurrent
    );
    assert_eq!(
        resolve_previous(0, &PlaybackHistory::default()),
        PreviousAction::RestartCurrent
    );
}

#[test]
fn play_14_back_walks_the_heard_order_not_the_queue_order() {
    let mut history = played(&[entry(10, 0), entry(77, 7), entry(33, 3)]);
    assert_eq!(history.step_back(), Some(entry(77, 7)));
    assert_eq!(history.step_back(), Some(entry(10, 0)));
    assert_eq!(history.step_back(), None);
    assert_eq!(history.current(), Some(entry(10, 0)));
}

#[test]
fn play_14_a_queued_track_returns_to_the_entry_it_interrupted() {
    let mut history = played(&[entry(10, 0), entry(20, 1), queued(99)]);
    let back = history.step_back().expect("interrupted context entry");
    assert_eq!(back, entry(20, 1));
    assert_eq!(back.context_pos, Some(1));
}

#[test]
fn play_14_forward_returns_to_the_track_the_jump_left() {
    let mut history = played(&[entry(10, 0), entry(77, 7), entry(33, 3)]);
    history.step_back();
    history.step_back();
    assert!(history.can_go_forward());
    assert_eq!(history.step_forward(), Some(entry(77, 7)));
    assert_eq!(history.step_forward(), Some(entry(33, 3)));
    assert_eq!(history.step_forward(), None);
    assert!(!history.can_go_forward());
}

#[test]
fn play_14_a_new_track_after_a_back_jump_drops_the_forward_side() {
    let mut history = played(&[entry(10, 0), entry(20, 1)]);
    history.step_back();
    assert!(history.can_go_forward());
    history.record(entry(99, 9));
    assert!(!history.can_go_forward());
    assert_eq!(history.peek_back(), Some(entry(10, 0)));
}

#[test]
fn play_14_episodes_travel_the_history_like_tracks() {
    let mut history = PlaybackHistory::default();
    history.record(entry(10, 0));
    history.record(HistoryEntry {
        item: QueueItem::Episode(5),
        context_pos: None,
        sequence: (1, 1),
        from_up_next: true,
    });
    history.record(entry(20, 1));
    let back = history.step_back().expect("episode in history");
    assert_eq!(back.item, QueueItem::Episode(5));
    assert!(back.from_up_next);
    assert_eq!(back.context_pos, None);
}

#[test]
fn play_14_a_reseeded_context_keeps_the_track_but_drops_the_playhead() {
    let recorded = entry(10, 4);
    assert_eq!(recorded.playhead_in((1, 1)), Some(4));
    assert_eq!(recorded.playhead_in((1, 2)), None);
    assert_eq!(recorded.playhead_in((2, 1)), None);
    assert_eq!(queued(99).playhead_in((1, 1)), None);
}

#[test]
fn play_14_the_history_is_capped_and_cut_at_the_front() {
    let mut history = PlaybackHistory::default();
    for id in 0..(HISTORY_CAPACITY as i64 + 50) {
        history.record(entry(id, id as usize));
    }
    assert_eq!(history.back_len(), HISTORY_CAPACITY);
    for _ in 0..HISTORY_CAPACITY {
        history.step_back();
    }
    assert_eq!(history.step_back(), None);
    assert_eq!(
        history.current().map(|held| held.item),
        Some(QueueItem::Track(49))
    );
}
