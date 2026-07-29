//! What a command did, told to the client that sent it.
//!
//! A surface that removes four rows says "4 removed". It cannot say that
//! from a snapshot: by the time the delta arrives the rows are gone, and
//! counting the difference between two snapshots gets a concurrent change
//! wrong. The count belongs to the command, not to the state it left behind.

use reprise_runtime_protocol::playback::PlaybackCommand;
use reprise_runtime_protocol::queue::QueueCommand;

use crate::runtime::Command;

use super::{full_client, harness};

#[test]
fn a_removal_reports_how_many_rows_it_removed() {
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
        .command(
            client,
            &Command::Queue(QueueCommand::AddNext(vec![1, 2, 3])),
        )
        .unwrap();
    let revision = harness.runtime.snapshot().unwrap().queue.revision;

    let outcome = harness
        .runtime
        .command(
            client,
            &Command::Queue(QueueCommand::RemoveAt {
                // One of these is not there. A surface that says "3 removed"
                // when two went is worse than one that says nothing.
                positions: vec![0, 2, 99],
                expected_revision: revision,
            }),
        )
        .expect("two of the three positions are real");

    assert_eq!(outcome.affected, 2);
}

#[test]
fn the_outcome_carries_the_revision_the_command_left_behind() {
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
    let outcome = harness
        .runtime
        .command(client, &Command::Queue(QueueCommand::AddNext(vec![2, 3])))
        .unwrap();

    // The point of shipping the revision back: a surface can act again
    // immediately — dragging a second row, say — instead of waiting for its
    // own delta to come round before it is allowed to speak.
    harness
        .runtime
        .command(
            client,
            &Command::Queue(QueueCommand::Move {
                from: 0,
                to: 1,
                expected_revision: outcome.queue_revision,
            }),
        )
        .expect("the revision the previous command reported is the current one");
}

#[test]
fn a_command_that_edits_no_queue_reports_no_rows_rather_than_a_guess() {
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

    let outcome = harness
        .runtime
        .command(client, &Command::Playback(PlaybackCommand::SetVolume(0.4)))
        .unwrap();

    assert_eq!(
        outcome.affected, 0,
        "zero means it edited no entries, which is the truth about setting \
         the volume — it does not mean the count is unknown"
    );
    assert_eq!(
        outcome.queue_revision,
        harness.runtime.snapshot().unwrap().queue.revision,
        "and it still says where the queue stands, so a surface never has to \
         issue a command purely to find out"
    );
}

#[test]
fn clearing_reports_the_rows_it_dropped() {
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
    harness
        .runtime
        .command(
            client,
            &Command::Queue(QueueCommand::AddNext(vec![1, 2, 3])),
        )
        .unwrap();

    let outcome = harness
        .runtime
        .command(client, &Command::Queue(QueueCommand::Clear))
        .unwrap();

    assert_eq!(
        outcome.affected, 3,
        "an undo offer has to name a number, and 'cleared the queue' with no \
         number is the one thing a user cannot check"
    );
}

#[test]
fn a_purge_counts_the_entries_it_removed_not_the_ids_it_was_given() {
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
    // Track 2 sits in both queues, so purging it really does remove two
    // entries — the count is about entries, not about tracks.
    harness
        .runtime
        .command(client, &Command::Queue(QueueCommand::AddNext(vec![2])))
        .unwrap();

    let found = harness
        .runtime
        .command(client, &Command::Queue(QueueCommand::Purge(vec![2, 404])))
        .unwrap();
    assert_eq!(
        found.affected, 2,
        "one entry in the explicit queue and one in the context; 404 was \
         never anywhere and must not be counted"
    );

    let nothing = harness
        .runtime
        .command(client, &Command::Queue(QueueCommand::Purge(vec![404])))
        .unwrap();
    assert_eq!(
        nothing.affected, 0,
        "counting the ids it was handed rather than the entries it removed \
         would report work that did not happen"
    );
}
