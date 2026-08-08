use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::playback::{
    AndroidPlaybackError, AndroidPlaybackPort, AndroidPlaybackState, AndroidTransitionMode,
    PlaybackEventBridge,
};
use crate::{
    AndroidEqualizerPoint, AndroidEqualizerSnapshot, AndroidPlaybackListener,
    AndroidPlaybackSession, AndroidPlaybackSnapshot,
};

#[derive(Clone, Debug, PartialEq)]
pub(super) enum PortCall {
    SetEventBridge,
    PlayPath(String),
    PlayUri(String),
    TogglePause,
    SeekTo(i64),
    SetVolume(f64),
    SetEqualizer(bool, Vec<AndroidEqualizerPoint>),
    EqualizerSnapshot,
    SetAudioEffects,
    SetSpectrumEnabled(bool),
    Stop,
    SetNext(Option<String>),
    SetTransition(AndroidTransitionMode),
    CurrentGeneration,
}

pub(super) struct RecordingPort {
    pub(super) calls: Arc<Mutex<Vec<PortCall>>>,
    pub(super) bridge: Arc<Mutex<Option<Arc<PlaybackEventBridge>>>>,
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

    fn set_equalizer(
        &self,
        enabled: bool,
        curve: Vec<AndroidEqualizerPoint>,
    ) -> Result<(), AndroidPlaybackError> {
        self.record(PortCall::SetEqualizer(enabled, curve));
        Ok(())
    }

    fn equalizer_snapshot(&self) -> Result<Option<AndroidEqualizerSnapshot>, AndroidPlaybackError> {
        self.record(PortCall::EqualizerSnapshot);
        Ok(None)
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

pub(super) struct RecordingListener {
    pub(super) snapshots: Arc<Mutex<Vec<AndroidPlaybackSnapshot>>>,
    pub(super) report_changes: Arc<AtomicUsize>,
}

impl AndroidPlaybackListener for RecordingListener {
    fn on_playback_changed(&self, snapshot: AndroidPlaybackSnapshot) {
        self.snapshots.lock().unwrap().push(snapshot);
    }

    fn on_listen_report_changed(&self) {
        self.report_changes.fetch_add(1, Ordering::Relaxed);
    }
}

pub(super) struct SessionFixture {
    pub(super) session: AndroidPlaybackSession,
    pub(super) calls: Arc<Mutex<Vec<PortCall>>>,
    pub(super) bridge: Arc<Mutex<Option<Arc<PlaybackEventBridge>>>>,
    pub(super) snapshots: Arc<Mutex<Vec<AndroidPlaybackSnapshot>>>,
    _directory: tempfile::TempDir,
}

pub(super) fn recording_session() -> SessionFixture {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let bridge = Arc::new(Mutex::new(None));
    let snapshots = Arc::new(Mutex::new(Vec::new()));
    let report_changes = Arc::new(AtomicUsize::new(0));
    let directory = tempfile::tempdir().unwrap();
    let session = AndroidPlaybackSession::new(
        directory.path().to_str().unwrap(),
        Box::new(RecordingPort {
            calls: Arc::clone(&calls),
            bridge: Arc::clone(&bridge),
        }),
        Box::new(RecordingListener {
            snapshots: Arc::clone(&snapshots),
            report_changes,
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
