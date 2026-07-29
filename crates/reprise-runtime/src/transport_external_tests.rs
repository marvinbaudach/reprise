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
        external_ref: "radio/7".into(),
        live: true,
    }
}

/// A downloaded episode: seekable, and its duration is not known yet — the
/// case that makes `live` a field rather than `duration_ms == 0`.
fn an_episode() -> reprise_runtime_protocol::playback::ExternalMedia {
    reprise_runtime_protocol::playback::ExternalMedia {
        location: "/podcasts/42.mp3".into(),
        remote: false,
        title: "Episode 42".into(),
        artist: "Example Show".into(),
        duration_ms: 0,
        external_ref: "podcast/42".into(),
        live: false,
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

/// A music context, playing, with a stream started on top of it. The stream
/// plays beside the queue — that is `play_external`'s own contract — so
/// everything the queue holds is still the user's music session.
fn a_stream_over_a_music_queue() -> Fixture {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();
    fixture
        .queue(&QueueCommand::AddNext(vec![2]))
        .expect("queuing succeeds");
    fixture
        .transport
        .play_external(&fixture.backend, &a_stream(), None)
        .expect("the stream plays");
    fixture
}

#[test]
fn stopping_a_stream_leaves_the_music_queue_alone() {
    let mut fixture = a_stream_over_a_music_queue();

    fixture.command(&PlaybackCommand::Stop).unwrap();

    let snapshot = fixture.transport.queue_snapshot();
    assert_eq!(
        snapshot.context_total, 2,
        "the stream never touched the context, so stopping it is not the \
         user ending their music session — wiping the queue here destroys a \
         playlist position they cannot get back"
    );
    assert_eq!(snapshot.play_next_track_ids, vec![2]);
}

#[test]
fn stopping_music_still_drops_the_context() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();

    fixture.command(&PlaybackCommand::Stop).unwrap();

    assert_eq!(
        fixture.transport.queue_snapshot().context_total,
        0,
        "the case the hard stop exists for must keep working"
    );
}

#[test]
fn next_during_a_stream_does_not_swap_in_the_music_behind_it() {
    let mut fixture = a_stream_over_a_music_queue();

    fixture.command(&PlaybackCommand::Next).unwrap();

    assert_eq!(
        fixture.transport.playback_snapshot().track_id,
        None,
        "the stream is still what is loaded; GTK gates both buttons on being \
         in queue mode and does nothing at all otherwise"
    );
    assert_eq!(
        fixture.transport.queue_snapshot().play_next_track_ids,
        vec![2],
        "and the queued track was not consumed by a press meant for the stream"
    );
}

#[test]
fn previous_during_a_stream_does_not_swap_in_the_music_behind_it() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 1).unwrap();
    fixture
        .transport
        .play_external(&fixture.backend, &a_stream(), None)
        .unwrap();

    fixture.command(&PlaybackCommand::Previous).unwrap();

    assert_eq!(
        fixture.transport.playback_snapshot().track_id,
        None,
        "there is a context entry before the cursor, so without the gate \
         Previous silently replaces the stream with a library track"
    );
}

// ---------------------------------------------------------------------------
// Identity, liveness, and why playback stopped.
//
// Three things a surface needs that the runtime did not carry. GTK has all
// three today — `external_media_mpris.rs` builds `podcast/{id}`/`radio/{id}`,
// `MprisState::live_stream` decides whether the item can be seeked at all, and
// `finish_podcast` marks an episode played and offers the next one — so a
// surface moved onto the runtime without them is a surface that loses them.
// ---------------------------------------------------------------------------

#[test]
fn the_surfaces_own_name_for_a_stream_comes_back_in_the_snapshot() {
    let mut fixture = fixture();

    fixture
        .transport
        .play_external(&fixture.backend, &a_stream(), None)
        .unwrap();

    assert_eq!(
        fixture
            .transport
            .playback_snapshot()
            .external_ref
            .as_deref(),
        Some("radio/7"),
        "without it nothing can tell two items apart: `track_id` is absent \
         for exactly these, and two episodes of a show share a title"
    );
}

#[test]
fn an_unnamed_item_reports_no_identity_rather_than_an_empty_one() {
    let mut fixture = fixture();
    let anonymous = reprise_runtime_protocol::playback::ExternalMedia {
        external_ref: String::new(),
        ..a_stream()
    };

    fixture
        .transport
        .play_external(&fixture.backend, &anonymous, None)
        .unwrap();

    assert_eq!(
        fixture.transport.playback_snapshot().external_ref,
        None,
        "`Some(\"\")` would make every reader check for two spellings of the \
         same nothing"
    );
}

#[test]
fn a_library_track_has_neither_an_external_name_nor_a_live_flag() {
    let mut fixture = fixture();

    fixture.play_tracks(vec![1], 0).unwrap();

    let snapshot = fixture.transport.playback_snapshot();
    assert_eq!(snapshot.external_ref, None);
    assert!(!snapshot.live);
}

#[test]
fn a_stream_is_live_and_an_episode_of_unknown_length_is_not() {
    let mut fixture = fixture();

    fixture
        .transport
        .play_external(&fixture.backend, &a_stream(), None)
        .unwrap();
    assert!(fixture.transport.playback_snapshot().live);

    fixture
        .transport
        .play_external(&fixture.backend, &an_episode(), None)
        .unwrap();
    let snapshot = fixture.transport.playback_snapshot();
    assert_eq!(
        snapshot.duration_ms, 0,
        "the episode's length is not known yet — the same zero the stream \
         reports"
    );
    assert!(
        !snapshot.live,
        "which is why this cannot be derived from duration_ms: doing so \
         disables the seek bar on every episode until the first duration \
         arrives"
    );
}

#[test]
fn an_episode_that_played_to_its_end_says_so() {
    let mut fixture = fixture();
    fixture
        .transport
        .play_external(&fixture.backend, &an_episode(), None)
        .unwrap();

    fixture.player_event(&PlayerEvent::TrackFinished);

    let snapshot = fixture.transport.playback_snapshot();
    assert_eq!(snapshot.status, "stopped");
    assert_eq!(
        snapshot.stopped_reason.as_deref(),
        Some("finished"),
        "a finished episode is marked played and hands the show on to the \
         next one; a surface reading only `stopped` cannot tell that from a \
         user who pressed stop, and guessing is wrong in one of the two"
    );
}

#[test]
fn stopping_an_episode_halfway_is_not_reported_as_finished() {
    let mut fixture = fixture();
    fixture
        .transport
        .play_external(&fixture.backend, &an_episode(), None)
        .unwrap();

    fixture.command(&PlaybackCommand::Stop).unwrap();

    assert_eq!(
        fixture.transport.playback_snapshot().stopped_reason,
        None,
        "marking this episode played would lose the user's place in it"
    );
}

/// The case the clear in `stop_hard` is actually for. The test above cannot
/// reach it: starting the episode had already cleared the field, so it would
/// pass with that line deleted.
#[test]
fn stopping_after_the_queue_ran_out_takes_back_the_offer_to_carry_on() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();
    fixture.player_event(&PlayerEvent::TrackFinished);
    assert_eq!(
        fixture
            .transport
            .playback_snapshot()
            .stopped_reason
            .as_deref(),
        Some("finished"),
        "the queue ended by itself, which is the state this starts from"
    );

    fixture.command(&PlaybackCommand::Stop).unwrap();

    assert_eq!(
        fixture.transport.playback_snapshot().stopped_reason,
        None,
        "Stop empties the context, so a surface still offering to carry on \
         from where it ended is offering something that is no longer there"
    );
}

#[test]
fn a_queue_that_ran_out_is_finished_too() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();

    fixture.player_event(&PlayerEvent::TrackFinished);

    let snapshot = fixture.transport.playback_snapshot();
    assert_eq!(snapshot.status, "stopped");
    assert_eq!(
        snapshot.stopped_reason.as_deref(),
        Some("finished"),
        "reaching the end of the queue is the moment a surface may offer to \
         carry on from what is on screen"
    );
}

#[test]
fn a_queue_that_gave_up_on_broken_files_is_not_reported_as_finished() {
    let mut fixture = fixture();
    // 404 is not in the library: the file went away after the queue was built.
    fixture.play_tracks(vec![1, 404], 0).unwrap();

    fixture.player_event(&PlayerEvent::TrackFinished);

    let snapshot = fixture.transport.playback_snapshot();
    assert_eq!(snapshot.status, "stopped");
    assert_eq!(
        snapshot.failure_kind.as_deref(),
        Some("not_playable"),
        "the failure facet is the fuller answer here"
    );
    assert_eq!(
        snapshot.stopped_reason, None,
        "two facets naming the same stop is two chances to contradict each \
         other about it"
    );
}

#[test]
fn a_backend_error_replaces_a_finished_that_was_already_standing() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();
    fixture.player_event(&PlayerEvent::TrackFinished);
    assert!(fixture
        .transport
        .playback_snapshot()
        .stopped_reason
        .is_some());

    fixture.player_event(&PlayerEvent::Error("the device went away".into()));

    let snapshot = fixture.transport.playback_snapshot();
    assert_eq!(snapshot.failure_kind.as_deref(), Some("backend"));
    assert_eq!(
        snapshot.stopped_reason, None,
        "the queue did not finish twice; the newer answer is the true one"
    );
}

#[test]
fn playing_again_clears_the_reason_the_last_thing_stopped() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();
    fixture.player_event(&PlayerEvent::TrackFinished);

    fixture.play_tracks(vec![2], 0).unwrap();

    assert_eq!(
        fixture.transport.playback_snapshot().stopped_reason,
        None,
        "the facet says what the situation is now, and nothing is stopped"
    );
}

/// A handoff reported when the runtime believed playback had ended.
///
/// The GStreamer backend cannot produce this — it clears its pre-fed slot on
/// every stop and every restart, so a real handoff always follows a `start`
/// that has already cleared the field. That promise lives in another crate,
/// about a trait this side only sees the near end of. The invariant is
/// written down in `transport.rs`, so it is held in `transport.rs`.
#[test]
fn a_handoff_after_a_finish_does_not_leave_the_finish_standing() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();
    fixture.player_event(&PlayerEvent::TrackFinished);
    assert_eq!(
        fixture
            .transport
            .playback_snapshot()
            .stopped_reason
            .as_deref(),
        Some("finished"),
        "the queue ran out, which is the state this starts from"
    );
    // Something to hand off to, so `load` is reached at all.
    fixture.queue(&QueueCommand::AddNext(vec![2])).unwrap();

    fixture.player_event(&PlayerEvent::AdvancedToNext);

    let snapshot = fixture.transport.playback_snapshot();
    assert_eq!(snapshot.track_id, Some(2), "the handoff was adopted");
    assert_eq!(
        snapshot.stopped_reason, None,
        "a track is current again; a surface reading `finished` here would \
         act on an ending that has been overtaken"
    );
}

#[test]
fn an_absolute_seek_lands_where_it_was_aimed() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();
    fixture.player_event(&PlayerEvent::Position {
        position_ms: 5_000,
        duration_ms: 240_000,
    });

    fixture
        .command(&PlaybackCommand::SeekTo(90_000))
        .expect("the track is 240s long");

    assert_eq!(
        fixture.transport.playback_snapshot().position_ms,
        90_000,
        "a scrubber knows where the user let go; expressing that as a delta \
         makes it depend on how far the playhead had run by the time the \
         message arrived, so the same drag lands elsewhere under load"
    );
}

#[test]
fn an_absolute_seek_past_the_end_stops_at_the_end() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();
    fixture.player_event(&PlayerEvent::Position {
        position_ms: 0,
        duration_ms: 240_000,
    });

    fixture.command(&PlaybackCommand::SeekTo(999_000)).unwrap();

    assert_eq!(fixture.transport.playback_snapshot().position_ms, 240_000);
}

#[test]
fn a_live_stream_refuses_to_be_seeked_rather_than_jumping_to_its_start() {
    let mut fixture = fixture();
    fixture
        .transport
        .play_external(&fixture.backend, &a_stream(), None)
        .unwrap();

    let error = fixture
        .command(&PlaybackCommand::SeekTo(30_000))
        .expect_err("a stream has no length to seek within");

    assert_eq!(error.kind(), "rejected:not_seekable");
    assert_eq!(
        fixture.transport.playback_snapshot().position_ms,
        0,
        "clamping against a duration of zero would silently turn every seek \
         into a jump to the start, which is the worst possible answer to \
         someone trying to skip ahead"
    );
}

#[test]
fn an_episode_seeks_once_its_duration_is_known() {
    let mut fixture = fixture();
    fixture
        .transport
        .play_external(&fixture.backend, &an_episode(), None)
        .unwrap();
    fixture.player_event(&PlayerEvent::Position {
        position_ms: 0,
        duration_ms: 3_600_000,
    });

    fixture
        .command(&PlaybackCommand::SeekTo(60_000))
        .expect("an episode is seekable");

    assert_eq!(fixture.transport.playback_snapshot().position_ms, 60_000);
}

/// The window between starting an episode and the first position report,
/// where this side does not yet know how long it is.
///
/// The test above does *not* cover this: it delivers a duration first, so it
/// only proves seeking works once the length is known. This is the case that
/// actually needed a guard.
#[test]
fn seeking_before_the_first_duration_report_does_not_collapse_to_the_start() {
    let mut fixture = fixture();
    // `an_episode` carries `duration_ms: 0` — unknown, not empty.
    fixture
        .transport
        .play_external(&fixture.backend, &an_episode(), None)
        .unwrap();

    fixture
        .command(&PlaybackCommand::SeekTo(60_000))
        .expect("an episode is seekable whether or not its length is known yet");

    assert_eq!(
        fixture.transport.playback_snapshot().position_ms,
        60_000,
        "clamping against a duration of zero turns every target into the \
         start — the same silent jump the live refusal exists to prevent, in \
         the one case that refusal deliberately lets through"
    );
    assert!(
        fixture.calls.calls().contains(&BackendCall::SeekTo(60_000)),
        "and the backend, which knows the real length even while this side \
         does not, is the one asked to judge it"
    );
}

#[test]
fn a_relative_seek_before_the_first_duration_report_moves_too() {
    let mut fixture = fixture();
    fixture
        .transport
        .play_external(&fixture.backend, &an_episode(), None)
        .unwrap();

    fixture.command(&PlaybackCommand::Seek(30_000)).unwrap();

    assert_eq!(
        fixture.transport.playback_snapshot().position_ms,
        30_000,
        "the pre-existing relative form had the same collapse"
    );
}

#[test]
fn seeking_before_the_start_still_stops_at_the_start() {
    let mut fixture = fixture();
    fixture
        .transport
        .play_external(&fixture.backend, &an_episode(), None)
        .unwrap();

    fixture.command(&PlaybackCommand::Seek(-30_000)).unwrap();

    assert_eq!(
        fixture.transport.playback_snapshot().position_ms,
        0,
        "the lower bound holds whether or not the length is known: there is \
         nothing before the start of anything"
    );
}
