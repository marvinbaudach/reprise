//! Keeps playing markers synchronized with playback while leaving the user's
//! selection and viewport untouched. NAV-10 reveal paths call the separate
//! centering helper explicitly.

use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::playback::PlaybackState;
use reprise_core::queries;
use reprise_core::view_source::ViewSource;

use super::player_controller::PlayerController;
use super::track_list::TrackList;
use super::track_list_activation::current_queue_ids;
use crate::ui::artist_view::ArtistView;
use crate::ui::scroll_center;

/// `(track_id, queue_position, playback_started)` — `playback_started` moves
/// only the marker; `false` is an explicit/session reveal without selection.
pub(in crate::ui) type OnCurrentTrackChanged = Rc<dyn Fn(i64, Option<usize>, bool)>;
/// Callback carrying coarse playback-state changes to the track list, which
/// uses them to freeze the now-playing equaliser on pause (via the
/// `.playback-paused` class on the `ColumnView`) and drop the marker on stop.
/// Mirror of `OnCurrentTrackChanged`'s seam — see `wire`.
pub(in crate::ui) type OnPlaybackStateChanged = Rc<dyn Fn(PlaybackState)>;
pub(in crate::ui) type OnNowPlayingAlbumChanged = Rc<dyn Fn(Option<(String, String)>)>;

fn visible_position_for_track_in_source(
    ids: &[i64],
    current_id: i64,
    queue_position: Option<usize>,
    is_queue: bool,
) -> Option<u32> {
    if queue_position.is_some_and(|position| ids.get(position) == Some(&current_id)) {
        return queue_position.and_then(|position| u32::try_from(position).ok());
    }
    if is_queue {
        return None;
    }
    ids.iter()
        .position(|candidate| *candidate == current_id)
        .and_then(|position| u32::try_from(position).ok())
}

/// Row count of the track table's current model — the divisor for
/// [`scroll_center::centered_scroll_target`]'s uniform-height row math.
fn track_table_row_count(column_view: &gtk4::ColumnView) -> u32 {
    column_view.model().map_or(0, |model| model.n_items())
}

pub(in crate::ui) fn wire(
    player: Option<&Rc<PlayerController>>,
    track_list: &Rc<TrackList>,
    artist_view: &Rc<ArtistView>,
) {
    let Some(player) = player else {
        return;
    };
    // The two closures below capture a *strong* `Rc<ArtistView>`: the
    // controller outlives `window::build`, so this keeps `ArtistView`'s
    // pure-Rust `Inner` alive past `build()` — which is what makes the Artists
    // tab-switch's `refresh_callback` (a `Weak<Inner>::upgrade`) succeed. No
    // cycle: the artist view's hero play/shuffle/queue closures capture the
    // controller only via `Weak` (see `window::build`), upgrading at call time.
    let track_list_for_current = Rc::downgrade(track_list);
    let player_weak = Rc::downgrade(player);
    {
        let artist_view = artist_view.clone();
        player.set_on_current_track_changed(move |track_id, queue_position, playback_started| {
            match track_list_for_current.upgrade() {
                Some(track_list) => {
                    track_list.update_current_track(track_id, queue_position, playback_started);
                }
                None => tracing::warn!(
                    track_id,
                    "current-track marker update skipped: track list is gone"
                ),
            }
            // Light the Artists view's mini-EQ — only for an actual playback
            // start; a restored-but-stopped track must not glow. The view
            // groups by *album* artist, so resolve the effective album artist
            // for the now-playing track (the same fallback the master rows
            // group by) before handing it over.
            if !playback_started {
                return;
            }
            if let Some(player) = player_weak.upgrade() {
                let album_artist = player.current_track_album_artist();
                artist_view.set_now_playing(album_artist, Some(track_id));
            }
        });
    }

    let track_list_for_state = Rc::downgrade(track_list);
    {
        let artist_view = artist_view.clone();
        player.set_on_playback_state_changed(move |state| {
            if let Some(track_list) = track_list_for_state.upgrade() {
                track_list.on_playback_state(state);
            } else {
                tracing::debug!("playback-state marker skipped: track list is gone");
            }
            // `current_track_changed` never fires on stop, so the Artists
            // mini-EQ is turned off here — the `Stopped` counterpart.
            if matches!(state, PlaybackState::Stopped) {
                artist_view.set_now_playing(None, None);
            }
        });
    }
}

impl PlayerController {
    pub(in crate::ui) fn set_on_current_track_changed(
        &self,
        callback: impl Fn(i64, Option<usize>, bool) + 'static,
    ) {
        *self.current_track_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn notify_current_track_changed(
        &self,
        track_id: i64,
        queue_position: Option<usize>,
        playback_started: bool,
    ) {
        let callback = self.current_track_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(track_id, queue_position, playback_started);
        }
    }

    pub(in crate::ui) fn set_on_playback_state_changed(
        &self,
        callback: impl Fn(PlaybackState) + 'static,
    ) {
        *self.playback_state_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_playback_state_changed_album(
        &self,
        callback: impl Fn(PlaybackState) + 'static,
    ) {
        *self.playback_state_changed_album.borrow_mut() = Some(Rc::new(callback));
    }

    /// Fans a coarse playback-state change out to all registered listeners
    /// (track list + album grid). Clones callbacks out of their `RefCell`s
    /// before invoking — never holds borrows across calls — per this
    /// project's reentrancy discipline.
    pub(in crate::ui) fn notify_playback_state_changed(&self, state: PlaybackState) {
        let callback = self.playback_state_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(state);
        }
        let album_cb = self.playback_state_changed_album.borrow().clone();
        if let Some(callback) = album_cb {
            callback(state);
        }
        let panel_callback = self.now_playing_panel_state_changed.borrow().clone();
        if let Some(callback) = panel_callback {
            callback(state);
        }
    }

    pub(in crate::ui) fn notify_restored_current_track(&self) {
        let current = self
            .current_up_next
            .get()
            .or_else(|| self.queue.borrow().current());
        if let Some(track_id) = current {
            // `false`: this is an explicit/session reveal, not a playback
            // start. It centers without selecting and leaves the marker off.
            self.notify_current_track_changed(track_id, None, false);
        }
    }
}

impl super::Shared {
    /// The ordered track ids of the current source/sort/filter view — the
    /// same list used to locate a row's visible position. Returns an empty
    /// vec (and logs) on a query failure rather than propagating, since every
    /// caller degrades to "leave the marker where it is" on an empty result.
    /// On `Shared` (not `TrackList`) so the reload path can reach it for the
    /// NAV-5 view-state restore.
    pub(in crate::ui) fn current_view_ids(&self) -> Vec<i64> {
        let sort = self.sort.borrow().clone();
        let filter = self.filter.borrow().clone();
        let source = self.source.borrow().clone();
        let browse = self.browse_filter.borrow().clone();
        let queue_ids = if matches!(source, ViewSource::Queue) {
            current_queue_ids(self)
        } else {
            Vec::new()
        };
        let result = {
            let conn = self.conn.borrow();
            queries::query_visible_track_ids_browsed(
                &conn,
                &source,
                &sort.field,
                &sort.dir,
                &filter,
                &browse,
                &queue_ids,
            )
        };
        result.unwrap_or_else(|error| {
            tracing::error!(%error, "failed to query current view ids for now-playing marker");
            Vec::new()
        })
    }
}

impl TrackList {
    /// Track ids for the transport's end-of-queue refill (see
    /// `PlayerController::set_view_refill_provider`): the visible view's
    /// full id list, or empty when the Queue view itself is showing —
    /// refilling the exhausted queue from its own (exhausted) contents would
    /// just loop it, overriding the user's repeat setting.
    pub fn transport_refill_ids(&self) -> Vec<i64> {
        if matches!(*self.shared.source.borrow(), ViewSource::Queue) {
            return Vec::new();
        }
        self.shared.current_view_ids()
    }

    fn update_current_track(
        &self,
        track_id: i64,
        queue_position: Option<usize>,
        playback_started: bool,
    ) {
        let ids = self.shared.current_view_ids();
        let is_queue = matches!(*self.shared.source.borrow(), ViewSource::Queue);

        if playback_started {
            let previous_id = self.shared.playing_track_id.replace(Some(track_id));
            if let Some(previous_id) = previous_id.filter(|id| *id != track_id) {
                if let Some(old_pos) = ids
                    .iter()
                    .position(|candidate| *candidate == previous_id)
                    .and_then(|position| u32::try_from(position).ok())
                {
                    self.shared.model.invalidate_window_at(old_pos);
                }
            }
        }

        let Some(position) =
            visible_position_for_track_in_source(&ids, track_id, queue_position, is_queue)
        else {
            tracing::debug!(
                track_id,
                "current track is not visible in the active table query"
            );
            return;
        };

        if playback_started {
            self.shared.model.invalidate_window_at(position);
            tracing::info!(
                track_id,
                position,
                "table playing marker updated without selection follow"
            );
        } else {
            reveal_track_position(&self.shared.column_view, position, 8);
            tracing::info!(
                track_id,
                position,
                "explicit current-track reveal centered without selection"
            );
        }
    }

    pub(in crate::ui) fn reveal_playing_context(&self) -> bool {
        let Some(track_id) = self.shared.playing_track_id.get() else {
            return false;
        };
        let ids = self.shared.current_view_ids();
        let is_queue = matches!(*self.shared.source.borrow(), ViewSource::Queue);
        let Some(position) = visible_position_for_track_in_source(&ids, track_id, None, is_queue)
        else {
            return false;
        };
        reveal_track_position(&self.shared.column_view, position, 8);
        true
    }

    /// Reacts to a coarse playback-state change: freeze the now-playing
    /// equaliser on pause, resume it on play, drop the marker on stop.
    fn on_playback_state(&self, state: PlaybackState) {
        match state {
            PlaybackState::Playing => self.set_playback_paused(false),
            PlaybackState::Paused => self.set_playback_paused(true),
            PlaybackState::Stopped => {
                self.set_playback_paused(false);
                self.clear_now_playing();
            }
        }
    }

    /// Toggles the `.playback-paused` class on the `ColumnView`. The animated
    /// equaliser's keyframes are scoped under it (see `eq_bars.rs`), so one
    /// class on a stable, non-recycled ancestor freezes every visible
    /// now-playing equaliser at once — no per-cell bookkeeping.
    fn set_playback_paused(&self, paused: bool) {
        if paused {
            self.shared.column_view.add_css_class("playback-paused");
        } else {
            self.shared.column_view.remove_css_class("playback-paused");
        }
    }

    /// Clears the now-playing marker (on stop) and rebinds the row that
    /// carried it so its `.now-playing` class drops.
    fn clear_now_playing(&self) {
        let previous = self.shared.playing_track_id.replace(None);
        if let Some(previous) = previous {
            let ids = self.shared.current_view_ids();
            if let Some(position) = ids
                .iter()
                .position(|candidate| *candidate == previous)
                .and_then(|position| u32::try_from(position).ok())
            {
                self.shared.model.invalidate_window_at(position);
            }
        }
    }
}

fn reveal_track_position(column_view: &gtk4::ColumnView, position: u32, attempts: u8) {
    let n_rows = track_table_row_count(column_view);
    if let Some((adjustment, value)) =
        scroll_center::centered_scroll_target(column_view, n_rows, position)
    {
        adjustment.set_value(value);
        return;
    }
    if attempts == 0 {
        return;
    }
    let column_view = column_view.clone();
    gtk4::glib::idle_add_local_once(move || {
        reveal_track_position(&column_view, position, attempts - 1);
    });
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use rusqlite::Connection;

    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nav_10_playback_marker_does_not_move_selection_or_viewport() {
        gtk4::init().unwrap();
        let mut conn = Connection::open_in_memory().unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let tx = conn.transaction().unwrap();
        for id in 1..=100 {
            tx.execute(
                "INSERT INTO tracks (id, path, title, artist, added_at) \
                 VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
                (
                    id,
                    format!("/synthetic/{id:03}.flac"),
                    format!("Track {id:03}"),
                ),
            )
            .unwrap();
        }
        tx.commit().unwrap();
        let track_list = TrackList::new(
            Rc::new(RefCell::new(conn)),
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            super::super::queue_sections::QueueViewModel::default,
            crate::ui::cover_download_worker::setup_for_test(),
        );
        let window = gtk4::Window::builder()
            .default_width(900)
            .default_height(320)
            .child(track_list.widget())
            .build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        let position = 60;
        let track_id = track_list.shared.model.track_at(position).unwrap().id;
        track_list
            .shared
            .column_view
            .scroll_to(position, None, gtk4::ListScrollFlags::FOCUS, None);
        let adjustment = track_list.shared.column_view.vadjustment().unwrap();
        // `scroll_to` settles over later main-loop turns, so pumping once is not
        // enough to establish the precondition. This is test setup, not the
        // behaviour under test: wait until the viewport actually moved.
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1000);
        while adjustment.value() <= 0.0 && std::time::Instant::now() < deadline {
            gtk4::glib::MainContext::default().iteration(false);
        }
        let before = adjustment.value();
        assert!(
            before > 0.0,
            "precondition: the list must be scrolled away from the top"
        );
        track_list.shared.selection.select_item(10, true);
        track_list.update_current_track(track_id, None, true);
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert!((adjustment.value() - before).abs() < 0.5);
        assert!(track_list.shared.selection.is_selected(10));
        assert!(!track_list.shared.selection.is_selected(position));
        window.close();
    }

    #[test]
    fn visible_position_finds_the_current_track_in_view_order() {
        assert_eq!(
            visible_position_for_track_in_source(&[41, 42, 43], 42, None, false),
            Some(1)
        );
    }

    #[test]
    fn visible_position_uses_queue_occurrence_then_falls_back_to_first_match() {
        assert_eq!(
            visible_position_for_track_in_source(&[7, 8, 7], 7, Some(2), false),
            Some(2)
        );
        assert_eq!(
            visible_position_for_track_in_source(&[7, 8, 7], 7, Some(1), false),
            Some(0)
        );
        assert_eq!(
            visible_position_for_track_in_source(&[7, 8, 7], 9, None, false),
            None
        );
    }

    #[test]
    fn queue_does_not_highlight_a_pending_duplicate_of_the_current_track() {
        assert_eq!(
            visible_position_for_track_in_source(&[7, 8, 7], 7, None, true),
            None
        );
        assert_eq!(
            visible_position_for_track_in_source(&[7, 8, 7], 7, None, false),
            Some(0)
        );
    }
}
