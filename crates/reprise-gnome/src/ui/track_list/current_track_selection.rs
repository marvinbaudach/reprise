//! Keeps the track-table selection synchronized with playback changes while
//! leaving a user's selection untouched when the playing track is not part
//! of the currently filtered/source-specific view.

use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::playback::PlaybackState;
use reprise_core::queries;
use reprise_core::view_source::ViewSource;

use super::player_controller::PlayerController;
use super::track_list::TrackList;
use super::track_list_activation::current_queue_ids;

pub(super) type OnCurrentTrackChanged = Rc<dyn Fn(i64, Option<usize>)>;
/// Callback carrying coarse playback-state changes to the track list, which
/// uses them to freeze the now-playing equaliser on pause (via the
/// `.playback-paused` class on the `ColumnView`) and drop the marker on stop.
/// Mirror of `OnCurrentTrackChanged`'s seam — see `wire`.
pub(super) type OnPlaybackStateChanged = Rc<dyn Fn(PlaybackState)>;

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

pub(super) fn wire(player: Option<&Rc<PlayerController>>, track_list: &Rc<TrackList>) {
    let Some(player) = player else {
        return;
    };
    let track_list_for_current = Rc::downgrade(track_list);
    player.set_on_current_track_changed(move |track_id, queue_position| {
        match track_list_for_current.upgrade() {
            Some(track_list) => track_list.select_current_track(track_id, queue_position),
            None => tracing::warn!(
                track_id,
                "current-track selection skipped: track list is gone"
            ),
        }
    });

    let track_list_for_state = Rc::downgrade(track_list);
    player.set_on_playback_state_changed(move |state| match track_list_for_state.upgrade() {
        Some(track_list) => track_list.on_playback_state(state),
        None => tracing::debug!("playback-state marker skipped: track list is gone"),
    });

    let weak = Rc::downgrade(player);
    player.set_on_title_click(move || {
        if let Some(controller) = weak.upgrade() {
            controller.notify_restored_current_track();
        }
    });
}

impl PlayerController {
    pub(super) fn set_on_current_track_changed(
        &self,
        callback: impl Fn(i64, Option<usize>) + 'static,
    ) {
        *self.current_track_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn notify_current_track_changed(
        &self,
        track_id: i64,
        queue_position: Option<usize>,
    ) {
        let callback = self.current_track_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(track_id, queue_position);
        }
    }

    pub(super) fn set_on_playback_state_changed(
        &self,
        callback: impl Fn(PlaybackState) + 'static,
    ) {
        *self.playback_state_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn set_on_playback_state_changed_album(
        &self,
        callback: impl Fn(PlaybackState) + 'static,
    ) {
        *self.playback_state_changed_album.borrow_mut() = Some(Rc::new(callback));
    }

    /// Fans a coarse playback-state change out to all registered listeners
    /// (track list + album grid). Clones callbacks out of their `RefCell`s
    /// before invoking — never holds borrows across calls — per this
    /// project's reentrancy discipline.
    pub(super) fn notify_playback_state_changed(&self, state: PlaybackState) {
        let callback = self.playback_state_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(state);
        }
        let album_cb = self.playback_state_changed_album.borrow().clone();
        if let Some(callback) = album_cb {
            callback(state);
        }
    }

    pub(super) fn notify_restored_current_track(&self) {
        let current = self
            .current_up_next
            .get()
            .or_else(|| self.queue.borrow().current());
        if let Some(track_id) = current {
            self.notify_current_track_changed(track_id, None);
        }
    }
}

impl TrackList {
    /// The ordered track ids of the current source/sort/filter view — the
    /// same list used to locate a row's visible position. Returns an empty
    /// vec (and logs) on a query failure rather than propagating, since every
    /// caller degrades to "leave the marker where it is" on an empty result.
    fn current_view_ids(&self) -> Vec<i64> {
        let sort = self.shared.sort.borrow().clone();
        let filter = self.shared.filter.borrow().clone();
        let source = self.shared.source.borrow().clone();
        let browse = self.shared.browse_filter.borrow().clone();
        let queue_ids = if matches!(source, ViewSource::Queue) {
            current_queue_ids(&self.shared)
        } else {
            Vec::new()
        };
        let result = {
            let conn = self.shared.conn.borrow();
            queries::query_track_ids_browsed(
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

    fn select_current_track(&self, track_id: i64, queue_position: Option<usize>) {
        let ids = self.current_view_ids();
        let is_queue = matches!(*self.shared.source.borrow(), ViewSource::Queue);

        // Move the now-playing marker id first, then invalidate the
        // previously-marked row so its `.now-playing` class clears. Querying
        // `ids` fresh on each change (rather than caching a position) keeps
        // this correct across a reload that shifted every row.
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

        let Some(position) =
            visible_position_for_track_in_source(&ids, track_id, queue_position, is_queue)
        else {
            tracing::debug!(
                track_id,
                "current track is not visible in the active table query"
            );
            return;
        };

        // Rebind the new row so its bind takes the marker class, then follow
        // it with the selection (auto-follow, kept by design).
        self.shared.model.invalidate_window_at(position);
        self.shared.selection.select_item(position, true);
        // Defer `scroll_to` to an idle tick rather than calling it inline.
        // During session restore this runs while the window is still being
        // constructed — before the `ColumnView` has completed its first size
        // allocation — and a `scroll_to` issued that early corrupts the row
        // layout near the top of the list, leaving a persistent phantom gap
        // (an unrendered row) that never resolves on scroll. Running it on the
        // next idle lets GTK finish the initial allocation first; for the live
        // track-change path (view already realized) the one-tick delay is
        // imperceptible.
        let column_view = self.shared.column_view.clone();
        gtk4::glib::idle_add_local_once(move || {
            column_view.scroll_to(position, None, gtk4::ListScrollFlags::NONE, None);
        });
        tracing::info!(track_id, position, "table selection followed current track");
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
            let ids = self.current_view_ids();
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

#[cfg(test)]
mod tests {
    use super::*;

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
