use std::sync::{Arc, Mutex};

use reprise_core::db::Db;

use super::{
    AndroidPlaybackError, AndroidPlaybackPort, AndroidPlaybackState, AndroidTransitionMode,
    PlaybackEventBridge,
};
use crate::{
    AndroidEqualizerPoint, AndroidEqualizerSnapshot, AndroidPlaybackListener,
    AndroidPlaybackSession, AndroidPlaybackSnapshot,
};

struct ReaderProbePort {
    reader: Arc<Mutex<Db>>,
    reader_available_during_settings_calls: Arc<Mutex<Vec<bool>>>,
}

impl ReaderProbePort {
    fn record_reader_availability(&self) {
        let available = self.reader.try_lock().is_ok();
        self.reader_available_during_settings_calls
            .lock()
            .unwrap()
            .push(available);
    }
}

impl AndroidPlaybackPort for ReaderProbePort {
    fn set_event_bridge(
        &self,
        _bridge: Arc<PlaybackEventBridge>,
    ) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn play_path(&self, _path: String) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn play_uri(&self, _uri: String) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn toggle_pause(&self) -> Result<AndroidPlaybackState, AndroidPlaybackError> {
        Ok(AndroidPlaybackState::Paused)
    }

    fn seek_to(&self, _position_ms: i64) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn set_volume(&self, _volume: f64) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn set_equalizer(
        &self,
        _enabled: bool,
        _curve: Vec<AndroidEqualizerPoint>,
    ) -> Result<(), AndroidPlaybackError> {
        self.record_reader_availability();
        Ok(())
    }

    fn equalizer_snapshot(&self) -> Result<Option<AndroidEqualizerSnapshot>, AndroidPlaybackError> {
        Ok(None)
    }

    fn set_audio_effects(&self) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn set_spectrum_enabled(&self, _enabled: bool) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn stop(&self) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn set_next(&self, _uri: Option<String>) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn set_transition(&self, _mode: AndroidTransitionMode) -> Result<(), AndroidPlaybackError> {
        self.record_reader_availability();
        Ok(())
    }

    fn current_generation(&self) -> Result<u64, AndroidPlaybackError> {
        Ok(0)
    }
}

struct QuietListener;

impl AndroidPlaybackListener for QuietListener {
    fn on_playback_changed(&self, _snapshot: AndroidPlaybackSnapshot) {}

    fn on_listen_report_changed(&self) {}
}

#[test]
fn reload_releases_the_shared_reader_before_dispatching_settings_to_media3() {
    let directory = tempfile::tempdir().unwrap();
    let library = super::test_support::library_in(directory.path());
    let observations = Arc::new(Mutex::new(Vec::new()));
    let session = AndroidPlaybackSession::new(
        Arc::clone(&library),
        Box::new(ReaderProbePort {
            reader: library.reader_handle(),
            reader_available_during_settings_calls: Arc::clone(&observations),
        }),
        Box::new(QuietListener),
    )
    .unwrap();
    observations.lock().unwrap().clear();

    session.reload_playback_settings().unwrap();

    assert_eq!(observations.lock().unwrap().as_slice(), &[true, true]);
}
