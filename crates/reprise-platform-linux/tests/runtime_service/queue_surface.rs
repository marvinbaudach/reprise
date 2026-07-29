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

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn a_restored_session_arrives_without_starting_the_music() {
    use reprise_runtime_protocol::session::RestoredQueue;

    let served = Served::start("restore", Duration::from_secs(60));
    let (client, events) =
        start_with_bus_name(vec!["playback:control".to_owned()], served.bus_name.clone());
    await_event(
        &events,
        |event| matches!(event, ClientEvent::Connected(_)),
        "connection",
    );

    client
        .call(RuntimeCommand::RestoreSession {
            context: RestoredQueue {
                track_ids: vec![1, 2, 3],
                play_order: vec![0, 1, 2],
                position: Some(1),
                repeat: "off".into(),
                shuffled: false,
            },
            play_next: vec![3],
        })
        .expect("a stored session restores over the bus");

    let event = await_event(
        &events,
        |event| matches!(event, ClientEvent::QueueChanged { .. }),
        "queue delta",
    );
    let ClientEvent::QueueChanged { snapshot, .. } = event else {
        unreachable!("the matcher only accepts QueueChanged")
    };
    assert_eq!(
        snapshot.play_next_track_ids,
        vec![3],
        "the whole session travels, not just the context"
    );
    // Asked for rather than waited out. Proving "nothing started" by not
    // seeing an event within some timeout proves only that the timeout was
    // short; a fresh snapshot answers it outright.
    let observer = served.client();
    let seen = observer.connect().expect("a second peer may look");
    assert_eq!(
        seen.playback.status, "stopped",
        "opening the app is not a request to play"
    );
    assert_eq!(seen.playback.track_id, None, "and nothing is loaded");
    assert_eq!(
        seen.queue.context_track_ids,
        vec![3],
        "while the restored cursor stands where the user left it"
    );
    client.shutdown();
}

/// Reading past what the snapshot carries, over a real bus.
#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn a_queue_page_reaches_rows_the_snapshot_does_not_carry() {
    let served = Served::start("queuepage", Duration::from_secs(60));
    let (client, events) =
        start_with_bus_name(vec!["playback:control".to_owned()], served.bus_name.clone());
    await_event(
        &events,
        |event| matches!(event, ClientEvent::Connected(_)),
        "connection",
    );
    client
        .call(RuntimeCommand::PlayTracks {
            track_ids: (1..=400).collect(),
            start_index: 0,
        })
        .expect("a long queue starts");

    let page = client
        .queue_page("context", 250, 4)
        .expect("a page past the window is readable");

    assert_eq!(
        page.track_ids,
        vec![252, 253, 254, 255],
        "a virtual tail asks for the window it is about to draw, and the \
         snapshot's 200 rows do not reach here"
    );
    assert_eq!(page.total, 399);
    assert_eq!(
        page.section, "context",
        "the page names the section it answers for, so a view with two \
         outstanding reads cannot mix them up"
    );

    client.shutdown();
}

/// Absolute and relative seeking are different intentions, and therefore
/// different bus methods. This uses both in sequence so wiring `SeekTo` to
/// the relative method lands at 90 seconds and cannot pass unnoticed.
#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn an_absolute_seek_survives_the_client_and_service_wire() {
    use reprise_runtime_protocol::playback::PlaybackCommand;

    let served = Served::start("absoluteseek", Duration::from_secs(60));
    let (client, events) =
        start_with_bus_name(vec!["playback:control".to_owned()], served.bus_name.clone());
    await_event(
        &events,
        |event| matches!(event, ClientEvent::Connected(_)),
        "connection",
    );
    client
        .call(RuntimeCommand::PlayTracks {
            track_ids: vec![1],
            start_index: 0,
        })
        .expect("a track starts");
    client
        .call(RuntimeCommand::Playback(PlaybackCommand::Seek(30_000)))
        .expect("the relative seek establishes a non-zero origin");
    client
        .call(RuntimeCommand::Playback(PlaybackCommand::SeekTo(60_000)))
        .expect("the absolute seek reaches the runtime");

    let observer = served.client();
    let seen = observer.connect().expect("a second peer may look");
    assert_eq!(
        seen.playback.position_ms, 60_000,
        "the absolute target replaces the old position; treating it as \
         another delta would land at 90 seconds"
    );

    client.shutdown();
}
