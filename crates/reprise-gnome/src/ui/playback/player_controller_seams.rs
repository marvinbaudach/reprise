//! Post-construction injection and UI signal seams.
//!
//! The `### Toast + track-list-reload seam` invariant is documented in
//! `player_controller.rs`, beside the fields it governs.

use std::rc::Rc;

use libadwaita as adw;

use super::player_controller::{PlayerController, VisibleView};

impl PlayerController {
    /// Injects the window's toast overlay, once it exists (see
    /// `player_controller.rs`'s `## Toast + track-list-reload seam` doc section
    /// for why this can't be a constructor parameter). Stored as a `WeakRef` —
    /// `show_toast` degrades to log-only if the upgrade ever fails.
    pub fn set_toast_overlay(&self, overlay: &adw::ToastOverlay) {
        self.toast_overlay.set(Some(overlay));
    }

    /// Injects the callback that refreshes the track list after a track is
    /// marked missing (see `playback_faults.rs`'s `handle_unplayable_track`),
    /// once the track list exists. `window::build` supplies a closure over a
    /// `Weak<TrackList>`, not a strong `Rc` — see `player_controller.rs`'s
    /// `## Toast + track-list-reload seam` doc section for why a strong
    /// reference here would leak.
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
