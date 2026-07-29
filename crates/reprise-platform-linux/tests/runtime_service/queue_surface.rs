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
