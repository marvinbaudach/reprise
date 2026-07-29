//! Handing off to the next track without a gap.
//!
//! The backend can do it — it takes a pre-fed path and swaps at the end of
//! the current track instead of restarting the pipeline — but nothing ever
//! fed it one. `transport.rs`'s `AdvancedToNext` branch said so in a comment
//! and pointed at this task. Until it is wired, every track boundary on the
//! runtime path is an audible gap, which is a regression a user notices
//! immediately once GTK stops doing its own pre-feeding.
//!
//! The contract is `PlaybackBackend::set_next`'s own: re-fed whenever the
//! upcoming track changes, `None` when there is nothing to hand off to, and
//! last write wins.

use reprise_core::library::settings;
use reprise_core::playback::{PlayerEvent, StreamEvent, StreamGeneration};
use reprise_runtime_protocol::playback::PlaybackCommand;
use reprise_runtime_protocol::queue::QueueCommand;

use crate::runtime::Command;

use super::{full_client, harness, over, Harness};

/// Playing 1 out of a three-track context, with the pre-feed log cleared so
/// each test sees only what its own action provoked.
fn playing(harness: &mut Harness, client: crate::client::ClientId) {
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
}

#[test]
fn starting_a_track_pre_feeds_the_one_after_it() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);

    playing(&mut harness, client);

    assert_eq!(
        harness.playback.pre_fed(),
        Some(Some("/music/2.flac".into())),
        "without this the backend has nothing to swap to and every track \
         boundary restarts the pipeline"
    );
}

#[test]
fn editing_the_queue_re_feeds_because_the_upcoming_track_changed() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    playing(&mut harness, client);

    harness
        .runtime
        .command(client, &Command::Queue(QueueCommand::AddNext(vec![3])))
        .unwrap();

    assert_eq!(
        harness.playback.pre_fed(),
        Some(Some("/music/3.flac".into())),
        "the queued track now comes next; leaving the old pre-feed standing \
         would hand off to a track the user has just displaced"
    );
}

#[test]
fn the_last_track_pre_feeds_nothing_rather_than_leaving_the_previous_value() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(
            client,
            &Command::PlayTracks {
                track_ids: vec![1, 2],
                start_index: 0,
            },
        )
        .unwrap();

    harness
        .runtime
        .command(client, &Command::Playback(PlaybackCommand::Next))
        .unwrap();

    assert_eq!(
        harness.playback.pre_fed(),
        Some(None),
        "at the end of the queue the backend has to be told to stop \
         expecting a handoff — 'last write wins' means a stale path would \
         otherwise still be swapped in"
    );
}

#[test]
fn a_gapless_handoff_advances_the_model_without_starting_anything() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    playing(&mut harness, client);
    harness.playback.clear();

    // The backend reports that it swapped to the pre-fed track by itself.
    harness.runtime.on_player_event(&StreamEvent {
        generation: StreamGeneration::from(1),
        event: PlayerEvent::AdvancedToNext,
    });

    assert_eq!(
        harness.runtime.snapshot().unwrap().playback.track_id,
        Some(2),
        "the model has to follow the audio that is already rolling"
    );
    assert!(
        !harness
            .playback
            .calls()
            .iter()
            .any(|call| matches!(call, crate::fakes::BackendCall::Play(_))),
        "the audio is already playing; starting it again is the very gap \
         this exists to avoid"
    );
}

#[test]
fn a_gapless_handoff_pre_feeds_the_track_after_the_new_one() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    playing(&mut harness, client);
    harness.playback.clear();

    harness.runtime.on_player_event(&StreamEvent {
        generation: StreamGeneration::from(1),
        event: PlayerEvent::AdvancedToNext,
    });

    assert_eq!(
        harness.playback.pre_fed(),
        Some(Some("/music/3.flac".into())),
        "one handoff has to set up the next, or gapless works exactly once"
    );
}

#[test]
fn transition_off_pre_feeds_nothing_at_all() {
    // Configured before the runtime takes the connection: the runtime owns
    // the writer once it has it, and a test reaching around that would be
    // testing a seam the product does not have.
    let conn = reprise_core::db::open_migrated(None).expect("an in-memory database migrates");
    settings::set_gapless_enabled(&conn, false).expect("the setting is writable");
    let mut harness = over(conn);
    let client = full_client(&mut harness.runtime);

    playing(&mut harness, client);

    assert!(
        harness
            .playback
            .pre_feeds()
            .iter()
            .all(std::option::Option::is_none),
        "with the handoff switched off the backend must never hold a path — \
         pre-feeding anyway is a setting that does not take effect"
    );
}

#[test]
fn repeat_one_pre_feeds_the_track_it_will_repeat() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    playing(&mut harness, client);

    harness
        .runtime
        .command(
            client,
            &Command::Playback(PlaybackCommand::SetRepeat("one".into())),
        )
        .unwrap();

    assert_eq!(
        harness.playback.pre_fed(),
        Some(Some("/music/1.flac".into())),
        "the pre-feed has to agree with what the advance will actually do, \
         or toggling repeat leaves the backend holding the wrong track"
    );
}

#[test]
fn an_unresolvable_upcoming_track_pre_feeds_nothing() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);

    harness
        .runtime
        .command(
            client,
            &Command::PlayTracks {
                // 404 is not in the library — the file went away after the
                // queue was built.
                track_ids: vec![1, 404],
                start_index: 0,
            },
        )
        .unwrap();

    assert_eq!(
        harness.playback.pre_fed(),
        Some(None),
        "handing the backend a path for a track that cannot be resolved \
         trades an audible gap for a failed handoff, which is worse"
    );
}

#[test]
fn an_unplayable_queued_entry_does_not_push_the_pre_feed_past_the_rest_of_the_queue() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(
            client,
            &Command::PlayTracks {
                track_ids: vec![1, 3],
                start_index: 0,
            },
        )
        .unwrap();
    // 404 is gone; 2 is right behind it and is what should actually play.
    harness
        .runtime
        .command(client, &Command::Queue(QueueCommand::AddNext(vec![404, 2])))
        .unwrap();

    assert_eq!(
        harness.playback.pre_fed(),
        Some(Some("/music/2.flac".into())),
        "skipping the broken entry must not skip the whole explicit queue: \
         pre-feeding the context track behind it promises a handoff the \
         advance will not make"
    );
}

#[test]
fn a_gapless_handoff_lands_on_the_track_that_was_pre_fed() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(
            client,
            &Command::PlayTracks {
                track_ids: vec![1, 3],
                start_index: 0,
            },
        )
        .unwrap();
    harness
        .runtime
        .command(client, &Command::Queue(QueueCommand::AddNext(vec![404, 2])))
        .unwrap();
    harness.playback.clear();

    // The backend swapped to whatever it was pre-fed and says so.
    harness.runtime.on_player_event(&StreamEvent {
        generation: StreamGeneration::from(1),
        event: PlayerEvent::AdvancedToNext,
    });

    assert_eq!(
        harness.runtime.snapshot().unwrap().playback.track_id,
        Some(2),
        "this branch adopts a handoff without retrying, so a model that \
         picks a different track than the pre-feed did abandons the one the \
         backend is already playing — and abandoning stops it"
    );
    assert!(
        !harness
            .playback
            .calls()
            .iter()
            .any(|call| matches!(call, crate::fakes::BackendCall::Stop)),
        "stopping here would cut off audio that is already rolling"
    );
}
