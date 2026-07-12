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
    view_session::restore(
        search_entry,
        track_list,
        sidebar,
        window_title,
        search_guard,
        state,
    );
    if let Some(player) = player {
        player.restore_session_queue(state.queue.clone());
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

pub(super) fn wire_close(
    window: &adw::ApplicationWindow,
    conn: &Rc<RefCell<Connection>>,
    track_list: &Rc<TrackList>,
    player: Option<&Rc<PlayerController>>,
    loaded: &SessionState,
) {
    let geometry = Rc::new(Cell::new((loaded.window_width, loaded.window_height)));
    wire_geometry_tracking(window, &geometry);

    let conn = conn.clone();
    let track_list = Rc::downgrade(track_list);
    let player = player.map(Rc::downgrade);
    let loaded = loaded.clone();
    let saved = Cell::new(false);
    let geometry = geometry.clone();
    window.connect_close_request(move |window| {
        if saved.replace(true) {
            return glib::Propagation::Proceed;
        }
        let mut state = loaded.clone();
        let (width, height) = geometry.get();
        state.window_width = width;
        state.window_height = height;
        state.maximized = window.is_maximized();
        if let Some(track_list) = track_list.upgrade() {
            apply_view_snapshot(&mut state, view_session::snapshot(&track_list));
        }
        if let Some(player) = player.as_ref().and_then(std::rc::Weak::upgrade) {
            state.queue = player.session_queue_snapshot();
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

fn wire_geometry_tracking(window: &adw::ApplicationWindow, geometry: &Rc<Cell<(i32, i32)>>) {
    for property in ["width", "height"] {
        let geometry = geometry.clone();
        window.connect_notify_local(Some(property), move |window, _| {
            if !window.is_maximized() && window.width() > 0 && window.height() > 0 {
                geometry.set((window.width(), window.height()));
            }
        });
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
    }
}
