//! The queue surface, over a real bus.
//!
//! Split out of `runtime_service.rs` because that file reached the 800-line
//! ceiling the architecture lint enforces. What is proved here is the pair
//! of rejections a positional command can earn, which are different mistakes
//! and must not collapse into one answer: a position that never existed, and
//! a position that exists but names a different row than the client meant.

use std::time::Duration;

use reprise_runtime_client::{ClientEvent, RuntimeCommand};

use super::{await_event, start_with_bus_name, Served};

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn the_queue_commands_a_queue_view_needs_all_survive_the_wire() {
    use reprise_runtime_protocol::queue::QueueCommand;

    let served = Served::start("queuesurface", Duration::from_secs(60));
    let (client, events) =
        start_with_bus_name(vec!["playback:control".to_owned()], served.bus_name.clone());
    await_event(
        &events,
        |event| matches!(event, ClientEvent::Connected(_)),
        "connection",
    );
    client
        .call(RuntimeCommand::PlayTracks {
            track_ids: vec![1, 2, 3],
            start_index: 0,
        })
        .expect("playing succeeds");

    // The revision carried by the *next* queue delta. Named for what it
    // does: the events arrive in order, so reading one per queue-changing
    // command keeps a caller in step. Draining fewer leaves a caller one
    // revision behind and looking at a rejection it caused itself.
    let next_revision = || {
        let event = await_event(
            &events,
            |event| matches!(event, ClientEvent::QueueChanged { .. }),
            "queue delta",
        );
        let ClientEvent::QueueChanged { snapshot, .. } = event else {
            unreachable!("the matcher only accepts QueueChanged")
        };
        snapshot.revision
    };

    // The commands that name tracks rather than rows: they mean the same
    // thing however the queue moved, so they carry no revision.
    // Seeding the queue was itself a queue change, so its delta has to be
    // taken too — a client that skips one is a client rendering a revision
    // the runtime has already moved past.
    let mut revision = next_revision();
    for command in [
        RuntimeCommand::Queue(QueueCommand::AddLast(vec![2, 3])),
        RuntimeCommand::Queue(QueueCommand::AddNext(vec![3])),
    ] {
        client
            .call(command.clone())
            .unwrap_or_else(|error| panic!("{command:?} was refused: {error}"));
        revision = next_revision();
    }

    // The row the client is looking at, against the queue it is looking at.
    client
        .call(RuntimeCommand::Queue(QueueCommand::Move {
            from: 0,
            to: 1,
            expected_revision: revision,
        }))
        .expect("the queue is exactly as this client last saw it");
    let revision = next_revision();

    // The same command, one revision behind — which is what a second client
    // editing the queue, or simply a track ending, leaves a surface holding.
    let stale = client
        .call(RuntimeCommand::Queue(QueueCommand::RemoveAt {
            positions: vec![0],
            expected_revision: revision.wrapping_sub(1),
        }))
        .expect_err("the snapshot those positions came from is gone");
    assert_eq!(
        stale.kind(),
        "rejected:stale_queue",
        "in range is not the same as still correct: without this the row          that goes is whichever one happens to sit at position 0 now"
    );
    assert!(
        !stale.is_retryable(),
        "the client refreshes, it does not retry"
    );

    // A position that never existed, against a current revision: a different
    // answer, because it is a different mistake.
    let absent = client
        .call(RuntimeCommand::Queue(QueueCommand::Move {
            from: 99,
            to: 0,
            expected_revision: revision,
        }))
        .expect_err("there is no hundredth entry");
    assert_eq!(absent.kind(), "rejected:no_such_queue_entry");

    for command in [
        RuntimeCommand::Queue(QueueCommand::Purge(vec![3])),
        RuntimeCommand::Queue(QueueCommand::Clear),
    ] {
        client
            .call(command.clone())
            .unwrap_or_else(|error| panic!("{command:?} was refused: {error}"));
    }
    client.shutdown();
}

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn a_command_reports_what_it_did_to_the_client_that_sent_it() {
    use reprise_runtime_protocol::queue::QueueCommand;

    let served = Served::start("queueoutcome", Duration::from_secs(60));
    let (client, events) =
        start_with_bus_name(vec!["playback:control".to_owned()], served.bus_name.clone());
    await_event(
        &events,
        |event| matches!(event, ClientEvent::Connected(_)),
        "connection",
    );
    client
        .call(RuntimeCommand::PlayTracks {
            track_ids: vec![1, 2, 3],
            start_index: 0,
        })
        .expect("playing succeeds");

    let added = client
        .call(RuntimeCommand::Queue(QueueCommand::AddNext(vec![1, 2, 3])))
        .expect("adding succeeds");
    assert_eq!(added.affected, 3, "three entries went in");

    // `send` cannot wait for the answer without stalling the caller's thread,
    // so the answer comes back as an event naming the send it belongs to.
    let request = client.send(RuntimeCommand::Queue(QueueCommand::RemoveAt {
        positions: vec![0, 2],
        expected_revision: added.queue_revision,
    }));

    let event = await_event(
        &events,
        |event| matches!(event, ClientEvent::CommandCompleted { .. }),
        "command outcome",
    );
    let ClientEvent::CommandCompleted {
        request: id,
        outcome,
    } = event
    else {
        unreachable!("the matcher only accepts CommandCompleted")
    };
    assert_eq!(
        id, request,
        "a surface with two removals in flight has to know which one this is"
    );
    assert_eq!(
        outcome.affected, 2,
        "the count has to survive the bus — a toast saying '2 removed' is the \
         whole reason this travels at all"
    );
    assert_ne!(
        outcome.queue_revision, added.queue_revision,
        "and the revision it left behind is the one a follow-up drag needs"
    );
    client.shutdown();
}
