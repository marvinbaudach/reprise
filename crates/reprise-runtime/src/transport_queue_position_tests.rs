//! Queue commands that name a position.
//!
//! Split out of `transport_tests.rs` to keep that file under the 800-line
//! gate. These belong together: every one of them is about a command that
//! points at a row by where it sits, which is the family where an off-by-one
//! or a premature removal does damage that a bounds check would not catch.

use reprise_runtime_protocol::queue::QueueCommand;

use super::transport_tests::fixture;
use crate::error::{Rejected, RuntimeError};
use crate::fakes::BackendCall;

#[test]
fn a_queue_entry_can_be_moved_and_a_position_that_is_not_there_is_rejected() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();
    fixture.queue(&QueueCommand::AddLast(vec![2, 3])).unwrap();

    fixture
        .queue(&QueueCommand::Move {
            from: 1,
            to: 0,
            expected_revision: 0,
        })
        .unwrap();
    assert_eq!(
        fixture.transport.queue_snapshot().play_next_track_ids,
        vec![3, 2]
    );

    assert_eq!(
        fixture
            .queue(&QueueCommand::Move {
                from: 9,
                to: 0,
                expected_revision: 0,
            })
            .expect_err("there is no ninth entry"),
        RuntimeError::Rejected(Rejected::NoSuchQueueEntry),
        "a stale position means the client's snapshot moved under it, and a \
         silent no-op would leave it believing the move happened"
    );
}

#[test]
fn queue_entries_are_removed_by_position_so_a_repeated_track_loses_one_row() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();
    fixture
        .queue(&QueueCommand::AddLast(vec![2, 3, 2]))
        .unwrap();

    fixture
        .queue(&QueueCommand::RemoveAt {
            positions: vec![0],
            expected_revision: 0,
        })
        .unwrap();

    assert_eq!(
        fixture.transport.queue_snapshot().play_next_track_ids,
        vec![3, 2],
        "removing by id would have taken both copies; the user pointed at \
         one row"
    );
}

#[test]
fn a_queued_entry_can_be_played_out_of_turn_and_leaves_the_queue() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();
    fixture.queue(&QueueCommand::AddLast(vec![2, 3])).unwrap();
    fixture.calls.clear();

    fixture
        .queue(&QueueCommand::PlayNextAt {
            position: 1,
            expected_revision: 0,
        })
        .unwrap();

    assert_eq!(fixture.transport.playback_snapshot().track_id, Some(3));
    assert_eq!(
        fixture.transport.queue_snapshot().play_next_track_ids,
        vec![2],
        "the entry that was played is no longer waiting to be played"
    );
    assert_eq!(
        fixture.calls.calls(),
        vec![BackendCall::Play("/music/3.flac".into())]
    );
}

#[test]
fn playing_a_queued_episode_out_of_turn_is_refused_without_eating_the_entry() {
    // `take_at` pops before the kind is known, so the obvious implementation
    // removes the episode and then reports that the position held nothing:
    // the entry is gone and the caller is told it never existed. The refusal
    // has to leave the queue exactly as it found it.
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();
    fixture.transport.up_next.append(&[
        reprise_core::up_next::QueueItem::Track(2),
        reprise_core::up_next::QueueItem::Episode(9),
    ]);
    fixture.calls.clear();

    let refused = fixture.queue(&QueueCommand::PlayNextAt {
        position: 1,
        expected_revision: 0,
    });

    assert_eq!(
        refused,
        Err(RuntimeError::Rejected(Rejected::UnsupportedCommand)),
        "the entry exists, this runtime just cannot start it — that is not \
         the same as there being no entry"
    );
    assert_eq!(
        fixture.transport.up_next.ids(),
        &[
            reprise_core::up_next::QueueItem::Track(2),
            reprise_core::up_next::QueueItem::Episode(9),
        ],
        "a refused command must leave the queue untouched"
    );
    assert!(
        fixture.calls.calls().is_empty(),
        "nothing may reach the backend"
    );
}

#[test]
fn a_position_past_the_end_is_still_no_such_entry() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();
    fixture.queue(&QueueCommand::AddLast(vec![2])).unwrap();

    let refused = fixture.queue(&QueueCommand::PlayNextAt {
        position: 5,
        expected_revision: 0,
    });

    assert_eq!(
        refused,
        Err(RuntimeError::Rejected(Rejected::NoSuchQueueEntry)),
        "out of range keeps its own answer, distinct from an unplayable kind"
    );
}

#[test]
fn a_context_row_played_out_of_turn_keeps_everything_it_passed_queued() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();

    fixture
        .queue(&QueueCommand::PlayContextAt {
            position: 2,
            expected_revision: 0,
        })
        .unwrap();

    assert_eq!(fixture.transport.playback_snapshot().track_id, Some(3));
    assert_eq!(
        fixture.transport.queue_snapshot().context_track_ids,
        vec![2],
        "the track it jumped over stays queued behind it — fast-forwarding \
         the playhead onto it instead would read as \"my upcoming songs \
         vanished\""
    );
}

#[test]
fn purging_a_deleted_track_leaves_the_one_that_is_playing_alone() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();
    fixture.queue(&QueueCommand::AddLast(vec![2])).unwrap();
    fixture.calls.clear();

    fixture.queue(&QueueCommand::Purge(vec![1, 2])).unwrap();

    assert_eq!(
        fixture.transport.playback_snapshot().track_id,
        Some(1),
        "stopping the music is not what deleting a file asked for"
    );
    assert!(fixture.calls.calls().is_empty());
    assert!(fixture
        .transport
        .queue_snapshot()
        .play_next_track_ids
        .is_empty());
    assert_eq!(
        fixture.transport.queue_snapshot().context_track_ids,
        vec![3],
        "the deleted track is gone from what is still to come"
    );
}

#[test]
fn a_context_row_can_be_removed_by_its_play_order_position() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();

    fixture
        .queue(&QueueCommand::RemoveContextAt {
            positions: vec![1],
            expected_revision: 0,
        })
        .unwrap();

    assert_eq!(
        fixture.transport.queue_snapshot().context_track_ids,
        vec![3]
    );
}
