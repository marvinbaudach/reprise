//! Playing things that are not library tracks.
//!
//! Split out of `transport_tests.rs` when that file reached the repository's
//! 800-line ceiling. These share its fixture through `use super::*` and are
//! the same suite, not a separate concern.

use super::*;

fn a_stream() -> reprise_runtime_protocol::playback::ExternalMedia {
    reprise_runtime_protocol::playback::ExternalMedia {
        location: "https://stream.example/live".into(),
        remote: true,
        title: "Morning Show".into(),
        artist: "Example FM".into(),
        duration_ms: 0,
    }
}

#[test]
fn external_media_plays_without_a_library_id() {
    let mut fixture = fixture();

    fixture
        .transport
        .play_external(&fixture.backend, &a_stream(), None)
        .unwrap();

    let snapshot = fixture.transport.playback_snapshot();
    assert_eq!(snapshot.status, "playing");
    assert_eq!(
        snapshot.track_id, None,
        "a stream has no library id, and a client must never invent one"
    );
    assert_eq!(snapshot.title, "Morning Show");
    assert_eq!(snapshot.artist, "Example FM");
    assert_eq!(
        fixture.calls.calls(),
        vec![BackendCall::PlayUri("https://stream.example/live".into())]
    );
}

#[test]
fn a_local_episode_goes_to_the_path_entry_point_even_if_it_looks_like_a_uri() {
    let mut fixture = fixture();
    let episode = reprise_runtime_protocol::playback::ExternalMedia {
        location: "/podcasts/weird://name.mp3".into(),
        remote: false,
        ..a_stream()
    };

    fixture
        .transport
        .play_external(&fixture.backend, &episode, None)
        .unwrap();

    assert_eq!(
        fixture.calls.calls(),
        vec![BackendCall::Play("/podcasts/weird://name.mp3".into())],
        "the caller says which entry point it means; sniffing the string \
         would open this file as a URL"
    );
}

#[test]
fn external_media_leaves_the_queue_exactly_where_it_was() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();
    fixture.queue(&QueueCommand::AddLast(vec![2])).unwrap();
    let before = fixture.transport.queue_snapshot();

    fixture
        .transport
        .play_external(&fixture.backend, &a_stream(), None)
        .unwrap();

    let after = fixture.transport.queue_snapshot();
    assert_eq!(after.play_next_track_ids, before.play_next_track_ids);
    assert_eq!(after.context_track_ids, before.context_track_ids);
    assert_eq!(
        after.current_track_id, None,
        "what is playing is the stream, which is not in the queue at all"
    );
}

#[test]
fn a_finished_episode_does_not_start_the_music_queued_behind_it() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();
    fixture
        .transport
        .play_external(&fixture.backend, &a_stream(), None)
        .unwrap();
    fixture.calls.clear();

    fixture.player_event(&PlayerEvent::TrackFinished);

    assert_eq!(
        fixture.transport.playback_snapshot().status,
        "stopped",
        "the user asked for one episode; starting the queue afterwards is \
         both unasked for and loud"
    );
    assert!(fixture.calls.calls().contains(&BackendCall::Stop));
    assert!(
        !fixture
            .transport
            .queue_snapshot()
            .context_track_ids
            .is_empty(),
        "and the queue is still there to go back to"
    );
}

#[test]
fn a_backend_that_refuses_to_stop_is_not_reported_as_stopped() {
    let backend = StopRefusingBackend::new();
    let library = FakeLibrary::with_tracks([1]);
    let mut transport = Transport::new();
    transport
        .play_tracks(&backend, &library, vec![1], 0, None)
        .unwrap();

    let error = transport
        .playback_command(&backend, &library, &PlaybackCommand::Stop)
        .expect_err("the backend refused to go silent");

    assert_eq!(error.category(), "failed");
    assert!(
        transport.is_active(),
        "reporting nothing loaded while audio may still play is the lie Finding A is about"
    );
    let snapshot = transport.playback_snapshot();
    assert_eq!(
        snapshot.track_id,
        Some(1),
        "must keep the track that is presumably still playing"
    );
    assert_eq!(snapshot.status, "playing");

    // Not stuck: the same command reaches the same backend.stop() again.
    backend.allow_stop();
    transport
        .playback_command(&backend, &library, &PlaybackCommand::Stop)
        .expect("the backend now agrees to stop");
    assert!(!transport.is_active());
    assert_eq!(transport.playback_snapshot().track_id, None);
}

/// Track 2 is not in the library, so `Next`'s `start` fails before it ever
/// reaches `backend.play` — the same shape as a file deleted after the queue
/// was built, but this time `abandon`'s defensive stop also fails.
#[test]
fn a_failed_skip_with_an_uncooperative_backend_keeps_the_previous_track_current() {
    let backend = StopRefusingBackend::new();
    let library = FakeLibrary::with_tracks([1]);
    let mut transport = Transport::new();
    transport
        .play_tracks(&backend, &library, vec![1, 2], 0, None)
        .unwrap();

    let error = transport
        .playback_command(&backend, &library, &PlaybackCommand::Next)
        .expect_err("track 2 cannot be resolved");

    assert_eq!(error.category(), "failed");
    assert!(
        transport.is_active(),
        "abandon() must not lie either, for the same reason stop() must not"
    );
    let snapshot = transport.playback_snapshot();
    assert_eq!(snapshot.track_id, Some(1));
    assert_eq!(snapshot.status, "playing");
}

#[test]
fn external_media_without_a_location_is_rejected_before_the_backend() {
    let mut fixture = fixture();
    let nothing = reprise_runtime_protocol::playback::ExternalMedia {
        location: "   ".into(),
        ..a_stream()
    };

    assert_eq!(
        fixture
            .transport
            .play_external(&fixture.backend, &nothing, None)
            .expect_err("there is nothing to play"),
        RuntimeError::Rejected(Rejected::NothingToPlay)
    );
    assert!(fixture.calls.calls().is_empty());
}
