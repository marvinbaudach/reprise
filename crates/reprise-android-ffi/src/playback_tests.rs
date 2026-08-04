use std::sync::{Arc, Mutex};

use reprise_core::library::settings::TrackTransition;
use reprise_core::playback::{
    AudioEffects, PlaybackBackend, PlaybackState, PlayerEvent, StreamEvent, StreamGeneration,
};

use crate::playback::{
    AndroidPlaybackBackend, AndroidPlaybackError, AndroidPlaybackPort, AndroidPlaybackState,
    AndroidPlayerEvent, AndroidTransitionMode, PlaybackEventBridge,
};
use crate::{
    AndroidPlaybackListener, AndroidPlaybackSession, AndroidPlaybackSnapshot, AndroidRepeatMode,
};

#[derive(Clone, Debug, PartialEq)]
enum PortCall {
    SetEventBridge,
    PlayPath(String),
    PlayUri(String),
    TogglePause,
    SeekTo(i64),
    SetVolume(f64),
    SetAudioEffects,
    SetSpectrumEnabled(bool),
    Stop,
    SetNext(Option<String>),
    SetTransition(AndroidTransitionMode),
    CurrentGeneration,
}

struct RecordingPort {
    calls: Arc<Mutex<Vec<PortCall>>>,
    bridge: Arc<Mutex<Option<Arc<PlaybackEventBridge>>>>,
}

impl AndroidPlaybackPort for RecordingPort {
    fn set_event_bridge(
        &self,
        bridge: Arc<PlaybackEventBridge>,
    ) -> Result<(), AndroidPlaybackError> {
        self.record(PortCall::SetEventBridge);
        *self.bridge.lock().unwrap() = Some(bridge);
        Ok(())
    }

    fn play_path(&self, path: String) -> Result<(), AndroidPlaybackError> {
        self.record(PortCall::PlayPath(path));
        Ok(())
    }

    fn play_uri(&self, uri: String) -> Result<(), AndroidPlaybackError> {
        self.record(PortCall::PlayUri(uri));
        Ok(())
    }

    fn toggle_pause(&self) -> Result<AndroidPlaybackState, AndroidPlaybackError> {
        self.record(PortCall::TogglePause);
        Ok(AndroidPlaybackState::Paused)
    }

    fn seek_to(&self, position_ms: i64) -> Result<(), AndroidPlaybackError> {
        self.record(PortCall::SeekTo(position_ms));
        Ok(())
    }

    fn set_volume(&self, volume: f64) -> Result<(), AndroidPlaybackError> {
        self.record(PortCall::SetVolume(volume));
        Ok(())
    }

    fn set_audio_effects(&self) -> Result<(), AndroidPlaybackError> {
        self.record(PortCall::SetAudioEffects);
        Err(AndroidPlaybackError::Unsupported {
            detail: "audio effects are not supported by the Android backend".to_owned(),
        })
    }

    fn set_spectrum_enabled(&self, enabled: bool) -> Result<(), AndroidPlaybackError> {
        self.record(PortCall::SetSpectrumEnabled(enabled));
        Err(AndroidPlaybackError::Unsupported {
            detail: "spectrum analysis is not supported by the Android backend".to_owned(),
        })
    }

    fn stop(&self) -> Result<(), AndroidPlaybackError> {
        self.record(PortCall::Stop);
        Ok(())
    }

    fn set_next(&self, uri: Option<String>) -> Result<(), AndroidPlaybackError> {
        self.record(PortCall::SetNext(uri));
        Ok(())
    }

    fn set_transition(&self, mode: AndroidTransitionMode) -> Result<(), AndroidPlaybackError> {
        self.record(PortCall::SetTransition(mode));
        Ok(())
    }

    fn current_generation(&self) -> Result<u64, AndroidPlaybackError> {
        self.record(PortCall::CurrentGeneration);
        Ok(23)
    }
}

impl RecordingPort {
    fn record(&self, call: PortCall) {
        self.calls.lock().unwrap().push(call);
    }
}

struct RecordingListener {
    snapshots: Arc<Mutex<Vec<AndroidPlaybackSnapshot>>>,
}

impl AndroidPlaybackListener for RecordingListener {
    fn on_playback_changed(&self, snapshot: AndroidPlaybackSnapshot) {
        self.snapshots.lock().unwrap().push(snapshot);
    }
}

struct SessionFixture {
    session: AndroidPlaybackSession,
    calls: Arc<Mutex<Vec<PortCall>>>,
    bridge: Arc<Mutex<Option<Arc<PlaybackEventBridge>>>>,
    snapshots: Arc<Mutex<Vec<AndroidPlaybackSnapshot>>>,
    _directory: tempfile::TempDir,
}

fn recording_session() -> SessionFixture {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let bridge = Arc::new(Mutex::new(None));
    let snapshots = Arc::new(Mutex::new(Vec::new()));
    let directory = tempfile::tempdir().unwrap();
    let session = AndroidPlaybackSession::new(
        directory.path().to_str().unwrap(),
        Box::new(RecordingPort {
            calls: Arc::clone(&calls),
            bridge: Arc::clone(&bridge),
        }),
        Box::new(RecordingListener {
            snapshots: Arc::clone(&snapshots),
        }),
    )
    .unwrap();
    SessionFixture {
        session,
        calls,
        bridge,
        snapshots,
        _directory: directory,
    }
}

#[test]
fn android_backend_routes_every_core_command_through_the_media3_port() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let backend = AndroidPlaybackBackend::new(
        Box::new(RecordingPort {
            calls: Arc::clone(&calls),
            bridge: Arc::new(Mutex::new(None)),
        }),
        Box::new(|_| {}),
    )
    .unwrap();

    backend.play("/music/song.flac").unwrap();
    backend
        .play_uri("content://provider/document/song.flac")
        .unwrap();
    assert_eq!(backend.toggle_pause().unwrap(), PlaybackState::Paused);
    backend.seek_to(1_250).unwrap();
    backend.set_volume(0.4);
    let effects_error = backend
        .set_audio_effects(AudioEffects::default())
        .unwrap_err();
    let spectrum_error = backend.set_spectrum_enabled(true).unwrap_err();
    backend.stop().unwrap();
    backend.set_next(Some("content://provider/document/next.flac"));
    backend.set_next(None);
    backend.set_transition(TrackTransition::Crossfade, 8);
    assert_eq!(backend.current_generation(), StreamGeneration::from(23));

    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[
            PortCall::SetEventBridge,
            PortCall::PlayPath("/music/song.flac".to_owned()),
            PortCall::PlayUri("content://provider/document/song.flac".to_owned()),
            PortCall::TogglePause,
            PortCall::SeekTo(1_250),
            PortCall::SetVolume(0.4),
            PortCall::SetAudioEffects,
            PortCall::SetSpectrumEnabled(true),
            PortCall::Stop,
            PortCall::SetNext(Some("content://provider/document/next.flac".to_owned())),
            PortCall::SetNext(None),
            PortCall::SetTransition(AndroidTransitionMode::Gapless),
            PortCall::CurrentGeneration,
        ]
    );
    assert!(effects_error
        .to_string()
        .contains("audio effects are not supported"));
    assert!(spectrum_error
        .to_string()
        .contains("spectrum analysis is not supported"));
}

#[test]
fn tapping_a_track_starts_a_core_queue_at_that_position() {
    let fixture = recording_session();

    fixture
        .session
        .play_tracks(
            vec![10, 11, 12],
            vec![
                "content://provider/first.flac".to_owned(),
                "content://provider/second.flac".to_owned(),
                "content://provider/third.flac".to_owned(),
            ],
            1,
        )
        .unwrap();

    assert_eq!(
        fixture.session.snapshot().unwrap(),
        AndroidPlaybackSnapshot {
            state: AndroidPlaybackState::Playing,
            current_index: Some(1),
            position_ms: 0,
            duration_ms: 0,
            shuffled: false,
            repeat: AndroidRepeatMode::Off,
            error: None,
        }
    );
    assert_eq!(
        fixture.calls.lock().unwrap().as_slice(),
        &[
            PortCall::SetEventBridge,
            PortCall::SetTransition(AndroidTransitionMode::Gapless),
            PortCall::PlayUri("content://provider/second.flac".to_owned()),
            PortCall::CurrentGeneration,
            PortCall::SetNext(Some("content://provider/third.flac".to_owned())),
        ]
    );
    assert_eq!(
        fixture.snapshots.lock().unwrap().last(),
        Some(&fixture.session.snapshot().unwrap())
    );
}

#[test]
fn core_queue_owns_gapless_advance_and_manual_next_previous() {
    let fixture = recording_session();
    fixture
        .session
        .play_tracks(
            vec![10, 11, 12],
            vec![
                "content://provider/first.flac".to_owned(),
                "content://provider/second.flac".to_owned(),
                "content://provider/third.flac".to_owned(),
            ],
            0,
        )
        .unwrap();
    fixture.calls.lock().unwrap().clear();
    let bridge = fixture.bridge.lock().unwrap().clone().unwrap();

    bridge.emit(24, AndroidPlayerEvent::AdvancedToNext);
    bridge.emit(
        24,
        AndroidPlayerEvent::Position {
            position_ms: 1_250,
            duration_ms: 180_000,
        },
    );

    assert_eq!(
        fixture.session.snapshot().unwrap(),
        AndroidPlaybackSnapshot {
            state: AndroidPlaybackState::Playing,
            current_index: Some(1),
            position_ms: 1_250,
            duration_ms: 180_000,
            shuffled: false,
            repeat: AndroidRepeatMode::Off,
            error: None,
        }
    );
    assert_eq!(
        fixture.calls.lock().unwrap().as_slice(),
        &[PortCall::SetNext(Some(
            "content://provider/third.flac".to_owned()
        ))]
    );

    fixture.session.next().unwrap();
    fixture.session.previous().unwrap();

    assert_eq!(fixture.session.snapshot().unwrap().current_index, Some(1));
    assert_eq!(
        fixture.calls.lock().unwrap().as_slice(),
        &[
            PortCall::SetNext(Some("content://provider/third.flac".to_owned())),
            PortCall::PlayUri("content://provider/third.flac".to_owned()),
            PortCall::CurrentGeneration,
            PortCall::SetNext(None),
            PortCall::PlayUri("content://provider/second.flac".to_owned()),
            PortCall::CurrentGeneration,
            PortCall::SetNext(Some("content://provider/third.flac".to_owned())),
        ]
    );
}

#[test]
fn core_queue_starts_the_next_track_when_media3_reports_a_plain_end() {
    let fixture = recording_session();
    fixture
        .session
        .play_tracks(
            vec![10, 11],
            vec![
                "content://provider/first.flac".to_owned(),
                "content://provider/second.flac".to_owned(),
            ],
            0,
        )
        .unwrap();
    fixture.calls.lock().unwrap().clear();
    let bridge = fixture.bridge.lock().unwrap().clone().unwrap();

    bridge.emit(23, AndroidPlayerEvent::TrackFinished);

    assert_eq!(fixture.session.snapshot().unwrap().current_index, Some(1));
    assert_eq!(
        fixture.calls.lock().unwrap().as_slice(),
        &[
            PortCall::PlayUri("content://provider/second.flac".to_owned()),
            PortCall::CurrentGeneration,
            PortCall::SetNext(None),
        ]
    );
}

#[test]
fn exported_session_seek_reaches_the_media3_port() {
    let fixture = recording_session();
    fixture.calls.lock().unwrap().clear();

    fixture.session.seek_to(48_000).unwrap();

    assert_eq!(
        fixture.calls.lock().unwrap().as_slice(),
        &[PortCall::SeekTo(48_000)]
    );
}

#[test]
fn session_modes_are_readable_and_repeat_one_refeeds_after_media3_auto_advance() {
    let fixture = recording_session();
    fixture
        .session
        .play_tracks(vec![10], vec!["content://provider/only.flac".to_owned()], 0)
        .unwrap();

    fixture.session.set_shuffle(true).unwrap();
    fixture.session.set_repeat(AndroidRepeatMode::One).unwrap();
    let snapshot = fixture.session.snapshot().unwrap();
    assert!(snapshot.shuffled);
    assert_eq!(snapshot.repeat, AndroidRepeatMode::One);

    fixture.calls.lock().unwrap().clear();
    fixture
        .bridge
        .lock()
        .unwrap()
        .clone()
        .unwrap()
        .emit(24, AndroidPlayerEvent::AdvancedToNext);

    assert_eq!(fixture.session.snapshot().unwrap().current_index, Some(0));
    assert_eq!(
        fixture.calls.lock().unwrap().as_slice(),
        &[PortCall::SetNext(Some(
            "content://provider/only.flac".to_owned()
        ))],
        "Repeat::One must re-feed the real AdvancedToNext path Media3 emits",
    );
}

#[test]
fn play_count_uses_the_tracks_high_water_position_and_records_only_once() {
    let directory = tempfile::tempdir().unwrap();
    let music = directory.path().join("music");
    std::fs::create_dir(&music).unwrap();
    let track_path = music.join("sine.flac");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../android/app/src/main/assets/sine.flac"),
        &track_path,
    )
    .unwrap();
    let db_path = directory.path().join("reprise.db");
    let db = reprise_core::db::Db::open_migrated(Some(&db_path)).unwrap();
    reprise_core::library::scanner::scan_folder(&db, &music).unwrap();
    let track = reprise_core::queries::query_library_text_search(
        &db,
        "",
        reprise_core::queries::WindowRange {
            offset: 0,
            limit: 1,
        },
    )
    .unwrap()
    .rows
    .remove(0);
    drop(db);

    let calls = Arc::new(Mutex::new(Vec::new()));
    let bridge = Arc::new(Mutex::new(None));
    let session = AndroidPlaybackSession::new(
        directory.path().to_str().unwrap(),
        Box::new(RecordingPort {
            calls,
            bridge: Arc::clone(&bridge),
        }),
        Box::new(RecordingListener {
            snapshots: Arc::new(Mutex::new(Vec::new())),
        }),
    )
    .unwrap();
    session
        .play_tracks(vec![track.id], vec![track.path], 0)
        .unwrap();
    let events = bridge.lock().unwrap().clone().unwrap();

    events.emit(
        23,
        AndroidPlayerEvent::Position {
            position_ms: 600,
            // Media3 can know the position before it has resolved duration;
            // this tick cannot count yet, but its high-water must survive.
            duration_ms: 0,
        },
    );
    events.emit(
        23,
        AndroidPlayerEvent::Position {
            position_ms: 100,
            duration_ms: 1_000,
        },
    );

    let verify = reprise_core::db::Db::open_ready(&db_path).unwrap();
    let updated = reprise_core::queries::query_library_text_search(
        &verify,
        "",
        reprise_core::queries::WindowRange {
            offset: 0,
            limit: 1,
        },
    )
    .unwrap()
    .rows
    .remove(0);
    assert_eq!(updated.play_count, 1);
}

#[test]
fn playback_event_bridge_delivers_ordered_core_events_with_production_generations() {
    let received = Arc::new(Mutex::new(Vec::<StreamEvent>::new()));
    let recorded = Arc::clone(&received);
    let bridge = PlaybackEventBridge::new(Box::new(move |event| {
        recorded.lock().unwrap().push(event);
    }));

    bridge.emit(
        7,
        AndroidPlayerEvent::StateChanged {
            state: AndroidPlaybackState::Playing,
        },
    );
    bridge.emit(
        7,
        AndroidPlayerEvent::Position {
            position_ms: 1_250,
            duration_ms: 180_000,
        },
    );
    bridge.emit(8, AndroidPlayerEvent::AdvancedToNext);
    bridge.emit(8, AndroidPlayerEvent::TrackFinished);
    bridge.emit(
        8,
        AndroidPlayerEvent::Error {
            message: "decoder failed".to_owned(),
        },
    );

    let events = received.lock().unwrap();
    assert_eq!(events.len(), 5);
    assert_eq!(events[0].generation, StreamGeneration::from(7));
    assert!(matches!(
        events[0].event,
        PlayerEvent::StateChanged(PlaybackState::Playing)
    ));
    assert_eq!(events[1].generation, StreamGeneration::from(7));
    assert!(matches!(
        events[1].event,
        PlayerEvent::Position {
            position_ms: 1_250,
            duration_ms: 180_000
        }
    ));
    assert_eq!(events[2].generation, StreamGeneration::from(8));
    assert!(matches!(events[2].event, PlayerEvent::AdvancedToNext));
    assert_eq!(events[3].generation, StreamGeneration::from(8));
    assert!(matches!(events[3].event, PlayerEvent::TrackFinished));
    assert_eq!(events[4].generation, StreamGeneration::from(8));
    assert!(matches!(
        &events[4].event,
        PlayerEvent::Error(message) if message == "decoder failed"
    ));
}
