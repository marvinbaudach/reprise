//! Construction of the long-lived services owned by the main window.

use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use libadwaita as adw;
use reprise_core::db::Db;
use reprise_core::waveform::RenderDataBackend;

use super::super::artist_news_worker::ArtistNewsRuntime;
use super::super::artist_portrait_worker::ArtistPortraitRuntime;
use super::super::concerts::ConcertsRuntime;
use super::super::cover_download_worker::{self, CoverDownloadRuntime};
use super::super::device_sync_runtime::DeviceSyncRuntime;
use super::super::player_controller::PlayerController;
use super::super::podcasts::PodcastsRuntime;
use super::super::scrobble_runtime::ScrobbleRuntime;

pub(super) struct WindowRuntimes {
    pub cover_download: CoverDownloadRuntime,
    pub listenbrainz: Rc<ScrobbleRuntime>,
    pub lastfm: Rc<ScrobbleRuntime>,
    pub artist_news: Rc<ArtistNewsRuntime>,
    pub concerts: Rc<ConcertsRuntime>,
    pub podcasts: Rc<PodcastsRuntime>,
    pub artist_portrait: Rc<ArtistPortraitRuntime>,
    pub device_sync: Rc<DeviceSyncRuntime>,
    pub player: Option<Rc<PlayerController>>,
}

pub(super) fn setup(
    app: &adw::Application,
    conn: &Rc<Db>,
    db_path: &Path,
    waveform_backend: Arc<dyn RenderDataBackend>,
) -> WindowRuntimes {
    let cover_download = cover_download_worker::setup(conn);
    let listenbrainz = ScrobbleRuntime::new(
        db_path.to_path_buf(),
        reprise_core::scrobbling::ScrobbleProvider::ListenBrainz,
        "ListenBrainz",
    );
    let lastfm = ScrobbleRuntime::new(
        db_path.to_path_buf(),
        reprise_core::scrobbling::ScrobbleProvider::LastFm,
        "Last.fm",
    );
    super::super::preference_lastfm::bootstrap(conn, &lastfm);
    super::super::preference_listenbrainz::bootstrap(conn, &listenbrainz);
    super::window_smoke::arm_listenbrainz(conn, &listenbrainz);
    super::window_smoke::arm_lastfm(conn, &lastfm);
    let artist_news = ArtistNewsRuntime::setup(conn);
    let concerts = ConcertsRuntime::setup(conn);
    let podcasts = PodcastsRuntime::setup(conn);
    let artist_portrait = ArtistPortraitRuntime::setup(conn);
    super::super::startup_report::mark("runtime setups");

    let media = std::env::var(crate::SMOKE_MPRIS_BUS_ENV_VAR).map_or_else(
        |_| reprise_platform_linux::mpris::start(crate::APP_ID),
        |bus_name| reprise_platform_linux::mpris::start_with_bus_name(crate::APP_ID, bus_name),
    );
    super::super::startup_report::mark("MPRIS");

    let device_sync =
        super::super::device_sync_smoke::runtime_from_env(conn).unwrap_or_else(|| {
            DeviceSyncRuntime::new(
                conn,
                reprise_platform_linux::device_sync::DeviceMonitor::new(),
            )
        });
    device_sync
        .bind_agent_device_sync(&media.device_sync_state, media.device_sync_commands.clone());
    super::super::device_sync_smoke::arm(&device_sync);
    super::super::startup_report::mark("device sync");

    let player = match super::player_backends::build(waveform_backend, media) {
        Ok(backends) => Some(PlayerController::new(
            conn.clone(),
            cover_download.clone(),
            listenbrainz.clone(),
            lastfm.clone(),
            backends,
            app,
        )),
        Err(error) => {
            tracing::error!(%error, "player unavailable: playback disabled");
            None
        }
    };
    super::super::startup_report::mark("player backends");

    WindowRuntimes {
        cover_download,
        listenbrainz,
        lastfm,
        artist_news,
        concerts,
        podcasts,
        artist_portrait,
        device_sync,
        player,
    }
}
