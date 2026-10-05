//! Assembles the live application session and writes it, once.
//!
//! Two routes end in the same save: the main window's `close_request` and a
//! termination request (see `termination`). Both go through one
//! [`SessionSaver`], so what is persisted cannot drift between them, and the
//! second route to arrive finds the work already done.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use reprise_core::db::Db;
use reprise_core::library::session::{self, SessionState};

use crate::ui::nav_history::NavHistory;
use crate::ui::player_controller::PlayerController;
use crate::ui::session_restore::{apply_view_snapshot, close_should_proceed, geometry_for_save};
use crate::ui::track_list::TrackList;
use crate::ui::view_session;

pub(in crate::ui) struct SessionSaver {
    conn: Rc<Db>,
    track_list: Weak<TrackList>,
    player: Option<Weak<PlayerController>>,
    loaded: SessionState,
    geometry: Rc<Cell<(i32, i32, bool)>>,
    nav_history: Rc<NavHistory>,
    saved: Cell<bool>,
}

impl SessionSaver {
    pub(in crate::ui) fn new(
        conn: &Rc<Db>,
        track_list: &Rc<TrackList>,
        player: Option<&Rc<PlayerController>>,
        loaded: &SessionState,
        geometry: &Rc<Cell<(i32, i32, bool)>>,
        nav_history: &Rc<NavHistory>,
    ) -> Self {
        Self {
            conn: conn.clone(),
            track_list: Rc::downgrade(track_list),
            player: player.map(Rc::downgrade),
            loaded: loaded.clone(),
            geometry: geometry.clone(),
            nav_history: nav_history.clone(),
            saved: Cell::new(false),
        }
    }

    /// Saves the session unless an earlier call already did. `live` is the
    /// window's current `(width, height, maximized)`.
    pub(in crate::ui) fn save_once(&self, live: (i32, i32, bool)) {
        if self.saved.replace(true) {
            return;
        }
        let state = self.assemble(live);
        let result = session::save(&self.conn, &state);
        match &result {
            Ok(()) => tracing::info!(?state, "application session saved"),
            Err(error) => tracing::error!(%error, "could not save application session"),
        }
        debug_assert!(close_should_proceed(result.is_ok()));
    }

    fn assemble(&self, live: (i32, i32, bool)) -> SessionState {
        let mut state = self.loaded.clone();
        let (width, height, maximized) = geometry_for_save(self.geometry.get(), live);
        state.window_width = width;
        state.window_height = height;
        state.maximized = maximized;
        if let Some(track_list) = self.track_list.upgrade() {
            apply_view_snapshot(&mut state, view_session::snapshot(&track_list));
            if let Some((current, library_root)) =
                self.nav_history.session_places(track_list.browser_place())
            {
                state.browser_place = Some(current);
                state.library_root = Some(library_root);
            }
        }
        if let Some(player) = self.player.as_ref().and_then(Weak::upgrade) {
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
        match reprise_core::library::settings::get_library_root(&self.conn) {
            Ok(Some(root)) => session::mark_clean_exit_now(&mut state, root),
            Ok(None) => state.clean_exit = None,
            Err(error) => {
                state.clean_exit = None;
                tracing::warn!(%error, "could not read library root; clean exit will not suppress a startup scan");
            }
        }
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A saver as it stands before the library is up or after the list was
    /// torn down: no live track list, no player.
    fn bare_saver(loaded: SessionState, geometry: (i32, i32, bool)) -> (SessionSaver, Rc<Db>) {
        let conn = Rc::new(crate::test_db::open().unwrap());
        let saver = SessionSaver {
            conn: conn.clone(),
            track_list: Weak::new(),
            player: None,
            loaded,
            geometry: Rc::new(Cell::new(geometry)),
            nav_history: Rc::new(NavHistory::default()),
            saved: Cell::new(false),
        };
        (saver, conn)
    }

    #[test]
    fn start_5a_a_saver_without_a_live_window_content_still_saves_geometry() {
        let loaded = SessionState {
            window_width: 800,
            window_height: 600,
            ..SessionState::default()
        };
        let (saver, conn) = bare_saver(loaded, (987, 654, false));

        saver.save_once((0, 0, false));

        let restored = session::load(&conn);
        assert_eq!((restored.window_width, restored.window_height), (987, 654));
        assert!(!restored.maximized);
    }

    #[test]
    fn start_5a_saving_twice_keeps_the_first_session() {
        let (saver, conn) = bare_saver(SessionState::default(), (800, 600, false));

        saver.save_once((1111, 777, false));
        saver.save_once((500, 400, true));

        let restored = session::load(&conn);
        assert_eq!((restored.window_width, restored.window_height), (1111, 777));
        assert!(!restored.maximized);
    }
}
