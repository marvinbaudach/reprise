//! Positions are only meaningful against the queue a client actually saw.
//!
//! A position comes from a snapshot. Between taking it and sending it, the
//! queue may have moved — another client edited it, or the current track
//! simply ended. Applying the position anyway hits whichever row is there
//! now, which is a different row than the user pointed at. The revision is
//! what makes "the snapshot moved under you" answerable instead of silently
//! wrong.

use reprise_runtime_protocol::playback::PlaybackCommand;
use reprise_runtime_protocol::queue::QueueCommand;

use crate::runtime::Command;

use super::{full_client, harness, stamped_finished};

#[test]
fn a_positional_command_carrying_the_revision_the_client_saw_is_applied() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(
            client,
            &Command::PlayTracks {
                track_ids: vec![1, 2, 3],
                start_index: 0,
            },
        )
        .unwrap();
    harness
        .runtime
        .command(client, &Command::Queue(QueueCommand::AddNext(vec![2, 3])))
        .unwrap();
    let seen = harness.runtime.snapshot().unwrap().queue;

    harness
        .runtime
        .command(
            client,
            &Command::Queue(QueueCommand::RemoveAt {
                positions: vec![0],
                expected_revision: seen.revision,
            }),
        )
        .expect("the queue is exactly as the client last saw it");

    assert_eq!(
        harness
            .runtime
            .snapshot()
            .unwrap()
            .queue
            .play_next_track_ids,
        vec![3],
        "the row the client pointed at is the row that went"
    );
}

#[test]
fn a_positional_command_against_a_queue_that_moved_is_rejected_rather_than_applied() {
    let mut harness = harness();
    let watcher = full_client(&mut harness.runtime);
    let editor = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(
            editor,
            &Command::PlayTracks {
                track_ids: vec![1, 2, 3],
                start_index: 0,
            },
        )
        .unwrap();
    harness
        .runtime
        .command(editor, &Command::Queue(QueueCommand::AddNext(vec![2, 3])))
        .unwrap();
    // What the watcher is rendering.
    let seen = harness.runtime.snapshot().unwrap().queue;

    // Somebody else gets there first, and position 0 now means a different
    // row than the one the watcher is looking at.
    harness
        .runtime
        .command(editor, &Command::Queue(QueueCommand::AddNext(vec![1])))
        .unwrap();

    let error = harness
        .runtime
        .command(
            watcher,
            &Command::Queue(QueueCommand::RemoveAt {
                positions: vec![0],
                expected_revision: seen.revision,
            }),
        )
        .expect_err("the snapshot the position came from is gone");

    assert_eq!(error.category(), "rejected");
    assert_eq!(error.kind(), "rejected:stale_queue");
    assert!(
        !error.is_retryable(),
        "resending it would apply to the same wrong row; the client refreshes"
    );
    assert_eq!(
        harness
            .runtime
            .snapshot()
            .unwrap()
            .queue
            .play_next_track_ids,
        vec![1, 2, 3],
        "and nothing was removed"
    );
}

#[test]
fn a_track_ending_by_itself_moves_the_revision_too() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(
            client,
            &Command::PlayTracks {
                track_ids: vec![1, 2, 3],
                start_index: 0,
            },
        )
        .unwrap();
    let before = harness.runtime.snapshot().unwrap().queue.revision;

    harness.runtime.on_player_event(&stamped_finished());

    assert_ne!(
        harness.runtime.snapshot().unwrap().queue.revision,
        before,
        "the context window starts at the cursor, so an automatic advance \
         renumbers every context position — a revision that only counted \
         edits would let a stale position through precisely when the user \
         was not touching anything"
    );
}

#[test]
fn a_command_that_names_no_row_needs_no_revision() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(
            client,
            &Command::PlayTracks {
                track_ids: vec![1, 2, 3],
                start_index: 0,
            },
        )
        .unwrap();

    // Both name tracks rather than rows, so a queue that moved underneath
    // does not change what they mean. Requiring a revision here would make
    // "add this album to the queue" fail because something else finished.
    harness
        .runtime
        .command(client, &Command::Queue(QueueCommand::AddNext(vec![2])))
        .expect("adding by id is not positional");
    harness
        .runtime
        .command(client, &Command::Queue(QueueCommand::Purge(vec![2])))
        .expect("purging by id is not positional");
}

#[test]
fn the_revision_stands_still_when_a_command_changed_nothing() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(
            client,
            &Command::PlayTracks {
                track_ids: vec![1],
                start_index: 0,
            },
        )
        .unwrap();
    let before = harness.runtime.snapshot().unwrap().queue.revision;

    harness
        .runtime
        .command(client, &Command::Playback(PlaybackCommand::SetVolume(0.4)))
        .unwrap();

    assert_eq!(
        harness.runtime.snapshot().unwrap().queue.revision,
        before,
        "a revision that moved on unrelated commands would reject positions \
         that are still perfectly valid, and clients would learn to retry \
         blindly"
    );
}

#[test]
fn every_rejection_leaves_the_queue_exactly_as_it_was() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(
            client,
            &Command::PlayTracks {
                track_ids: vec![1, 2, 3],
                start_index: 0,
            },
        )
        .unwrap();
    harness
        .runtime
        .command(client, &Command::Queue(QueueCommand::AddNext(vec![2, 3])))
        .unwrap();
    let before = harness.runtime.snapshot().unwrap().queue;
    let stale = before.revision.wrapping_sub(1);

    for command in [
        QueueCommand::Move {
            from: 0,
            to: 1,
            expected_revision: stale,
        },
        QueueCommand::RemoveAt {
            positions: vec![0],
            expected_revision: stale,
        },
        QueueCommand::RemoveContextAt {
            positions: vec![0],
            expected_revision: stale,
        },
        QueueCommand::PlayNextAt {
            position: 0,
            expected_revision: stale,
        },
        QueueCommand::PlayContextAt {
            position: 0,
            expected_revision: stale,
        },
    ] {
        let error = harness
            .runtime
            .command(client, &Command::Queue(command.clone()))
            .expect_err("every positional command checks the revision");
        assert_eq!(
            error.kind(),
            "rejected:stale_queue",
            "{command:?} let a stale position through"
        );
    }

    assert_eq!(
        harness.runtime.snapshot().unwrap().queue,
        before,
        "a rejected command is one that did not happen"
    );
}
