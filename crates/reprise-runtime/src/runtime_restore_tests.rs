//! Coming back to where the user left off, without starting the music.
//!
//! Every way of putting something into the runtime so far also started it:
//! `PlayTracks` and `PlayExternal` both set the position to zero and the
//! status to playing. A cold start had no way to say "this is what was in
//! the queue" without also saying "and play it", so restoring a session
//! would have meant Reprise beginning to play the moment it opens.
//!
//! GTK does not do that today — `restore_session_queue` asserts it is not
//! starting playback — and it restores the shuffled order too, so "next"
//! continues where it left off rather than reshuffling behind the user's
//! back.

use reprise_runtime_protocol::playback::PlaybackCommand;
use reprise_runtime_protocol::session::RestoredQueue;

use crate::runtime::Command;

use super::{full_client, harness};

/// A stored context of three tracks, stopped on the second one.
fn stored() -> RestoredQueue {
    RestoredQueue {
        track_ids: vec![1, 2, 3],
        play_order: vec![0, 1, 2],
        position: Some(1),
        repeat: "off".into(),
        shuffled: false,
    }
}

#[test]
fn restoring_a_session_does_not_start_playing() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);

    harness
        .runtime
        .command(
            client,
            &Command::RestoreSession {
                context: stored(),
                play_next: vec![3],
            },
        )
        .expect("a stored session restores");

    let playback = harness.runtime.snapshot().unwrap().playback;
    assert_eq!(
        playback.status, "stopped",
        "opening the app is not a request to play; a restore that starts \
         the music is the single most intrusive thing a player can do on \
         launch"
    );
    assert_eq!(playback.track_id, None, "and nothing is loaded");
    assert!(
        harness.playback.calls().is_empty(),
        "the backend was never asked to do anything at all"
    );
}

#[test]
fn a_restored_session_holds_the_queue_the_user_left() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);

    harness
        .runtime
        .command(
            client,
            &Command::RestoreSession {
                context: stored(),
                play_next: vec![3],
            },
        )
        .unwrap();

    let queue = harness.runtime.snapshot().unwrap().queue;
    assert_eq!(
        queue.context_track_ids,
        vec![3],
        "the cursor stands on the second entry, so one is left after it"
    );
    assert_eq!(queue.play_next_track_ids, vec![3]);
}

#[test]
fn pressing_play_after_a_restore_resumes_where_the_user_stopped() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(
            client,
            &Command::RestoreSession {
                context: stored(),
                play_next: Vec::new(),
            },
        )
        .unwrap();

    harness
        .runtime
        .command(client, &Command::Playback(PlaybackCommand::Play))
        .expect("there is a restored entry to play");

    assert_eq!(
        harness.runtime.snapshot().unwrap().playback.track_id,
        Some(2),
        "the restored cursor is the whole point: play continues the session \
         rather than restarting it from the top"
    );
}

#[test]
fn a_restored_shuffle_order_is_the_one_the_user_was_hearing() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);

    harness
        .runtime
        .command(
            client,
            &Command::RestoreSession {
                context: RestoredQueue {
                    track_ids: vec![1, 2, 3],
                    // Shuffled: play order is 3, 1, 2 and the user is on 3.
                    play_order: vec![2, 0, 1],
                    position: Some(0),
                    repeat: "off".into(),
                    shuffled: true,
                },
                play_next: Vec::new(),
            },
        )
        .unwrap();

    let snapshot = harness.runtime.snapshot().unwrap();
    assert!(snapshot.playback.shuffle, "shuffle was on and stays on");
    assert_eq!(
        snapshot.queue.context_track_ids,
        vec![1, 2],
        "restoring the ids but reshuffling would change what comes next \
         behind the user's back — the order is part of what was saved"
    );
}

#[test]
fn a_stored_queue_that_does_not_add_up_is_rejected_rather_than_half_applied() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    // Play something first, so there is state a bad restore could damage.
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

    let error = harness
        .runtime
        .command(
            client,
            &Command::RestoreSession {
                context: RestoredQueue {
                    track_ids: vec![1, 2, 3],
                    // Not a permutation of the ids.
                    play_order: vec![0, 0, 0],
                    position: Some(0),
                    repeat: "off".into(),
                    shuffled: false,
                },
                play_next: Vec::new(),
            },
        )
        .expect_err("the stored order is not a permutation");

    assert_eq!(error.category(), "rejected");
    assert_eq!(error.kind(), "rejected:unusable_session");
    assert_eq!(
        harness.runtime.snapshot().unwrap().playback.track_id,
        Some(1),
        "a session file that got corrupted must not take the running player \
         down with it"
    );
}
