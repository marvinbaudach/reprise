//! Who caused what.
//!
//! Split out of `runtime_tests.rs` because that file reached the 800-line
//! ceiling the architecture lint enforces. Attribution is its own subject:
//! §9.7 asks every mutation to name its initiator, RUN-5 needs "was this
//! mine?" to be decidable, and the quit policy needs to know whose session
//! is playing.

use crate::runtime::Command;

use super::{full_client, harness};

#[test]
fn an_event_names_the_client_whose_command_caused_it() {
    let mut harness = harness();
    let watcher = full_client(&mut harness.runtime);
    let actor = full_client(&mut harness.runtime);

    harness
        .runtime
        .command(
            actor,
            &Command::PlayTracks {
                track_ids: vec![1, 2, 3],
                start_index: 0,
            },
        )
        .expect("playing three known tracks succeeds");

    let delivery = harness.runtime.drain(watcher).unwrap();
    let caused = delivery
        .events
        .first()
        .expect("the command produced an event");
    assert_eq!(
        caused.initiator,
        Some(actor),
        "§9.7 asks every mutation to name its initiator; without it a surface \
         cannot tell its own change from somebody else's, which is exactly \
         what RUN-5 hangs on"
    );
}

#[test]
fn a_change_no_client_asked_for_names_nobody() {
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
    harness.runtime.drain(client).unwrap();

    // The backend reporting progress is not anybody's command.
    harness.runtime.on_player_event(&stamped_position(30_000));

    let delivery = harness.runtime.drain(client).unwrap();
    let tick = delivery
        .events
        .first()
        .expect("a moved position is a published change");
    assert_eq!(
        tick.initiator, None,
        "attributing a backend tick to whoever happened to command last \
         would make the log lie about who did what"
    );
}

#[test]
fn the_playback_facet_names_who_started_what_is_playing() {
    let mut harness = harness();
    let first = full_client(&mut harness.runtime);
    let second = full_client(&mut harness.runtime);

    harness
        .runtime
        .command(
            first,
            &Command::PlayTracks {
                track_ids: vec![1],
                start_index: 0,
            },
        )
        .unwrap();

    assert_eq!(
        harness.runtime.snapshot().unwrap().playback.initiated_by,
        Some(first.into()),
        "the quit policy turns on this: a surface may only stop playback it \
         started itself"
    );

    harness
        .runtime
        .command(
            second,
            &Command::PlayTracks {
                track_ids: vec![2],
                start_index: 0,
            },
        )
        .unwrap();

    assert_eq!(
        harness.runtime.snapshot().unwrap().playback.initiated_by,
        Some(second.into()),
        "and taking playback over transfers the claim, or the first surface \
         would still stop a stream it no longer owns"
    );
}

/// A position report stamped with the stream the transport is on, so it is
/// not discarded as stale before the test can observe it.
fn stamped_position(position_ms: i64) -> reprise_core::playback::StreamEvent {
    reprise_core::playback::StreamEvent {
        generation: reprise_core::playback::StreamGeneration::from(1),
        event: reprise_core::playback::PlayerEvent::Position {
            position_ms,
            duration_ms: 180_000,
        },
    }
}
