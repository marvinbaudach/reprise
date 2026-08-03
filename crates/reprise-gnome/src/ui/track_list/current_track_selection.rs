//! Keeps playing markers synchronized with playback while applying NAV-10a's
//! intent-sensitive viewport policy. Row activation never moves the viewport;
//! explicit transport centers, and automatic advance yields to recent scrolling.
//! Explicit metadata reveals restore selection/focus through `BrowserPlace`.

use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use reprise_core::playback::PlaybackState;
use reprise_core::queries;
use reprise_core::view_source::ViewSource;

use super::player_controller::PlayerController;
use super::track_list::TrackList;
use super::track_list_activation::current_queue_ids;
use crate::ui::scroll_center;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) enum CurrentTrackChange {
    PlaybackStarted,
    AutomaticAdvance,
    ExplicitTransport,
    SessionRestore,
}

const USER_SCROLL_GRACE: Duration = Duration::from_millis(1_500);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrackRevealPolicy {
    MarkerOnly,
    Center,
}

fn reveal_policy(change: CurrentTrackChange, user_scrolling: bool) -> TrackRevealPolicy {
    match change {
        CurrentTrackChange::PlaybackStarted | CurrentTrackChange::SessionRestore => {
            TrackRevealPolicy::MarkerOnly
        }
        CurrentTrackChange::AutomaticAdvance if user_scrolling => TrackRevealPolicy::MarkerOnly,
        CurrentTrackChange::AutomaticAdvance | CurrentTrackChange::ExplicitTransport => {
            TrackRevealPolicy::Center
        }
    }
}

pub(in crate::ui) type OnCurrentTrackChanged = Rc<dyn Fn(i64, Option<usize>, CurrentTrackChange)>;
/// Callback carrying coarse playback-state changes to the track list, which
/// uses them to freeze the now-playing equaliser on pause (via the
/// `.playback-paused` class on the `ColumnView`) and drop the marker on stop.
/// Mirror of `OnCurrentTrackChanged`'s seam — see `wire`.
pub(in crate::ui) type OnPlaybackStateChanged = Rc<dyn Fn(PlaybackState)>;

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

pub(in crate::ui) fn wire(player: Option<&Rc<PlayerController>>, track_list: &Rc<TrackList>) {
    let Some(player) = player else {
        return;
    };
    let track_list_for_current = Rc::downgrade(track_list);
    player.add_on_current_track_changed(move |track_id, queue_position, change| {
        match track_list_for_current.upgrade() {
            Some(track_list) => {
                track_list.update_current_track(track_id, queue_position, change);
            }
            None => tracing::warn!(
                track_id,
                "current-track marker update skipped: track list is gone"
            ),
        }
    });

    // The marker's loop runs faster where the track pushes harder. It reads
    // `swell`, the same slow envelope the cover breathes on — never `kick`:
    // the track list is a surface for reading and for hitting, and a
    // per-beat rate there would be the restlessness rounds 3 and 5 removed.
    {
        let track_list_for_bass = Rc::downgrade(track_list);
        let swell = std::cell::RefCell::new(crate::ui::swell::Swell::default());
        let tempo = std::cell::Cell::new(crate::ui::eq_bars::EqTempo::default());
        let last_us = std::cell::Cell::new(0i64);
        player.add_on_bass_changed(move |_kick, pressure| {
            let Some(track_list) = track_list_for_bass.upgrade() else {
                return;
            };
            let now = gtk4::glib::monotonic_time();
            let previous = last_us.replace(now);
            let dt_s = if previous == 0 {
                0.0
            } else {
                ((now - previous) as f64 / 1_000_000.0).clamp(0.0, 0.25)
            };
            let value = {
                let mut swell = swell.borrow_mut();
                swell.advance(f64::from(pressure), dt_s);
                if crate::ui::motion::animations_enabled() {
                    swell.value()
                } else {
                    swell.value_without_motion()
                }
            };
            let next = crate::ui::eq_bars::tempo_step(value, tempo.get());
            if next != tempo.replace(next) {
                track_list.set_marker_tempo(next);
            }
        });
    }

    let track_list_for_state = Rc::downgrade(track_list);
    player.add_on_playback_state_changed(move |state| {
        if let Some(track_list) = track_list_for_state.upgrade() {
            track_list.on_playback_state(state);
        } else {
            tracing::debug!("playback-state marker skipped: track list is gone");
        }
    });
}

impl PlayerController {
    /// Adds a loaded-track listener. Appends rather than replaces: every
    /// surface that carries the shared playback marker registers here, and
    /// NAV-10a requires all of them to be told, not just the last one to
    /// register.
    pub(in crate::ui) fn add_on_current_track_changed(
        &self,
        callback: impl Fn(i64, Option<usize>, CurrentTrackChange) + 'static,
    ) {
        self.current_track_changed
            .borrow_mut()
            .push(Rc::new(callback));
    }

    pub(in crate::ui) fn notify_current_track_changed(
        &self,
        track_id: i64,
        queue_position: Option<usize>,
        change: CurrentTrackChange,
    ) {
        let callbacks = self.current_track_changed.borrow().clone();
        for callback in callbacks {
            callback(track_id, queue_position, change);
        }
    }

    /// Adds a playback-state listener — the `add_on_current_track_changed`
    /// counterpart for the running/paused half of the marker.
    pub(in crate::ui) fn add_on_bass_changed(&self, callback: impl Fn(f32, f32) + 'static) {
        self.bass_changed.borrow_mut().push(Rc::new(callback));
    }

    pub(in crate::ui) fn add_on_playback_state_changed(
        &self,
        callback: impl Fn(PlaybackState) + 'static,
    ) {
        self.playback_state_changed
            .borrow_mut()
            .push(Rc::new(callback));
    }

    /// Fans a coarse playback-state change out to registered listeners.
    /// Clones callbacks out of their `RefCell`s before invoking — never holds
    /// borrows across calls — per this project's reentrancy discipline.
    pub(in crate::ui) fn notify_playback_state_changed(&self, state: PlaybackState) {
        let callbacks = self.playback_state_changed.borrow().clone();
        for callback in callbacks {
            callback(state);
        }
        let panel_callback = self.now_playing_panel_state_changed.borrow().clone();
        if let Some(callback) = panel_callback {
            callback(state);
        }
    }

    pub(in crate::ui) fn notify_restored_current_track(&self) {
        self.notify_current_track(CurrentTrackChange::SessionRestore);
    }

    pub(in crate::ui) fn notify_current_track(&self, change: CurrentTrackChange) {
        let current = self
            .current_up_next
            .get()
            .and_then(reprise_core::up_next::QueueItem::track_id)
            .or_else(|| self.queue.borrow().current());
        if let Some(track_id) = current {
            self.notify_current_track_changed(track_id, None, change);
        }
    }
}

impl super::Shared {
    /// The ordered track ids of the current source/sort/filter view — the
    /// same list used to locate a row's visible position. Returns an empty
    /// vec (and logs) on a query failure rather than propagating, since every
    /// caller degrades to "leave the marker where it is" on an empty result.
    /// On `Shared` (not `TrackList`) so the reload path can reach it for the
    /// BROWSE-2 view-state restore.
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
        let queue_items = queue_ids
            .iter()
            .copied()
            .map(reprise_core::up_next::QueueItem::Track)
            .collect::<Vec<_>>();
        let result = {
            let conn = &self.conn;
            queries::query_visible_track_ids_browsed(
                conn,
                &source,
                &sort.field,
                &sort.dir,
                &filter,
                &browse,
                &queue_items,
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
        change: CurrentTrackChange,
    ) {
        let ids = self.shared.current_view_ids();
        let is_queue = matches!(*self.shared.source.borrow(), ViewSource::Queue);

        // Every change carries a loaded track, including the session restore:
        // NAV-10a asks for the marker on every visible instance of the
        // *loaded* track, not only the running one.
        self.shared.playing_track_id.set(Some(track_id));
        if change == CurrentTrackChange::SessionRestore {
            // Intentionally set before lookup: the class is inert without a matching row.
            // START-3: a restored track is loaded but not running, so its row
            // must look exactly like a mid-session pause — same marker, same
            // frozen equaliser. `restore_session_queue` fans out a
            // `Stopped` before this runs (session_player.rs), which is why
            // the class is set here and not earlier. The first real `Playing`
            // drops it again (`on_playback_state`).
            self.set_playback_paused(true);
        }
        let Some(position) =
            visible_position_for_track_in_source(&ids, track_id, queue_position, is_queue)
        else {
            // The new track is not in the current view, but the old playing
            // row (if visible) still needs its marker cleared — do it
            // viewport-neutrally so it never nudges the list.
            self.shared.reapply_now_playing_markers_pinned();
            tracing::debug!(
                track_id,
                "current track is not visible in the active table query"
            );
            return;
        };

        let user_scrolling = self
            .shared
            .last_scroll_activity
            .get()
            .is_some_and(|last| last.elapsed() < USER_SCROLL_GRACE);
        match reveal_policy(change, user_scrolling) {
            TrackRevealPolicy::MarkerOnly => {
                // No reveal: the viewport must stay exactly where it is, so the
                // marker update is deferred out of the activation handler and
                // pinned (see `reapply_now_playing_markers_pinned`) — this is
                // the double-click-to-play path that was snapping to the top.
                self.shared.reapply_now_playing_markers_pinned();
                tracing::info!(
                    track_id,
                    position,
                    "table playing marker updated without selection follow"
                );
            }
            TrackRevealPolicy::Center => {
                // The reveal owns the viewport, so apply the marker plainly and
                // then scroll to center the track.
                self.shared.reapply_now_playing_markers();
                if change == CurrentTrackChange::AutomaticAdvance {
                    reveal_automatic_track_position(&self.shared, position, 8);
                } else {
                    reveal_track_position(&self.shared, position, 8);
                }
                tracing::info!(track_id, position, ?change, "current track centered");
            }
        }
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
    /// Sets the marker loop's tempo the same way `set_playback_paused` sets
    /// its frozen state: one class on the `ColumnView`, which is a stable,
    /// non-recycled ancestor. No cell is touched, so this cannot move the
    /// viewport — the failure `now_playing_marker.rs` exists to avoid.
    fn set_marker_tempo(&self, tempo: crate::ui::eq_bars::EqTempo) {
        use crate::ui::eq_bars::{EqTempo, EQ_CALM_CLASS, EQ_DRIVEN_CLASS};
        let view = &self.shared.column_view;
        view.remove_css_class(EQ_CALM_CLASS);
        view.remove_css_class(EQ_DRIVEN_CLASS);
        match tempo {
            EqTempo::Calm => view.add_css_class(EQ_CALM_CLASS),
            EqTempo::Driven => view.add_css_class(EQ_DRIVEN_CLASS),
            EqTempo::Normal => {}
        }
    }

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
        self.shared.playing_track_id.set(None);
        // Drop the marker from whatever visible row still carries it, without
        // nudging the viewport (same reasoning as the double-click path).
        self.shared.reapply_now_playing_markers_pinned();
    }
}

fn reveal_track_position(shared: &Rc<super::track_list::Shared>, position: u32, attempts: u8) {
    let n_rows = track_table_row_count(&shared.column_view);
    if let Some((adjustment, value)) =
        scroll_center::centered_scroll_target(&shared.column_view, n_rows, position)
    {
        shared.scroll_glide.glide_to(&adjustment, value);
        return;
    }
    if attempts == 0 {
        return;
    }
    let shared = Rc::downgrade(shared);
    gtk4::glib::idle_add_local_once(move || {
        if let Some(shared) = shared.upgrade() {
            reveal_track_position(&shared, position, attempts - 1);
        }
    });
}

fn reveal_automatic_track_position(
    shared: &Rc<super::track_list::Shared>,
    position: u32,
    attempts: u8,
) {
    let user_scrolling = shared
        .last_scroll_activity
        .get()
        .is_some_and(|last| last.elapsed() < USER_SCROLL_GRACE);
    if user_scrolling {
        tracing::debug!(
            position,
            "automatic track centering suppressed by scroll activity"
        );
        return;
    }
    let n_rows = track_table_row_count(&shared.column_view);
    if let Some((adjustment, value)) =
        scroll_center::centered_scroll_target(&shared.column_view, n_rows, position)
    {
        shared.scroll_glide.glide_to(&adjustment, value);
        return;
    }
    if attempts == 0 {
        return;
    }
    let shared = Rc::downgrade(shared);
    gtk4::glib::idle_add_local_once(move || {
        if let Some(shared) = shared.upgrade() {
            reveal_automatic_track_position(&shared, position, attempts - 1);
        }
    });
}

#[cfg(test)]
#[path = "current_track_selection_glide_tests.rs"]
mod glide_tests;

#[cfg(test)]
#[path = "current_track_selection_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "start_restore_tests.rs"]
mod start_restore_tests;
