//! The browse metadata is read from inside the playback port, so the library
//! reader must be free whenever the core calls `play_uri` or `set_next`.
//! `reload_playback_settings` once held it across a port call and the app
//! deadlocked; this pins the same property for the two calls that now read.

use std::sync::{Arc, Mutex};

use reprise_core::db::Db;

use crate::playback::{
    AndroidPlaybackError, AndroidPlaybackPort, AndroidPlaybackState, AndroidTransitionMode,
    PlaybackEventBridge,
};
use crate::{
    AndroidEqualizerPoint, AndroidEqualizerSnapshot, AndroidPlaybackListener,
    AndroidPlaybackSession, AndroidPlaybackSnapshot, MusicLibrary,
};

struct ReaderProbePort {
    reader: Arc<Mutex<Db>>,
    observations: Arc<Mutex<Vec<(&'static str, bool)>>>,
}

impl ReaderProbePort {
    fn observe(&self, call: &'static str) {
        let free = self.reader.try_lock().is_ok();
        self.observations.lock().unwrap().push((call, free));
    }
}

impl AndroidPlaybackPort for ReaderProbePort {
    fn set_event_bridge(
        &self,
        _bridge: Arc<PlaybackEventBridge>,
    ) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn play_path(&self, _path: String, _gain_db: f64) -> Result<(), AndroidPlaybackError> {
        self.observe("play_path");
        Ok(())
    }

    fn play_uri(&self, _uri: String) -> Result<(), AndroidPlaybackError> {
        self.observe("play_uri");
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

    fn set_next(&self, _uri: Option<String>, _gain_db: f64) -> Result<(), AndroidPlaybackError> {
        self.observe("set_next");
        Ok(())
    }

    fn set_gains(
        &self,
        _current_gain_db: f64,
        _next_gain_db: Option<f64>,
    ) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn set_transition(&self, _mode: AndroidTransitionMode) -> Result<(), AndroidPlaybackError> {
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
fn the_library_reader_is_free_while_the_core_calls_play_path_and_set_next() {
    let directory = tempfile::tempdir().unwrap();
    let library = Arc::new(
        MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap(),
    );
    let observations = Arc::new(Mutex::new(Vec::new()));
    let session = AndroidPlaybackSession::new(
        Arc::clone(&library),
        Box::new(ReaderProbePort {
            reader: library.reader_handle(),
            observations: Arc::clone(&observations),
        }),
        Box::new(QuietListener),
    )
    .unwrap();
    observations.lock().unwrap().clear();

    session
        .play_tracks(
            vec![1, 2, 3],
            vec![
                "content://a".into(),
                "content://b".into(),
                "content://c".into(),
            ],
            0,
        )
        .unwrap();
    session.next().unwrap();

    let observed = observations.lock().unwrap().clone();
    assert!(
        observed.iter().any(|(call, _)| *call == "play_path")
            && observed.iter().any(|(call, _)| *call == "set_next"),
        "the probe saw no play_path and set_next calls: {observed:?}"
    );
    assert!(
        observed.iter().all(|(_, free)| *free),
        "the library reader was held across a port call: {observed:?}"
    );
}
