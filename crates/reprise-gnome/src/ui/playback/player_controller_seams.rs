//! Post-construction injection and UI signal seams.
//!
//! ### Toast + track-list-reload seam
//!
//! Both `handle_unplayable_track` (now in `playback_faults.rs`) and this
//! file need to reach widgets the controller doesn't own outright — this is
//! why the two fields below stay here (and their accessor methods are
//! `pub(in crate::ui)`, so `playback_faults.rs` can call through them):
//!
//! - `toast_overlay: glib::WeakRef<adw::ToastOverlay>` — the overlay is built
//!   in `window::build` *after* `PlayerController::new` (it wraps the whole
//!   window), so it can't be a constructor parameter; `set_toast_overlay`
//!   injects it once `window::build` has it. A `WeakRef`, not a strong
//!   reference, so the controller can never keep the window alive past its
//!   natural lifetime; `show_toast` degrades to a log line if the upgrade
//!   ever fails rather than panicking or silently dropping the toast.
//! - `reload_track_list: RefCell<Option<Rc<dyn Fn()>>>` — similarly injected
//!   post-construction via `set_track_list_reload`, since `TrackList` is
//!   also built after the controller. `window::build` supplies a closure
//!   over a `Weak<TrackList>`, never a strong `Rc`: a strong reference back
//!   would be an `Rc` cycle with `TrackList`'s own `Shared.on_activate`,
//!   which already holds a strong `Rc<PlayerController>`. `Rc<dyn Fn()>`
//!   (not `Box`) so `reload_track_list()` can clone it out of the `RefCell`
//!   in one `let` statement before calling it — same hoist-before-calling-
//!   out shape the queue borrows above use, kept for consistency even though
//!   this `RefCell` isn't actually subject to the `## Queue borrow
//!   discipline` hazard: nothing reachable from `reload_track_list()`'s call
//!   can currently call back into it re-entrantly, so there's no live bug
//!   here today, just the same defensive shape.
//!

use std::rc::Rc;

use libadwaita as adw;

use super::player_controller::{PlayerController, VisibleView};

impl PlayerController {
    /// Injects the window's toast overlay, once it exists (see the module's
    /// `## Toast + track-list-reload seam` doc section for why this can't be
    /// a constructor parameter). Stored as a `WeakRef` — `show_toast`
    /// degrades to log-only if the upgrade ever fails.
    pub fn set_toast_overlay(&self, overlay: &adw::ToastOverlay) {
        self.toast_overlay.set(Some(overlay));
    }

    /// Injects the callback that refreshes the track list after a track is
    /// marked missing (see `playback_faults.rs`'s `handle_unplayable_track`),
    /// once the track list exists. `window::build` supplies a closure over a
    /// `Weak<TrackList>`, not a strong `Rc` — see the module's `## Toast +
    /// track-list-reload seam` doc section for why a strong reference here
    /// would leak.
    pub fn set_on_title_click(&self, f: impl Fn() + 'static) {
        self.bar.set_on_title_click(f);
    }

    pub(in crate::ui) fn set_view_refill_provider(
        &self,
        provider: impl Fn() -> VisibleView + 'static,
    ) {
        *self.view_refill_ids.borrow_mut() = Some(Rc::new(provider));
    }

    /// Wires the cover-image click gesture — see `PlayerBar::connect_cover_clicked`.
    pub fn connect_cover_clicked(&self, f: impl Fn() + 'static) {
        self.bar.connect_cover_clicked(f);
    }

    /// Wires the artist-label click gesture — see `PlayerBar::connect_artist_clicked`.
    pub fn connect_artist_clicked(&self, f: impl Fn() + 'static) {
        self.bar.connect_artist_clicked(f);
    }

    pub fn set_track_list_reload(&self, reload: impl Fn() + 'static) {
        *self.reload_track_list.borrow_mut() = Some(Rc::new(reload));
    }

    pub(in crate::ui) fn set_on_listen_event_recorded(&self, callback: impl Fn() + 'static) {
        *self.listen_event_recorded.borrow_mut() = Some(Rc::new(callback));
    }
}
