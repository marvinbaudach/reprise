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
use crate::ui::artist_view::ArtistView;

/// `(track_id, queue_position, playback_started)` — `playback_started` is
/// `true` only when an actual playback start fired the change. Session
/// restore and player-bar title clicks re-select the row with `false`, so
/// they never light the now-playing equaliser while nothing is audible.
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

/// Consumes the one-shot "don't scroll on the next follow" marker
/// `activate_track` arms when the user starts playback from the table
/// itself (double-click/Enter/queue activation): the activated row is
/// already on screen under the pointer, so centering it would visibly
/// yank the viewport. Returns `true` (suppress centering) only when the
/// armed id matches the track the follow is about to select. Always
/// `take`s — a stale id from an activation that never reached playback
/// (unplayable file, empty queue) is discarded on the next change instead
/// of suppressing some later, unrelated auto-advance scroll.
fn take_scroll_suppression(suppressed: &std::cell::Cell<Option<i64>>, track_id: i64) -> bool {
    suppressed.take() == Some(track_id)
}

/// Adjustment value that vertically centers row `position` in the viewport.
/// Assumes uniform row heights (true for the track table), so a row's offset
/// is derived from the adjustment's total content height. Returns `None`
/// when the list is not yet allocated (`upper`/`page_size` unset) or fits
/// entirely in the viewport — in both cases there is nothing to center.
fn centered_scroll_value(position: u32, n_rows: u32, upper: f64, page_size: f64) -> Option<f64> {
    if n_rows == 0 || upper <= 0.0 || page_size <= 0.0 || upper <= page_size {
        return None;
    }
    let row_height = upper / f64::from(n_rows);
    let target = (f64::from(position) + 0.5) * row_height - page_size / 2.0;
    Some(target.clamp(0.0, upper - page_size))
}

/// Resolves the adjustment + value that would center row `position`, or
/// `None` when the list has no usable geometry (not yet allocated, or it
/// fits the viewport entirely).
fn centered_scroll_target(
    column_view: &gtk4::ColumnView,
    position: u32,
) -> Option<(gtk4::Adjustment, f64)> {
    let n_rows = column_view.model().map_or(0, |model| model.n_items());
    let adjustment = gtk4::prelude::ScrollableExt::vadjustment(column_view)?;
    let value =
        centered_scroll_value(position, n_rows, adjustment.upper(), adjustment.page_size())?;
    Some((adjustment, value))
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
                    track_list.select_current_track(track_id, queue_position, playback_started);
                }
                None => tracing::warn!(
                    track_id,
                    "current-track selection skipped: track list is gone"
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
    }

    pub(in crate::ui) fn notify_restored_current_track(&self) {
        let current = self
            .current_up_next
            .get()
            .or_else(|| self.queue.borrow().current());
        if let Some(track_id) = current {
            // `false`: this is a selection restore, not a playback start —
            // the row is selected and centered, but the now-playing marker
            // (equaliser) stays off until real playback fires the callback.
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

    fn select_current_track(
        &self,
        track_id: i64,
        queue_position: Option<usize>,
        playback_started: bool,
    ) {
        // Consumed unconditionally, before any early return, so the one-shot
        // marker never outlives the track change it was armed for.
        let suppress_scroll =
            take_scroll_suppression(&self.shared.suppress_follow_scroll, track_id);
        let ids = self.shared.current_view_ids();
        let is_queue = matches!(*self.shared.source.borrow(), ViewSource::Queue);

        // Move the now-playing marker id first, then invalidate the
        // previously-marked row so its `.now-playing` class clears. Querying
        // `ids` fresh on each change (rather than caching a position) keeps
        // this correct across a reload that shifted every row. Restore/title-
        // click re-selection (`playback_started == false`) leaves the marker
        // untouched: nothing is audible, so no row may show the equaliser.
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

        // Rebind the new row so its bind takes the marker class, then follow
        // it with the selection (auto-follow, kept by design).
        self.shared.model.invalidate_window_at(position);
        self.shared.selection.select_item(position, true);
        // A table-originated activation (double-click/Enter — see
        // `take_scroll_suppression` above) selects without centering: the
        // clicked row is already visible, and yanking the viewport to
        // mid-center it read as a glitch. Every other origin (auto-advance,
        // transport skips, player-bar title click, session restore) still
        // centers below.
        if suppress_scroll {
            tracing::info!(
                track_id,
                position,
                "table selection followed current track (centering skipped: table activation)"
            );
            return;
        }
        // Centering happens DIRECTLY through the vadjustment and — when the
        // list already has usable geometry (the live path: an auto-advanced
        // row) — SYNCHRONOUSLY, in the same main-loop iteration as the
        // invalidate/select above. Both halves matter:
        //
        // - Direct adjustment write: `scroll_to` only edge-snaps, so an
        //   earlier two-phase version (snap, then center) read as two jumps.
        // - Same frame: the `items_changed` above recreates the focused row
        //   widget, and GTK's focus restore can scroll on its own; a deferred
        //   (idle) centering let that intermediate jump render for one frame
        //   ("scrolls up, then back down"). Centering before the next frame
        //   leaves the row visible mid-viewport, so the focus restore has
        //   nothing left to scroll.
        //
        // The idle + `scroll_to` fallback only remains for a list with no
        // geometry yet (session restore during window construction — where a
        // scroll issued inline used to corrupt the row layout, and nothing is
        // on screen anyway, so the two-phase motion is invisible).
        let column_view = self.shared.column_view.clone();
        match centered_scroll_target(&column_view, position) {
            Some((adjustment, value)) => adjustment.set_value(value),
            None => {
                gtk4::glib::idle_add_local_once(move || {
                    match centered_scroll_target(&column_view, position) {
                        Some((adjustment, value)) => adjustment.set_value(value),
                        // Still no usable geometry (or the list fits the
                        // viewport): make the row visible; nothing to center.
                        None => {
                            column_view.scroll_to(
                                position,
                                None,
                                gtk4::ListScrollFlags::NONE,
                                None,
                            );
                        }
                    }
                });
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_activation_suppresses_centering_for_the_activated_track_once() {
        let armed = std::cell::Cell::new(Some(7));
        // The follow for the activated track consumes the marker: no scroll.
        assert!(take_scroll_suppression(&armed, 7));
        // The next change (auto-advance) centers again — the marker is gone.
        assert!(!take_scroll_suppression(&armed, 7));
    }

    #[test]
    fn stale_suppression_from_a_dead_activation_never_hits_a_later_track() {
        let armed = std::cell::Cell::new(Some(7));
        // A different track change (the activation never reached playback,
        // e.g. unplayable file → auto-skip elsewhere) still centers …
        assert!(!take_scroll_suppression(&armed, 9));
        // … and clears the stale marker, so the armed track centers later too.
        assert_eq!(armed.get(), None);
        assert!(!take_scroll_suppression(&armed, 7));
    }

    #[test]
    fn centered_scroll_value_centers_a_mid_list_row() {
        // 100 rows x 10px = 1000px content, 200px viewport. Row 50's middle
        // sits at 505px; centering puts the viewport at 505 - 100 = 405.
        assert_eq!(centered_scroll_value(50, 100, 1000.0, 200.0), Some(405.0));
    }

    #[test]
    fn centered_scroll_value_clamps_at_both_list_edges() {
        assert_eq!(centered_scroll_value(0, 100, 1000.0, 200.0), Some(0.0));
        assert_eq!(centered_scroll_value(99, 100, 1000.0, 200.0), Some(800.0));
    }

    #[test]
    fn centered_scroll_value_skips_unallocated_or_short_lists() {
        // Not yet allocated: no geometry to work with.
        assert_eq!(centered_scroll_value(5, 100, 0.0, 0.0), None);
        // Whole list fits in the viewport: nothing to scroll.
        assert_eq!(centered_scroll_value(5, 10, 100.0, 200.0), None);
        // Empty model.
        assert_eq!(centered_scroll_value(0, 0, 1000.0, 200.0), None);
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
