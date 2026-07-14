//! Window, view, and queue session orchestration.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::library::session::{self, SessionSource, SessionState};
use reprise_core::media_integration::MprisPlaybackStatus;
use reprise_core::queue::{QueueSnapshot, Repeat};
use reprise_core::view_source::ViewSource;
use rusqlite::Connection;

use crate::ui::player_controller::PlayerController;
use crate::ui::sidebar::Sidebar;
use crate::ui::track_list::TrackList;
use crate::ui::view_session::{self, SearchRestoreGuard, TrackViewSnapshot};

const SEED_ENV: &str = "REPRISE_SMOKE_SESSION_SEED";
const REPORT_ENV: &str = "REPRISE_SMOKE_SESSION_REPORT";
const PLAY_ENV: &str = "REPRISE_SMOKE_SESSION_PLAY";

pub(super) fn load(conn: &Connection) -> SessionState {
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
                session::load(conn)
            }
        },
        Err(_) => session::load(conn),
    }
}

pub(super) fn apply_initial_geometry(window: &adw::ApplicationWindow, state: &SessionState) {
    if state.maximized {
        window.maximize();
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn restore_runtime(
    search_entry: &gtk4::SearchEntry,
    track_list: &TrackList,
    sidebar: &Sidebar,
    window_title: &adw::WindowTitle,
    search_guard: &SearchRestoreGuard,
    player: Option<&Rc<PlayerController>>,
    state: &SessionState,
) {
    if let Some(player) = player {
        player.restore_session_queue(
            state.queue.clone(),
            state.up_next.clone(),
            state.current_up_next,
        );
    }
    view_session::restore(
        search_entry,
        track_list,
        sidebar,
        window_title,
        search_guard,
        state,
    );
    if let Some(player) = player {
        player.notify_restored_current_track();
        arm_play(player);
    }
    if std::env::var(REPORT_ENV).is_ok() {
        let playback = player.map_or(MprisPlaybackStatus::Stopped, |player| {
            player.session_playback_status()
        });
        tracing::info!(
            ?state,
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
        tracing::info!("{PLAY_ENV}: activating restored play button");
        player.bar.smoke_activate_play_pause();
    });
}

pub(super) fn wire_close(
    window: &adw::ApplicationWindow,
    conn: &Rc<RefCell<Connection>>,
    track_list: &Rc<TrackList>,
    player: Option<&Rc<PlayerController>>,
    loaded: &SessionState,
    geometry_suppressed: &Rc<Cell<bool>>,
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
        }
        if let Some(player) = player.as_ref().and_then(std::rc::Weak::upgrade) {
            state.queue = player.session_queue_snapshot();
            let (up_next, current_up_next) = player.session_up_next_snapshot();
            state.up_next = up_next;
            state.current_up_next = current_up_next;
        }

        let result = session::save(&conn.borrow(), &state);
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
        ViewSource::Playlist(id) => SessionSource::Playlist(id),
        ViewSource::Smart(id) => SessionSource::Smart(id),
        ViewSource::Queue => SessionSource::Queue,
        ViewSource::Missing => SessionSource::Missing,
        ViewSource::ImportErrors => SessionSource::ImportErrors,
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
    Some(SessionState {
        window_width: 1111,
        window_height: 777,
        source: SessionSource::Queue,
        search: "session".into(),
        browse: reprise_core::queries::BrowseFilter {
            genre: Some("Rock".into()),
            artist: Some(String::new()),
            album: None,
        },
        sort_field: "rating".into(),
        sort_dir: "desc".into(),
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
    let current_up_next = fields.next()?.parse().ok()?;
    let pending = parse_smoke_ids(fields.next()?)?;
    if fields.next().is_some() {
        return None;
    }
    let mut up_next = reprise_core::up_next::UpNextQueue::default();
    up_next.append(&pending);
    Some(SessionState {
        source: SessionSource::Queue,
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
    use super::*;

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
        assert_eq!(state.current_up_next, Some(3));
        assert_eq!(state.up_next.ids(), &[4, 5]);
        assert_eq!(state.queue.repeat, Repeat::Off);
        assert!(!state.queue.shuffled);
    }
}
