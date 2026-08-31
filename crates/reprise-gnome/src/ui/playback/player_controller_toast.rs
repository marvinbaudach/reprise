//! `PlayerController`'s toast and track-list-reload seam.
//!
//! Split out of `player_controller.rs` so that file stays under the
//! architecture lint's 800-line ceiling. Only the two methods moved: the
//! reasoning for the seam itself stays in that module's
//! `### Toast + track-list-reload seam` doc section, which
//! `playback_faults.rs` also points at.

use super::player_controller::PlayerController;

impl PlayerController {
    /// Shows `text` as an `adw::Toast` on the window's toast overlay, if one
    /// has been wired via `set_toast_overlay` and is still alive — degrades
    /// to a warn log otherwise (never unwraps the `WeakRef` upgrade). See the
    /// module's `## Toast + track-list-reload seam` doc section. `pub(in crate::ui)`
    /// so `playback_faults.rs`'s `handle_unplayable_track`/`skip_after_
    /// failure` can call it too.
    pub(in crate::ui) fn show_toast(&self, text: &str) {
        match self.toast_overlay.upgrade() {
            Some(overlay) => crate::ui::toasts::show(&overlay, text),
            None => {
                tracing::warn!(text, "toast overlay is gone; degrading to log-only");
            }
        }
    }

    /// Calls the track-list reload callback wired via `set_track_list_reload`,
    /// if any — used after `queries::mark_track_missing` so the now-missing
    /// row disappears from the view. Degrades to a warn log if no callback is
    /// wired yet. Borrow discipline: the `Rc<dyn Fn()>` is cloned out of the
    /// `RefCell` in its own `let` statement before being called, mirroring
    /// the `queue` borrow discipline elsewhere in this file (see the
    /// module's `## Toast + track-list-reload seam` doc section for why this
    /// one field can currently never be re-entered, but the hoist keeps the
    /// same shape regardless). `pub(in crate::ui)` so `playback_faults.rs`'s
    /// `handle_unplayable_track` can call it too.
    pub(in crate::ui) fn reload_track_list(&self) {
        let reload = self.reload_track_list.borrow().clone();
        match reload {
            Some(reload) => reload(),
            None => {
                tracing::warn!("track list reload requested but no callback is wired yet");
            }
        }
    }
}
