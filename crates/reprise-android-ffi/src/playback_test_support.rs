use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::playback::{
    AndroidPlaybackError, AndroidPlaybackItem, AndroidPlaybackPort, AndroidPlaybackState,
    AndroidPlayerEvent, AndroidTransitionMode, PlaybackEventBridge,
};
use crate::{
    AndroidEqualizerPoint, AndroidEqualizerSnapshot, AndroidPlaybackListener,
    AndroidPlaybackSegment, AndroidPlaybackSession, AndroidPlaybackSnapshot, MusicLibrary,
};

pub(super) fn library_in(directory: &Path) -> Arc<MusicLibrary> {
    Arc::new(
        MusicLibrary::open(
            directory.to_str().unwrap(),
            directory.join("cache").to_str().unwrap(),
        )
        .unwrap(),
    )
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum PortCall {
    SetEventBridge,
    PlayPath(String, f64),
    PlayUri(String),
    TogglePause,
    SeekTo(i64),
    SetVolume(f64),
    SetEqualizer(bool, Vec<AndroidEqualizerPoint>),
    EqualizerSnapshot,
    SetAudioEffects,
    SetSpectrumEnabled(bool),
    Stop,
    SetNext(Option<String>, f64),
    /// A track cut from a CUE file, recorded with everything the port gets.
    PlayClip(Clip),
    SetNextClip(Clip),
    SetGains(f64, Option<f64>),
    SetTransition(AndroidTransitionMode),
    CurrentGeneration,
}

/// A clipped item as the port receives it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Clip {
    pub(super) track_id: Option<i64>,
    pub(super) uri: String,
    pub(super) gain_db: f64,
    pub(super) segment: AndroidPlaybackSegment,
}

impl Clip {
    /// The clip `item` describes; `None` for a whole-file item.
    fn of(item: &AndroidPlaybackItem) -> Option<Self> {
        Some(Self {
            track_id: item.track_id,
            uri: item.uri.clone(),
            gain_db: item.gain_db,
            segment: item.segment?,
        })
    }
}

pub(super) const SYNCHRONOUS_BUFFERING_URI: &str = "test://synchronous-buffering";
pub(super) const FAILING_PLAY_URI: &str = "test://play-uri-failure";

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

    fn play_path(&self, item: AndroidPlaybackItem) -> Result<(), AndroidPlaybackError> {
        let emits_buffering = item.uri == SYNCHRONOUS_BUFFERING_URI;
        let fails = item.uri == FAILING_PLAY_URI;
        self.record(Clip::of(&item).map_or_else(
            || PortCall::PlayPath(item.uri.clone(), item.gain_db),
            PortCall::PlayClip,
        ));
        if emits_buffering {
            let bridge = self.bridge.lock().unwrap().clone().unwrap();
            bridge.emit(
                23,
                AndroidPlayerEvent::StateChanged {
                    state: AndroidPlaybackState::Buffering,
                },
            );
        }
        if fails {
            Err(AndroidPlaybackError::Backend {
                detail: "play_path failed".to_owned(),
            })
        } else {
            Ok(())
        }
    }

    fn play_uri(&self, uri: String) -> Result<(), AndroidPlaybackError> {
        let emits_buffering = uri == SYNCHRONOUS_BUFFERING_URI;
        let fails = uri == FAILING_PLAY_URI;
        self.record(PortCall::PlayUri(uri));
        if emits_buffering {
            let bridge = self.bridge.lock().unwrap().clone().unwrap();
            bridge.emit(
                23,
                AndroidPlayerEvent::StateChanged {
                    state: AndroidPlaybackState::Buffering,
                },
            );
        }
        if fails {
            Err(AndroidPlaybackError::Backend {
                detail: "play_uri failed".to_owned(),
            })
        } else {
            Ok(())
        }
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

    fn set_next(&self, item: Option<AndroidPlaybackItem>) -> Result<(), AndroidPlaybackError> {
        self.record(match item {
            None => PortCall::SetNext(None, 0.0),
            Some(item) => Clip::of(&item).map_or_else(
                || PortCall::SetNext(Some(item.uri.clone()), item.gain_db),
                PortCall::SetNextClip,
            ),
        });
        Ok(())
    }

    fn set_gains(
        &self,
        current_gain_db: f64,
        next_gain_db: Option<f64>,
    ) -> Result<(), AndroidPlaybackError> {
        self.record(PortCall::SetGains(current_gain_db, next_gain_db));
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
        library_in(directory.path()),
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
