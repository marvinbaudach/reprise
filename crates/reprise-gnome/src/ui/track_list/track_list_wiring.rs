//! Post-construction callback wiring for [`TrackList`].

use std::path::PathBuf;
use std::rc::Rc;

use super::surface::TrackList;

impl TrackList {
    /// Injects the callback invoked after any context-menu action that
    /// mutates a playlist's membership — see the `Shared::on_playlist_
    /// mutated` doc comment. `window.rs` wires this to `Sidebar::refresh`.
    pub fn set_on_playlist_mutated(&self, callback: impl Fn() + 'static) {
        *self.shared.on_playlist_mutated.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the queue drag-reorder callback (Stage 3 Task 6) — see the
    /// `Shared::on_queue_reorder` doc comment. `window.rs` wires this to
    /// `PlayerController::move_queue_item`.
    pub fn set_on_queue_reorder(
        &self,
        callback: impl Fn(super::queue_row_mapping::QueueReorderOp) -> bool + 'static,
    ) {
        *self.shared.on_queue_reorder.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the sidebar "add to playlist" drag-and-drop callback (Stage 3
    /// Task 6 review finding #1) — see the `Shared::on_sidebar_playlist_drop`
    /// doc comment. `window.rs` wires this to `Sidebar::handle_playlist_drop`.
    pub fn set_on_sidebar_playlist_drop(
        &self,
        callback: impl Fn(i64, &str, &[i64]) -> bool + 'static,
    ) {
        *self.shared.on_sidebar_playlist_drop.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the sidebar "add to queue" drag-and-drop callback — see the
    /// `Shared::on_sidebar_queue_drop` doc comment. `window.rs` wires this to
    /// `Sidebar::handle_queue_drop`.
    pub fn set_on_sidebar_queue_drop(&self, callback: impl Fn(&[i64]) -> bool + 'static) {
        *self.shared.on_sidebar_queue_drop.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the "Rescan library" context-menu action callback (Missing
    /// source, Stage 3 Task 8) — see the `Shared::on_rescan_library` doc
    /// comment. `window.rs` wires this to `trigger_rescan_of_library_root`.
    pub fn set_on_rescan_library(&self, callback: impl Fn() + 'static) {
        *self.shared.on_rescan_library.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the Missing-view mutation callback — see `Shared::on_library_
    /// mutated` for the empty-vs-purged id contract.
    pub fn set_on_library_mutated(&self, callback: impl Fn(&[i64]) + 'static) {
        *self.shared.on_library_mutated.borrow_mut() = Some(Rc::new(callback));
    }

    pub fn remove_missing_with_undo(&self, ids: &[i64]) {
        self.shared.missing_files_view.remove_with_undo(ids);
    }

    pub fn set_on_tags_mutated(&self, callback: impl Fn(&[PathBuf]) + 'static) {
        *self.shared.on_tags_mutated.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn tag_write_gate(&self) -> crate::ui::tag_write_gate::TagWriteGate {
        self.shared.tag_write_gate.clone()
    }

    /// Injects the callback invoked after the ImportErrors panel's own
    /// Retry/Dismiss actions mutate `import_errors` (Stage 3 Task 8) — see
    /// the `Shared::on_import_errors_mutated` doc comment. `window.rs` wires
    /// this to `Sidebar::refresh`.
    pub fn set_on_import_errors_mutated(&self, callback: impl Fn() + 'static) {
        *self.shared.on_import_errors_mutated.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the player controller — injected post-construction via
    /// `TrackList::set_player`, used by the tag-edit flow to refresh
    /// now-playing metadata after successful tag edits.
    pub fn set_player(&self, player: &Rc<crate::ui::player_controller::PlayerController>) {
        *self.shared.player.borrow_mut() = Rc::downgrade(player);
    }
}
