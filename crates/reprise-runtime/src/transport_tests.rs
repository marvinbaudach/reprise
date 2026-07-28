//! Player and queue behaviour, driven through a recording backend.

use reprise_core::playback::{PlaybackState, PlayerEvent};
use reprise_runtime_protocol::playback::PlaybackCommand;
use reprise_runtime_protocol::queue::QueueCommand;

use super::Transport;
use crate::error::{Rejected, RuntimeError};
use crate::fakes::{BackendCall, FakeLibrary, FakePlayback, FakePlaybackHandle};
use crate::ports::{LibraryPort, PlayableTrack, TrackLocation};

struct Fixture {
    transport: Transport,
    backend: FakePlayback,
    calls: FakePlaybackHandle,
    library: FakeLibrary,
}

fn fixture() -> Fixture {
    let backend = FakePlayback::new();
    let calls = backend.handle();
    Fixture {
        transport: Transport::new(),
        backend,
        calls,
        library: FakeLibrary::with_tracks([1, 2, 3]),
    }
}

impl Fixture {
    fn play_tracks(&mut self, ids: Vec<i64>, start: usize) -> Result<(), RuntimeError> {
        self.transport
            .play_tracks(&self.backend, &self.library, ids, start)
    }

    fn command(&mut self, command: &PlaybackCommand) -> Result<(), RuntimeError> {
        self.transport
            .playback_command(&self.backend, &self.library, command)
    }

    fn queue(&mut self, command: &QueueCommand) -> Result<(), RuntimeError> {
        self.transport
            .queue_command(&self.backend, &self.library, command)
    }

    fn player_event(&mut self, event: &PlayerEvent) {
        self.transport
            .player_event(&self.backend, &self.library, event);
    }
}

#[test]
fn playing_a_list_starts_the_track_at_the_given_index() {
    let mut fixture = fixture();

    fixture.play_tracks(vec![1, 2, 3], 1).unwrap();

    assert_eq!(
        fixture.calls.calls(),
        vec![BackendCall::Play("/music/2.flac".into())]
    );
    let snapshot = fixture.transport.playback_snapshot();
    assert_eq!(snapshot.status, "playing");
    assert_eq!(snapshot.track_id, Some(2));
    assert_eq!(snapshot.title, "Track 2");
}

#[test]
fn a_stream_goes_to_the_uri_entry_point_rather_than_the_path_one() {
    let mut fixture = fixture();
    fixture.library = FakeLibrary::with_tracks([]).with(PlayableTrack {
        track_id: 9,
        location: TrackLocation::Uri("https://stream.example/live".into()),
        title: "Live".into(),
        artist: String::new(),
        album: String::new(),
        duration_ms: 0,
    });

    fixture.play_tracks(vec![9], 0).unwrap();

    assert_eq!(
        fixture.calls.calls(),
        vec![BackendCall::PlayUri("https://stream.example/live".into())],
        "a backend that receives a URI through the local-path entry point \
         would try to open it as a file"
    );
}

#[test]
fn playing_nothing_is_rejected_rather_than_starting_silence() {
    let mut fixture = fixture();

    assert_eq!(
        fixture
            .play_tracks(Vec::new(), 0)
            .expect_err("nothing to play"),
        RuntimeError::Rejected(Rejected::NothingToPlay)
    );
    assert!(fixture.calls.calls().is_empty());
}

#[test]
fn an_unresolvable_track_fails_without_touching_the_backend() {
    let mut fixture = fixture();

    let error = fixture
        .play_tracks(vec![404], 0)
        .expect_err("the library knows no such track");

    assert_eq!(error.category(), "failed");
    assert!(fixture.calls.calls().is_empty());
}

#[test]
fn pause_and_resume_toggle_once_each_and_are_idempotent() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();
    fixture.calls.clear();

    fixture.command(&PlaybackCommand::Pause).unwrap();
    fixture.command(&PlaybackCommand::Pause).unwrap();
    assert_eq!(fixture.transport.playback_snapshot().status, "paused");
    fixture.command(&PlaybackCommand::Play).unwrap();
    fixture.command(&PlaybackCommand::Play).unwrap();

    assert_eq!(
        fixture.calls.calls(),
        vec![BackendCall::TogglePause, BackendCall::TogglePause],
        "pausing a paused player and resuming a playing one are no-ops, not \
         second toggles that would undo the first"
    );
    assert_eq!(fixture.transport.playback_snapshot().status, "playing");
}

#[test]
fn stopping_unloads_the_track_so_the_runtime_can_become_idle() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();
    assert!(fixture.transport.is_active());

    fixture.command(&PlaybackCommand::Stop).unwrap();

    assert!(
        !fixture.transport.is_active(),
        "a paused track still counts as playback (§9.6); a stopped one does \
         not, and that difference is what the idle rule reads"
    );
    assert_eq!(fixture.transport.playback_snapshot().track_id, None);
    assert_eq!(fixture.transport.playback_snapshot().position_ms, 0);
}

#[test]
fn the_explicit_queue_is_played_before_the_context_continues() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();
    fixture.queue(&QueueCommand::AddNext(vec![3])).unwrap();
    fixture.calls.clear();

    fixture.command(&PlaybackCommand::Next).unwrap();
    assert_eq!(fixture.transport.playback_snapshot().track_id, Some(3));

    fixture.command(&PlaybackCommand::Next).unwrap();
    assert_eq!(
        fixture.transport.playback_snapshot().track_id,
        Some(2),
        "with the explicit queue drained, the context resumes where it stood \
         — the queued track played beside it, not inside it"
    );
}

#[test]
fn clearing_the_queue_does_not_stop_the_music() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2], 0).unwrap();
    fixture.queue(&QueueCommand::AddNext(vec![2])).unwrap();
    fixture.calls.clear();

    fixture.queue(&QueueCommand::Clear).unwrap();

    assert_eq!(fixture.transport.playback_snapshot().status, "playing");
    assert!(fixture
        .transport
        .queue_snapshot()
        .play_next_track_ids
        .is_empty());
    assert!(fixture.calls.calls().is_empty());
}

#[test]
fn a_finished_track_advances_and_the_end_of_the_queue_stops() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2], 0).unwrap();
    fixture.calls.clear();

    fixture.player_event(&PlayerEvent::TrackFinished);
    assert_eq!(fixture.transport.playback_snapshot().track_id, Some(2));

    fixture.player_event(&PlayerEvent::TrackFinished);
    assert_eq!(
        fixture.transport.playback_snapshot().status,
        "stopped",
        "running off the end stops rather than looping silently"
    );
    assert!(fixture.calls.calls().contains(&BackendCall::Stop));
}

#[test]
fn the_backends_duration_replaces_the_librarys_guess() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();

    fixture.player_event(&PlayerEvent::Position {
        position_ms: 5_000,
        duration_ms: 200_000,
    });

    let snapshot = fixture.transport.playback_snapshot();
    assert_eq!(snapshot.position_ms, 5_000);
    assert_eq!(
        snapshot.duration_ms, 200_000,
        "the tag-derived duration is a guess; the decoder's is not"
    );
}

#[test]
fn seeking_is_relative_and_clamped_to_the_track() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();
    fixture.player_event(&PlayerEvent::Position {
        position_ms: 10_000,
        duration_ms: 60_000,
    });
    fixture.calls.clear();

    fixture.command(&PlaybackCommand::Seek(5_000)).unwrap();
    assert_eq!(fixture.transport.playback_snapshot().position_ms, 15_000);

    fixture.command(&PlaybackCommand::Seek(-1_000_000)).unwrap();
    assert_eq!(fixture.transport.playback_snapshot().position_ms, 0);

    fixture.command(&PlaybackCommand::Seek(1_000_000)).unwrap();
    assert_eq!(
        fixture.transport.playback_snapshot().position_ms,
        60_000,
        "seeking past the end lands on the end instead of an impossible \
         position the backend would reject"
    );
    assert_eq!(
        fixture.calls.calls(),
        vec![
            BackendCall::SeekTo(15_000),
            BackendCall::SeekTo(0),
            BackendCall::SeekTo(60_000),
        ]
    );
}

#[test]
fn volume_is_clamped_and_the_applied_value_is_what_the_snapshot_reports() {
    let mut fixture = fixture();

    fixture.command(&PlaybackCommand::SetVolume(1.8)).unwrap();
    assert!((fixture.transport.playback_snapshot().volume - 1.0).abs() < f64::EPSILON);

    fixture.command(&PlaybackCommand::SetVolume(-0.5)).unwrap();
    assert!(fixture.transport.playback_snapshot().volume.abs() < f64::EPSILON);
    assert_eq!(
        fixture.calls.calls(),
        vec![BackendCall::SetVolume(1000), BackendCall::SetVolume(0)]
    );
}

#[test]
fn an_unknown_repeat_mode_is_rejected_instead_of_falling_back_to_off() {
    let mut fixture = fixture();

    assert_eq!(
        fixture
            .command(&PlaybackCommand::SetRepeat("sometimes".into()))
            .expect_err("that is not a repeat mode"),
        RuntimeError::Rejected(Rejected::UnknownRepeatMode)
    );
    assert_eq!(fixture.transport.playback_snapshot().repeat, "off");

    fixture
        .command(&PlaybackCommand::SetRepeat("all".into()))
        .unwrap();
    assert_eq!(fixture.transport.playback_snapshot().repeat, "all");
}

#[test]
fn a_backend_error_stops_playback_rather_than_leaving_a_phantom_track() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();

    fixture.player_event(&PlayerEvent::Error("decoder exploded".into()));

    assert_eq!(fixture.transport.playback_snapshot().status, "stopped");
    assert_eq!(fixture.transport.playback_snapshot().track_id, None);
}

#[test]
fn the_queue_snapshot_reports_what_plays_not_where_the_cursor_stands() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();
    fixture.queue(&QueueCommand::AddNext(vec![3])).unwrap();
    fixture.command(&PlaybackCommand::Next).unwrap();

    let snapshot = fixture.transport.queue_snapshot();
    assert_eq!(
        snapshot.current_track_id,
        Some(3),
        "the explicitly queued track is what a user hears, so it is what the \
         snapshot calls current"
    );
    assert!(snapshot.play_next_track_ids.is_empty());
    assert_eq!(
        snapshot.context_total,
        snapshot.context_track_ids.len() as u64
    );
}

#[test]
fn a_paused_backend_report_is_adopted_without_a_command() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();

    fixture.player_event(&PlayerEvent::StateChanged(PlaybackState::Paused));

    assert_eq!(
        fixture.transport.playback_snapshot().status,
        "paused",
        "the backend is the truth about whether audio is flowing"
    );
}

#[test]
fn resolving_an_absent_track_returns_nothing_rather_than_a_placeholder() {
    let library = FakeLibrary::with_tracks([1]);

    assert!(library.resolve(1).is_some());
    assert!(library.resolve(2).is_none());
}

#[test]
fn play_next_jumps_the_manual_line_and_add_to_queue_joins_its_back() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();

    fixture.queue(&QueueCommand::AddLast(vec![2])).unwrap();
    fixture.queue(&QueueCommand::AddNext(vec![3])).unwrap();

    assert_eq!(
        fixture.transport.queue_snapshot().play_next_track_ids,
        vec![3, 2],
        "\"play next\" is the whole point of being a separate command from \
         \"add to queue\"; appending both would make them the same button"
    );
}

#[test]
fn neither_queue_command_disturbs_the_surrounding_context() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();
    let context = fixture.transport.queue_snapshot().context_track_ids;

    fixture.queue(&QueueCommand::AddLast(vec![3])).unwrap();
    fixture.queue(&QueueCommand::AddNext(vec![2])).unwrap();

    assert_eq!(
        fixture.transport.queue_snapshot().context_track_ids,
        context,
        "the explicit queue sits beside the context, which is what makes \
         clearing it a complete undo"
    );
}

#[test]
fn a_queue_entry_can_be_moved_and_a_position_that_is_not_there_is_rejected() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();
    fixture.queue(&QueueCommand::AddLast(vec![2, 3])).unwrap();

    fixture
        .queue(&QueueCommand::Move { from: 1, to: 0 })
        .unwrap();
    assert_eq!(
        fixture.transport.queue_snapshot().play_next_track_ids,
        vec![3, 2]
    );

    assert_eq!(
        fixture
            .queue(&QueueCommand::Move { from: 9, to: 0 })
            .expect_err("there is no ninth entry"),
        RuntimeError::Rejected(Rejected::NoSuchQueueEntry),
        "a stale position means the client's snapshot moved under it, and a \
         silent no-op would leave it believing the move happened"
    );
}

#[test]
fn queue_entries_are_removed_by_position_so_a_repeated_track_loses_one_row() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();
    fixture
        .queue(&QueueCommand::AddLast(vec![2, 3, 2]))
        .unwrap();

    fixture.queue(&QueueCommand::RemoveAt(vec![0])).unwrap();

    assert_eq!(
        fixture.transport.queue_snapshot().play_next_track_ids,
        vec![3, 2],
        "removing by id would have taken both copies; the user pointed at \
         one row"
    );
}

#[test]
fn a_queued_entry_can_be_played_out_of_turn_and_leaves_the_queue() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1], 0).unwrap();
    fixture.queue(&QueueCommand::AddLast(vec![2, 3])).unwrap();
    fixture.calls.clear();

    fixture.queue(&QueueCommand::PlayNextAt(1)).unwrap();

    assert_eq!(fixture.transport.playback_snapshot().track_id, Some(3));
    assert_eq!(
        fixture.transport.queue_snapshot().play_next_track_ids,
        vec![2],
        "the entry that was played is no longer waiting to be played"
    );
    assert_eq!(
        fixture.calls.calls(),
        vec![BackendCall::Play("/music/3.flac".into())]
    );
}

#[test]
fn a_context_row_played_out_of_turn_keeps_everything_it_passed_queued() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();

    fixture.queue(&QueueCommand::PlayContextAt(2)).unwrap();

    assert_eq!(fixture.transport.playback_snapshot().track_id, Some(3));
    assert_eq!(
        fixture.transport.queue_snapshot().context_track_ids,
        vec![2],
        "the track it jumped over stays queued behind it — fast-forwarding \
         the playhead onto it instead would read as \"my upcoming songs \
         vanished\""
    );
}

#[test]
fn purging_a_deleted_track_leaves_the_one_that_is_playing_alone() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();
    fixture.queue(&QueueCommand::AddLast(vec![2])).unwrap();
    fixture.calls.clear();

    fixture.queue(&QueueCommand::Purge(vec![1, 2])).unwrap();

    assert_eq!(
        fixture.transport.playback_snapshot().track_id,
        Some(1),
        "stopping the music is not what deleting a file asked for"
    );
    assert!(fixture.calls.calls().is_empty());
    assert!(fixture
        .transport
        .queue_snapshot()
        .play_next_track_ids
        .is_empty());
    assert_eq!(
        fixture.transport.queue_snapshot().context_track_ids,
        vec![3],
        "the deleted track is gone from what is still to come"
    );
}

#[test]
fn a_context_row_can_be_removed_by_its_play_order_position() {
    let mut fixture = fixture();
    fixture.play_tracks(vec![1, 2, 3], 0).unwrap();

    fixture
        .queue(&QueueCommand::RemoveContextAt(vec![1]))
        .unwrap();

    assert_eq!(
        fixture.transport.queue_snapshot().context_track_ids,
        vec![3]
    );
}

/// The review finding this file did not previously cover: a *fresh* failed
/// start leaves `current` empty either way, so only an advance from a track
/// that was already playing shows the divergence.
#[test]
fn a_track_that_vanished_before_its_turn_stops_playback_instead_of_freezing_it() {
    let mut fixture = fixture();
    // The queue's second entry is not in the library — the ordinary shape of
    // a file deleted after the queue was built.
    fixture.library = FakeLibrary::with_tracks([1]);
    fixture.play_tracks(vec![1, 2], 0).unwrap();
    fixture.player_event(&PlayerEvent::Position {
        position_ms: 30_000,
        duration_ms: 60_000,
    });

    fixture.player_event(&PlayerEvent::TrackFinished);

    let snapshot = fixture.transport.playback_snapshot();
    assert_eq!(
        snapshot.status, "stopped",
        "the runtime must not keep reporting a track that already finished; \
         only changed facets are published, so a stale one is never corrected"
    );
    assert_eq!(snapshot.track_id, None);
    assert_eq!(snapshot.position_ms, 0, "and not the frozen old position");
    assert!(
        !fixture.transport.is_active(),
        "nothing is loaded, so the idle rule must be able to see that"
    );
}

#[test]
fn a_failed_skip_stops_the_previous_track_rather_than_leaving_it_audible() {
    let mut fixture = fixture();
    fixture.library = FakeLibrary::with_tracks([1]);
    fixture.play_tracks(vec![1, 2], 0).unwrap();
    fixture.calls.clear();

    let error = fixture
        .command(&PlaybackCommand::Next)
        .expect_err("the next track is not playable");

    assert_eq!(error.category(), "failed");
    assert!(
        fixture.calls.calls().contains(&BackendCall::Stop),
        "the backend was still playing the previous track; reporting nothing \
         loaded without stopping it is the same divergence in reverse"
    );
    assert_eq!(fixture.transport.playback_snapshot().track_id, None);
}

#[test]
fn a_failed_start_from_a_stop_leaves_the_stopped_state_it_found() {
    let mut fixture = fixture();
    fixture.library = FakeLibrary::with_tracks([]);

    let error = fixture
        .play_tracks(vec![7], 0)
        .expect_err("nothing resolves");

    assert_eq!(error.category(), "failed");
    assert_eq!(fixture.transport.playback_snapshot().status, "stopped");
    assert!(!fixture.transport.is_active());
}
