//! Window, view, and queue session orchestration.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::db::Db;
use reprise_core::library::session::{self, SessionSource, SessionState};
use reprise_core::media_integration::MprisPlaybackStatus;
use reprise_core::queue::{QueueSnapshot, Repeat};
use reprise_core::view_source::ViewSource;

use crate::ui::player_controller::PlayerController;
use crate::ui::track_list::TrackList;
use crate::ui::view_session::{self, TrackViewSnapshot};

const SEED_ENV: &str = "REPRISE_SMOKE_SESSION_SEED";
const REPORT_ENV: &str = "REPRISE_SMOKE_SESSION_REPORT";
const PLAY_ENV: &str = "REPRISE_SMOKE_SESSION_PLAY";

pub(super) fn load(db: &Db) -> SessionState {
    let persisted = session::load_and_mark_running(db);
    match std::env::var(SEED_ENV) {
        Ok(fixture) => match seeded_state(&fixture) {
            Some(state) => {
                tracing::info!(fixture, ?state, "session smoke fixture loaded");
                state
            }
            None => {
                tracing::warn!(
                    fixture,
                    "invalid session smoke fixture; loading persisted state"
                );
                persisted
            }
        },
        Err(_) => persisted,
    }
}

pub(super) fn apply_initial_geometry(window: &adw::ApplicationWindow, state: &SessionState) {
    if state.maximized {
        window.maximize();
    }
}

/// The place a normal start routes to (START-3): the last valid browser place.
/// Back/Forward stacks stay session-local, but the visible destination owns
/// its complete refinements, anchor, and selection across a restart.
pub(super) fn startup_place(state: &SessionState) -> reprise_core::browser::BrowserPlace {
    state
        .browser_place
        .clone()
        .or_else(|| state.library_root.clone())
        .unwrap_or_else(|| reprise_core::browser::BrowserPlace::from(ViewSource::Library))
}

pub(super) fn restore_runtime(player: Option<&Rc<PlayerController>>, state: &SessionState) {
    if let Some(player) = player {
        player.restore_session_queue(
            state.queue.clone(),
            state.up_next.clone(),
            state.current_up_next,
            crate::ui::playback::play_origin::from_session(
                state.play_origin.clone(),
                state.play_origin_label.clone(),
                state.play_origin_place.clone(),
            ),
        );
        crate::ui::playback::player_controller_wiring::arm_smoke_repeat(player);
    }
    if let Some(player) = player {
        let episode_restored = player.restore_session_episode(state.active_episode.as_ref());
        if !episode_restored {
            player.notify_restored_current_track();
        }
        arm_play(player);
    }
    if std::env::var(REPORT_ENV).is_ok() {
        let playback = player.map_or(MprisPlaybackStatus::Stopped, |player| {
            player.session_playback_status()
        });
        let runtime_queue = player.map(|player| player.session_queue_snapshot());
        let runtime_current_up_next = player.map(|player| player.session_up_next_snapshot().1);
        tracing::info!(
            ?state,
            ?runtime_queue,
            ?runtime_current_up_next,
            playback = playback.as_str(),
            "session restore report"
        );
    }
}

fn arm_play(player: &Rc<PlayerController>) {
    if std::env::var(PLAY_ENV).is_err() {
        return;
    }
    let player = Rc::downgrade(player);
    glib::idle_add_local_once(move || {
        let Some(player) = player.upgrade() else {
            return;
        };
        tracing::info!("{PLAY_ENV}: activating startup play button");
        player.bar.smoke_activate_play_pause();
    });
}

pub(super) fn wire_close(
    window: &adw::ApplicationWindow,
    conn: &Rc<Db>,
    track_list: &Rc<TrackList>,
    player: Option<&Rc<PlayerController>>,
    loaded: &SessionState,
    geometry_suppressed: &Rc<Cell<bool>>,
    nav_history: &Rc<crate::ui::nav_history::NavHistory>,
) {
    let geometry = Rc::new(Cell::new((
        loaded.window_width,
        loaded.window_height,
        loaded.maximized,
    )));
    wire_geometry_tracking(window, &geometry, geometry_suppressed);

    let conn = conn.clone();
    let track_list = Rc::downgrade(track_list);
    let player = player.map(Rc::downgrade);
    let loaded = loaded.clone();
    let saved = Cell::new(false);
    let geometry = geometry.clone();
    let geometry_suppressed = geometry_suppressed.clone();
    let nav_history = nav_history.clone();
    window.connect_close_request(move |window| {
        if saved.replace(true) {
            return glib::Propagation::Proceed;
        }
        let mut state = loaded.clone();
        let live = (window.width(), window.height(), window.is_maximized());
        let (width, height, maximized) =
            geometry_for_save(geometry_suppressed.get(), geometry.get(), live);
        state.window_width = width;
        state.window_height = height;
        state.maximized = maximized;
        if let Some(track_list) = track_list.upgrade() {
            apply_view_snapshot(&mut state, view_session::snapshot(&track_list));
            if let Some((current, library_root)) =
                nav_history.session_places(track_list.browser_place())
            {
                state.browser_place = Some(current);
                state.library_root = Some(library_root);
            }
        }
        if let Some(player) = player.as_ref().and_then(std::rc::Weak::upgrade) {
            player.persist_external_on_quit();
            state.queue = player.session_queue_snapshot();
            let (up_next, current_up_next) = player.session_up_next_snapshot();
            state.up_next = up_next;
            state.current_up_next = current_up_next;
            let origin = player.current_play_origin();
            let (origin_kind, origin_label, origin_place) =
                crate::ui::playback::play_origin::to_session(origin.as_ref());
            state.play_origin = origin_kind;
            state.play_origin_label = origin_label;
            state.play_origin_place = origin_place;
            state.active_episode = player.session_episode_snapshot();
        }

        match reprise_core::library::settings::get_library_root(&conn) {
            Ok(Some(root)) => session::mark_clean_exit_now(&mut state, root),
            Ok(None) => state.clean_exit = None,
            Err(error) => {
                state.clean_exit = None;
                tracing::warn!(%error, "could not read library root; clean exit will not suppress a startup scan");
            }
        }

        let result = session::save(&conn, &state);
        match &result {
            Ok(()) => tracing::info!(?state, "application session saved"),
            Err(error) => tracing::error!(%error, "could not save application session"),
        }
        debug_assert!(close_should_proceed(result.is_ok()));
        glib::Propagation::Proceed
    });
}

pub(super) fn arm_seed_close(window: &adw::ApplicationWindow) {
    if std::env::var(SEED_ENV).is_err() {
        return;
    }
    let window = window.clone();
    glib::timeout_add_seconds_local_once(1, move || {
        tracing::info!("session seed smoke closing through real close handler");
        window.close();
    });
}

fn wire_geometry_tracking(
    window: &adw::ApplicationWindow,
    geometry: &Rc<Cell<(i32, i32, bool)>>,
    suppressed: &Rc<Cell<bool>>,
) {
    for property in ["width", "height", "maximized"] {
        let geometry = geometry.clone();
        let suppressed = suppressed.clone();
        window.connect_notify_local(Some(property), move |window, _| {
            if suppressed.get() {
                return;
            }
            let (width, height, _) = geometry.get();
            let maximized = window.is_maximized();
            let size = if !maximized && window.width() > 0 && window.height() > 0 {
                (window.width(), window.height())
            } else {
                (width, height)
            };
            geometry.set((size.0, size.1, maximized));
        });
    }
}

fn geometry_for_save(
    suppressed: bool,
    tracked: (i32, i32, bool),
    live: (i32, i32, bool),
) -> (i32, i32, bool) {
    if suppressed {
        tracked
    } else if live.2 {
        (tracked.0, tracked.1, true)
    } else {
        live
    }
}

fn apply_view_snapshot(state: &mut SessionState, view: TrackViewSnapshot) {
    state.source = match view.source {
        ViewSource::Library => SessionSource::Library,
        ViewSource::RecentlyAdded => SessionSource::RecentlyAdded,
        ViewSource::Playlist(id) => SessionSource::Playlist(id),
        ViewSource::Smart(id) => SessionSource::Smart(id),
        ViewSource::Queue => SessionSource::Queue,
        ViewSource::Missing => SessionSource::Missing,
        ViewSource::ImportErrors => SessionSource::ImportErrors,
        ViewSource::MyStats
        | ViewSource::Releases
        | ViewSource::Concerts
        | ViewSource::Podcasts
        | ViewSource::Youtube
        | ViewSource::Radio
        | ViewSource::Conversions
        | ViewSource::Album { .. }
        | ViewSource::Artist(_)
        | ViewSource::Genre(_) => SessionSource::Library,
    };
    state.search = view.search;
    state.browse = view.browse;
    state.sort_field = view.sort.field;
    state.sort_dir = view.sort.dir;
}

fn seeded_state(fixture: &str) -> Option<SessionState> {
    if let Some(value) = fixture.strip_prefix("up-next:") {
        return seeded_up_next_state(value);
    }
    let ids = match fixture.strip_prefix("deterministic")? {
        "" => Vec::new(),
        value => value
            .strip_prefix(':')?
            .split(',')
            .map(str::parse)
            .collect::<Result<Vec<i64>, _>>()
            .ok()?,
    };
    let mut order: Vec<_> = (0..ids.len()).collect();
    order.reverse();
    let browse = reprise_core::queries::BrowseFilter {
        genre: Some("Rock".into()),
        artist: Some(String::new()),
        ..reprise_core::queries::BrowseFilter::default()
    };
    let mut browser_place = reprise_core::browser::BrowserPlace::from(ViewSource::Queue);
    if let Some(track_state) = browser_place.track_state_mut() {
        track_state.search = "session".into();
        track_state.browse = browse.clone();
        track_state.sort = reprise_core::browser::TrackSort::new(
            "rating",
            reprise_core::browser::SortDirection::Descending,
        );
    }
    Some(SessionState {
        window_width: 1111,
        window_height: 777,
        source: SessionSource::Queue,
        search: "session".into(),
        browse,
        sort_field: "rating".into(),
        sort_dir: "desc".into(),
        browser_place: Some(browser_place),
        queue: QueueSnapshot {
            position: (!ids.is_empty()).then_some(0),
            ids,
            order,
            repeat: Repeat::All,
            shuffled: true,
        },
        ..SessionState::default()
    })
}

fn seeded_up_next_state(value: &str) -> Option<SessionState> {
    let mut fields = value.split(':');
    let context = parse_smoke_ids(fields.next()?)?;
    let current_up_next = reprise_core::up_next::QueueItem::Track(fields.next()?.parse().ok()?);
    let pending = parse_smoke_ids(fields.next()?)?;
    if fields.next().is_some() {
        return None;
    }
    let pending = pending
        .into_iter()
        .map(reprise_core::up_next::QueueItem::Track)
        .collect::<Vec<_>>();
    let mut up_next = reprise_core::up_next::UpNextQueue::default();
    up_next.append(&pending);
    Some(SessionState {
        source: SessionSource::Queue,
        browser_place: Some(reprise_core::browser::BrowserPlace::from(ViewSource::Queue)),
        queue: QueueSnapshot {
            position: (!context.is_empty()).then_some(0),
            order: (0..context.len()).collect(),
            ids: context,
            repeat: Repeat::Off,
            shuffled: false,
        },
        up_next,
        current_up_next: Some(current_up_next),
        ..SessionState::default()
    })
}

fn parse_smoke_ids(value: &str) -> Option<Vec<i64>> {
    value
        .split(',')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()
}

fn close_should_proceed(_save_succeeded: bool) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use reprise_core::playback::{
        AudioEffects, PlaybackBackend, PlaybackError, PlaybackState, PlayerEvent,
    };
    use reprise_core::waveform::{RenderDataBackend, WaveformBackend, WaveformError};

    use super::*;
    use crate::ui::playback::player_controller::PlayerControllerBackends;
    use crate::ui::scrobble_runtime::ScrobbleRuntime;

    struct TestPlayback;

    impl PlaybackBackend for TestPlayback {
        fn play(&self, _: &str) -> Result<(), PlaybackError> {
            Ok(())
        }

        fn play_uri(&self, _: &str) -> Result<(), PlaybackError> {
            Ok(())
        }

        fn toggle_pause(&self) -> Result<PlaybackState, PlaybackError> {
            Ok(PlaybackState::Paused)
        }

        fn seek_to(&self, _: i64) -> Result<(), PlaybackError> {
            Ok(())
        }

        fn set_volume(&self, _: f64) {}

        fn set_audio_effects(&self, _: AudioEffects) -> Result<(), PlaybackError> {
            Ok(())
        }

        fn stop(&self) -> Result<(), PlaybackError> {
            Ok(())
        }

        fn set_next(&self, _: Option<&str>) {}

        fn set_transition(&self, _: reprise_core::library::settings::TrackTransition, _: u8) {}
    }

    struct TestWaveform;

    impl WaveformBackend for TestWaveform {
        fn extract_peaks(&self, _: &Path, buckets: usize) -> Result<Vec<u8>, WaveformError> {
            Ok(vec![0; buckets])
        }
    }

    impl RenderDataBackend for TestWaveform {}

    fn controller(test_root: &Path) -> Rc<PlayerController> {
        let conn = Rc::new(crate::test_db::open().unwrap());
        let app = libadwaita::Application::builder()
            .application_id("io.github.marvinbaudach.Reprise.SessionRepeatTest")
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
                playback: Box::new(TestPlayback),
                playback_events,
                media: reprise_core::media_integration::MediaIntegrationHandles::inert(),
                waveform: Arc::new(TestWaveform),
            },
            &app,
        )
    }

    struct SmokeRepeatGuard(Option<std::ffi::OsString>);

    impl SmokeRepeatGuard {
        fn arm() -> Self {
            let previous = std::env::var_os("REPRISE_SMOKE_REPEAT");
            std::env::set_var("REPRISE_SMOKE_REPEAT", "all");
            Self(previous)
        }
    }

    impl Drop for SmokeRepeatGuard {
        fn drop(&mut self) {
            if let Some(previous) = self.0.take() {
                std::env::set_var("REPRISE_SMOKE_REPEAT", previous);
            } else {
                std::env::remove_var("REPRISE_SMOKE_REPEAT");
            }
        }
    }

    #[test]
    fn close_always_proceeds_even_when_session_save_fails() {
        assert!(close_should_proceed(true));
        assert!(close_should_proceed(false));
        assert_eq!(
            geometry_for_save(true, (1200, 800, true), (440, 240, false)),
            (1200, 800, true)
        );
        assert_eq!(
            geometry_for_save(false, (1200, 800, true), (900, 600, false)),
            (900, 600, false)
        );
        assert_eq!(
            geometry_for_save(false, (1200, 800, true), (1920, 1080, true)),
            (1200, 800, true)
        );
    }

    #[test]
    fn up_next_smoke_fixture_seeds_current_and_pending_manual_tracks() {
        let state = seeded_state("up-next:1,2:3:4,5").unwrap();

        assert_eq!(state.queue.ids, vec![1, 2]);
        assert_eq!(state.queue.position, Some(0));
        assert_eq!(
            state.current_up_next,
            Some(reprise_core::up_next::QueueItem::Track(3))
        );
        assert_eq!(state.up_next.ids(), &[4, 5]);
        assert_eq!(state.queue.repeat, Repeat::Off);
        assert!(!state.queue.shuffled);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn smoke_repeat_all_survives_constructor_then_session_restore() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let _repeat = SmokeRepeatGuard::arm();
        let test_root = tempfile::tempdir().unwrap();
        let player = controller(test_root.path());
        let state = SessionState {
            queue: QueueSnapshot {
                position: None,
                ids: Vec::new(),
                order: Vec::new(),
                repeat: Repeat::Off,
                shuffled: false,
            },
            ..SessionState::default()
        };

        restore_runtime(Some(&player), &state);

        assert_eq!(player.session_queue_snapshot().repeat, Repeat::All);
    }

    #[test]
    fn transient_album_detail_saves_as_the_stable_library_source() {
        let mut state = SessionState::default();
        apply_view_snapshot(
            &mut state,
            TrackViewSnapshot {
                source: ViewSource::Album {
                    album: "Blue".into(),
                    album_artist: "Joni Mitchell".into(),
                },
                search: String::new(),
                browse: reprise_core::queries::BrowseFilter::default(),
                sort: crate::ui::track_list_sort::SortState::default(),
            },
        );

        assert_eq!(state.source, SessionSource::Library);
    }

    #[test]
    fn transient_artist_detail_saves_as_the_stable_library_source() {
        let mut state = SessionState::default();
        apply_view_snapshot(
            &mut state,
            TrackViewSnapshot {
                source: ViewSource::Artist("Björk".into()),
                search: String::new(),
                browse: reprise_core::queries::BrowseFilter::default(),
                sort: crate::ui::track_list_sort::SortState::default(),
            },
        );

        assert_eq!(state.source, SessionSource::Library);
    }

    #[test]
    fn start_3_missing_browser_place_falls_back_to_the_library_root() {
        let state = SessionState {
            browser_place: None,
            library_root: None,
            ..SessionState::default()
        };

        assert_eq!(
            startup_place(&state),
            reprise_core::browser::BrowserPlace::from(ViewSource::Library)
        );
    }

    #[test]
    fn start_3_startup_place_restores_the_last_browser_place() {
        let mut remembered = reprise_core::browser::BrowserPlace::from(ViewSource::Playlist(7));
        let state = remembered
            .track_state_mut()
            .expect("a playlist is a track place");
        state.search = "remember me".into();
        state.selected_ids = vec![41];
        let session = SessionState {
            browser_place: Some(remembered.clone()),
            ..SessionState::default()
        };

        assert_eq!(startup_place(&session), remembered);
    }

    #[test]
    fn browse_12_restart_restores_the_last_location_and_playback_origin() {
        use reprise_core::browser::BrowserPlace;

        let db = crate::test_db::open().unwrap();
        crate::test_db::connection(&db)
            .execute(
                "INSERT INTO playlists (id, name, position) VALUES (7, 'Road', 0)",
                [],
            )
            .unwrap();
        let last_location = BrowserPlace::from(ViewSource::Playlist(7));
        let mut play_origin = BrowserPlace::from(ViewSource::Library);
        play_origin.track_state_mut().unwrap().search = "origin query".into();
        let state = SessionState {
            sort_field: "year".into(),
            sort_dir: "desc".into(),
            browser_place: Some(last_location.clone()),
            play_origin: Some(SessionSource::Playlist(7)),
            play_origin_place: Some(play_origin.clone()),
            ..SessionState::default()
        };

        session::save(&db, &state).unwrap();
        let restored = session::load(&db);

        assert_eq!(restored.sort_field, "year");
        assert_eq!(restored.sort_dir, "desc");
        assert_eq!(restored.play_origin, Some(SessionSource::Playlist(7)));
        assert_eq!(restored.play_origin_place, Some(play_origin));
        assert_eq!(restored.browser_place, Some(last_location.clone()));

        assert_eq!(startup_place(&restored), last_location);
    }
}
