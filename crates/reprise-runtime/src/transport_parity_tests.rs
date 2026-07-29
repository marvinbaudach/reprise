//! Where the runtime's queue has to behave like the one users already have.
//!
//! The runtime wraps the same `Queue` and `UpNextQueue` types the GTK
//! controller wraps, which makes it tempting to assume the behaviour came
//! along. It did not: what lived in the controller was the *binding* between
//! the two, and that is what these tests pin. Each one names a case where
//! the two answered differently, with the GTK behaviour as the contract —
//! the migration must not be the moment a user's player quietly changes its
//! mind.

use reprise_core::playback::PlayerEvent;
use reprise_runtime_protocol::playback::PlaybackCommand;
use reprise_runtime_protocol::queue::QueueCommand;

use super::fixture;

/// Plays 2 out of the context, then lets a queued track jump the line, so
/// something from "play next" is loaded while the context still stands at 2.
fn context_interrupted_by_a_queued_track() -> super::Fixture {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 1).unwrap();
    fixture
        .queue(&QueueCommand::AddNext(vec![3]))
        .expect("queuing succeeds");
    fixture
        .command(&PlaybackCommand::Next)
        .expect("the queued track jumps the line");
    assert_eq!(
        fixture.transport.playback_snapshot().track_id,
        Some(3),
        "setup: the queued track is what is loaded"
    );
    fixture
}

#[test]
fn previous_after_a_queued_track_returns_to_the_context_it_interrupted() {
    let mut fixture = context_interrupted_by_a_queued_track();

    fixture.command(&PlaybackCommand::Previous).unwrap();

    assert_eq!(
        fixture.transport.playback_snapshot().track_id,
        Some(2),
        "the queued track played *beside* the context, so going back means \
         going back to what it interrupted — stepping the context cursor as \
         well lands a track further back than the user ever heard"
    );
}

#[test]
fn previous_from_a_queued_track_at_the_head_of_the_context_still_goes_back() {
    let mut fixture = fixture();
    // The context sits on its first entry, so there is nothing *before* it.
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();
    fixture.queue(&QueueCommand::AddNext(vec![3])).unwrap();
    fixture.command(&PlaybackCommand::Next).unwrap();

    fixture.command(&PlaybackCommand::Previous).unwrap();

    assert_eq!(
        fixture.transport.playback_snapshot().track_id,
        Some(1),
        "asking the context for its predecessor finds nothing here and does \
         nothing at all, which reads as a dead Previous button — the answer \
         is the context's current track, not its previous one"
    );
}

#[test]
fn a_queued_track_leaves_the_context_cursor_where_it_was() {
    let fixture = context_interrupted_by_a_queued_track();

    // The window lists what comes *after* the current context entry, so a
    // context still standing on 2 has exactly 3 left to give.
    assert_eq!(
        fixture.transport.queue_snapshot().context_track_ids,
        vec![3],
        "the whole reason Previous has to be special-cased: playing a queued \
         track must not consume the context entry it interrupted"
    );
}

#[test]
fn previous_within_the_context_still_steps_back_by_one() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 1).unwrap();

    fixture.command(&PlaybackCommand::Previous).unwrap();

    assert_eq!(
        fixture.transport.playback_snapshot().track_id,
        Some(1),
        "nothing queued was involved, so this is the ordinary case and must \
         not have been broken by the special one"
    );
}

#[test]
fn a_finished_queued_track_still_hands_back_to_the_context() {
    let mut fixture = context_interrupted_by_a_queued_track();

    fixture.player_event(&PlayerEvent::TrackFinished);

    assert_eq!(
        fixture.transport.playback_snapshot().track_id,
        Some(3),
        "the context stood at 2 and moves on by itself; a queued track that \
         ends hands back rather than stopping, unlike external media"
    );
}
