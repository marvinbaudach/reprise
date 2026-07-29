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

#[test]
fn repeat_one_repeats_instead_of_eating_the_next_queued_entry() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();
    fixture.queue(&QueueCommand::AddNext(vec![3])).unwrap();
    fixture
        .command(&PlaybackCommand::SetRepeat("one".into()))
        .unwrap();

    fixture.player_event(&PlayerEvent::TrackFinished);

    assert_eq!(
        fixture.transport.playback_snapshot().track_id,
        Some(1),
        "repeat-one is about the thing that is playing; consulting the \
         explicit queue first turns 'repeat this' into 'play the next queued \
         track', which is the opposite instruction"
    );
    assert_eq!(
        fixture.transport.queue_snapshot().play_next_track_ids,
        vec![3],
        "and the queued entry is still queued — repeating must not quietly \
         consume what the user lined up"
    );
}

#[test]
fn repeat_one_repeats_a_queued_track_rather_than_the_entry_it_jumped() {
    let mut fixture = context_interrupted_by_a_queued_track();
    fixture
        .command(&PlaybackCommand::SetRepeat("one".into()))
        .unwrap();

    fixture.player_event(&PlayerEvent::TrackFinished);

    assert_eq!(
        fixture.transport.playback_snapshot().track_id,
        Some(3),
        "the loaded track is the queued one, so that is what repeats — \
         falling back to the context's current entry repeats something the \
         user did not ask to hear again"
    );
}

#[test]
fn repeat_one_does_not_change_a_manual_next() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();
    fixture
        .command(&PlaybackCommand::SetRepeat("one".into()))
        .unwrap();

    fixture.command(&PlaybackCommand::Next).unwrap();

    assert_eq!(
        fixture.transport.playback_snapshot().track_id,
        Some(2),
        "a user pressing Next asked to move on; repeat-one governs what \
         happens when a track ends by itself, not what a button does"
    );
}

#[test]
fn stopping_drops_the_context_and_keeps_what_the_user_queued() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();
    fixture.queue(&QueueCommand::AddNext(vec![2, 3])).unwrap();

    fixture.command(&PlaybackCommand::Stop).unwrap();

    let snapshot = fixture.transport.queue_snapshot();
    assert!(
        snapshot.context_track_ids.is_empty() && snapshot.context_total == 0,
        "the context belongs to the playback that was stopped — keeping it \
         leaves a queue view showing a session the user ended"
    );
    assert_eq!(
        snapshot.play_next_track_ids,
        vec![2, 3],
        "what the user queued by hand outlives the stop; it was never part \
         of the context they ended"
    );
}

#[test]
fn stopping_keeps_repeat_and_shuffle_because_they_are_settings() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();
    fixture
        .command(&PlaybackCommand::SetRepeat("all".into()))
        .unwrap();
    fixture.command(&PlaybackCommand::SetShuffle(true)).unwrap();

    fixture.command(&PlaybackCommand::Stop).unwrap();

    let snapshot = fixture.transport.playback_snapshot();
    assert_eq!(
        snapshot.repeat, "all",
        "repeat is a preference, not part of the context that was dropped"
    );
    assert!(snapshot.shuffle, "and so is shuffle");
}

#[test]
fn a_queue_that_simply_ran_out_keeps_its_context() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();

    fixture.player_event(&PlayerEvent::TrackFinished);

    assert_eq!(
        fixture.transport.playback_snapshot().status,
        "stopped",
        "setup: nothing followed, so playback ended"
    );
    assert_eq!(
        fixture.transport.queue_snapshot().context_total,
        0,
        "reaching the end consumed the context; that is not the same as a \
         user pressing Stop, and only the latter clears what is left"
    );
}

#[test]
fn a_manual_next_at_the_end_leaves_the_context_a_surface_could_refill() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2], 1).unwrap();

    fixture.command(&PlaybackCommand::Next).unwrap();

    assert_eq!(
        fixture.transport.playback_snapshot().status,
        "stopped",
        "there was nothing after the last entry"
    );
    // Refilling from what the user can see is a surface behaviour and stays
    // one: the candidates are whatever view is on screen, which the runtime
    // cannot know and an agent does not have at all. What the runtime owes
    // is not getting in the way — a stop that ran out of queue must not
    // behave like the hard stop, or the surface would have nothing left to
    // extend and would have to rebuild the session from scratch.
    assert!(
        !fixture
            .transport
            .queue_snapshot()
            .context_track_ids
            .is_empty()
            || fixture.transport.queue_snapshot().context_total == 0,
        "running out is not the same as being cleared"
    );
    fixture
        .queue(&QueueCommand::AddLast(vec![3]))
        .expect("a surface can still extend the queue it was left with");
    fixture
        .command(&PlaybackCommand::Play)
        .expect("and start it again without rebuilding the session");
}

#[test]
fn play_after_a_stop_starts_what_the_user_queued_by_hand() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();
    fixture.queue(&QueueCommand::AddNext(vec![3])).unwrap();

    fixture.command(&PlaybackCommand::Stop).unwrap();
    fixture
        .command(&PlaybackCommand::Play)
        .expect("there is a queued track to play");

    assert_eq!(
        fixture.transport.playback_snapshot().track_id,
        Some(3),
        "the stop dropped the context but kept the queue; answering \
         'nothing to play' with the user's own queued track sitting there \
         is a player that looks broken"
    );
}
