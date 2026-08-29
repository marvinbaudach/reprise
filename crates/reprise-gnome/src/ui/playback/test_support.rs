//! Shared controller construction for display-backed playback tests.

use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use reprise_core::playback::{PlaybackBackend, PlayerEvent};
use reprise_core::waveform::{RenderDataBackend, WaveformBackend, WaveformError};

use super::player_controller::{PlayerController, PlayerControllerBackends};
use crate::ui::scrobble_runtime::ScrobbleRuntime;

struct TestWaveform;

impl WaveformBackend for TestWaveform {
    fn extract_peaks(&self, _: &Path, buckets: usize) -> Result<Vec<u8>, WaveformError> {
        Ok(vec![0; buckets])
    }
}

impl RenderDataBackend for TestWaveform {}

pub(in crate::ui) fn controller_with_db(
    test_root: &Path,
    conn: Rc<reprise_core::db::Db>,
    playback: Box<dyn PlaybackBackend>,
) -> Rc<PlayerController> {
    let app = libadwaita::Application::builder()
        .application_id("io.github.marvinbaudach.Reprise.PlaybackTest")
        .build();
    let (_event_sender, playback_events) = async_channel::unbounded::<PlayerEvent>();
    let listenbrainz = ScrobbleRuntime::new(
        test_root.join("listenbrainz.db"),
        reprise_core::scrobbling::ScrobbleProvider::ListenBrainz,
        "ListenBrainz",
    );
    let lastfm = ScrobbleRuntime::new(
        test_root.join("lastfm.db"),
        reprise_core::scrobbling::ScrobbleProvider::LastFm,
        "Last.fm",
    );
    PlayerController::new(
        conn,
        crate::ui::cover_download_worker::setup_for_test(),
        listenbrainz,
        lastfm,
        PlayerControllerBackends {
            playback,
            playback_events,
            media: reprise_core::media_integration::MediaIntegrationHandles::inert(),
            waveform: Arc::new(TestWaveform),
        },
        &app,
    )
}
