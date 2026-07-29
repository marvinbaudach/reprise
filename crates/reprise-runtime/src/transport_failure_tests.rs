//! What happens when a track will not play.
//!
//! Split out of `transport_tests.rs` because that file reached the 800-line
//! ceiling the architecture lint enforces, and "a start that failed" is a
//! coherent subject on its own: stepping over a broken entry, the bound that
//! keeps `Repeat::All` from retrying for ever, and reporting the reason so a
//! stop does not look like an exhausted queue.

use std::cell::RefCell;

use reprise_core::playback::PlayerEvent;
use reprise_runtime_protocol::playback::PlaybackCommand;

use super::fixture;
use crate::fakes::{BackendCall, FakeLibrary};
use crate::ports::{LibraryPort, PlayableTrack};

/// A library that can be emptied mid-test, which `FakeLibrary` cannot: the
/// bound on automatic skipping is only observable when *every* remaining
/// candidate fails, and a fixed library always keeps the track that is
/// already playing resolvable.
struct EmptyingLibrary {
    tracks: FakeLibrary,
    empty: RefCell<bool>,
}

impl EmptyingLibrary {
    fn with_tracks(ids: impl IntoIterator<Item = i64>) -> Self {
        Self {
            tracks: FakeLibrary::with_tracks(ids),
            empty: RefCell::new(false),
        }
    }

    /// Every track disappears from here on — the shape of a mount going away
    /// under a queue that was built while it was there.
    fn empty(&self) {
        *self.empty.borrow_mut() = true;
    }
}

impl LibraryPort for EmptyingLibrary {
    fn resolve(&self, track_id: i64) -> Option<PlayableTrack> {
        if *self.empty.borrow() {
            return None;
        }
        self.tracks.resolve(track_id)
    }
}

#[test]
fn an_unplayable_track_between_two_good_ones_is_stepped_over() {
    let mut fixture = fixture();
    // The middle entry is not in the library — a file deleted after the queue
    // was built, with more music behind it.
    fixture.library = FakeLibrary::with_tracks([1, 3]);
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();

    fixture.player_event(&PlayerEvent::TrackFinished);

    let snapshot = fixture.transport.playback_snapshot();
    assert_eq!(
        snapshot.track_id,
        Some(3),
        "one broken file must not end a queue that still has music in it"
    );
    assert_eq!(snapshot.status, "playing");
}

#[test]
fn a_track_that_was_stepped_over_is_named_on_the_snapshot() {
    let mut fixture = fixture();
    fixture.library = FakeLibrary::with_tracks([1, 3]);
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();

    fixture.player_event(&PlayerEvent::TrackFinished);

    let snapshot = fixture.transport.playback_snapshot();
    assert_eq!(
        snapshot.failure_kind.as_deref(),
        Some("not_playable"),
        "a surface that skipped a track silently cannot tell the user why \
         the track they queued never played"
    );
    assert_eq!(
        snapshot.failure_track_id,
        Some(2),
        "and it has to name the track, not just that something failed"
    );
}

#[test]
fn a_track_that_plays_clears_the_failure_before_it() {
    let mut fixture = fixture();
    fixture.library = FakeLibrary::with_tracks([1, 3]);
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();
    fixture.player_event(&PlayerEvent::TrackFinished);
    assert!(fixture.transport.playback_snapshot().failure_kind.is_some());

    fixture.play_tracks(vec![3], 0).unwrap();

    assert_eq!(
        fixture.transport.playback_snapshot().failure_kind,
        None,
        "the failure describes the current state, not a log — a track that \
         plays has nothing left to report"
    );
}

#[test]
fn a_queue_whose_every_remaining_entry_fails_stops_instead_of_trying_forever() {
    let mut fixture = fixture();
    let library = EmptyingLibrary::with_tracks([1, 2, 3]);
    fixture
        .transport
        .play_tracks(&fixture.backend, &library, vec![1, 2, 3], 0, None)
        .unwrap();
    // Repeat would otherwise hand the same three entries back for ever.
    fixture
        .transport
        .playback_command(
            &fixture.backend,
            &library,
            &PlaybackCommand::SetRepeat("all".into()),
        )
        .unwrap();
    library.empty();
    fixture.calls.clear();

    fixture
        .transport
        .player_event(&fixture.backend, &library, &PlayerEvent::TrackFinished);

    let snapshot = fixture.transport.playback_snapshot();
    assert_eq!(
        snapshot.status, "stopped",
        "with nothing left that can play, stopping is the only honest end"
    );
    assert!(
        !fixture
            .calls
            .calls()
            .iter()
            .any(|call| matches!(call, BackendCall::Play(_))),
        "no entry resolved, so nothing should have reached the backend"
    );
    assert_eq!(
        snapshot.failure_kind.as_deref(),
        Some("not_playable"),
        "and the reason has to survive the stop, or it looks like the queue \
         simply ran out"
    );
}

#[test]
fn a_backend_error_mid_playback_is_named_rather_than_a_silent_stop() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();

    fixture.player_event(&PlayerEvent::Error("decoder gave up".into()));

    let snapshot = fixture.transport.playback_snapshot();
    assert_eq!(snapshot.status, "stopped");
    assert_eq!(
        snapshot.failure_kind.as_deref(),
        Some("backend"),
        "a stream that dropped and a user pressing stop both leave the same \
         status; without this a surface cannot tell them apart"
    );
    assert_eq!(snapshot.failure_track_id, Some(1));
}
