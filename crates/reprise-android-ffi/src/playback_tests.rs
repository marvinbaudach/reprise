use std::sync::{Arc, Mutex};

use reprise_core::library::settings::TrackTransition;
use reprise_core::playback::{
    AudioEffects, PlaybackBackend, PlaybackState, PlayerEvent, StreamEvent, StreamGeneration,
};

use crate::playback::{
    AndroidPlaybackBackend, AndroidPlaybackError, AndroidPlaybackPort, AndroidPlaybackState,
    AndroidPlayerEvent, AndroidTransitionMode, PlaybackEventBridge,
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
}

impl AndroidPlaybackPort for RecordingPort {
    fn set_event_bridge(
        &self,
        _bridge: Arc<PlaybackEventBridge>,
    ) -> Result<(), AndroidPlaybackError> {
        self.record(PortCall::SetEventBridge);
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

#[test]
fn android_backend_routes_every_core_command_through_the_media3_port() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let backend = AndroidPlaybackBackend::new(
        Box::new(RecordingPort {
            calls: Arc::clone(&calls),
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
