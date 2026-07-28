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
    fixture
        .transport
        .queue_command(&QueueCommand::AddNext(vec![3]));
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
    fixture
        .transport
        .queue_command(&QueueCommand::AddNext(vec![2]));
    fixture.calls.clear();

    fixture.transport.queue_command(&QueueCommand::Clear);

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
    fixture
        .transport
        .queue_command(&QueueCommand::AddNext(vec![3]));
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
