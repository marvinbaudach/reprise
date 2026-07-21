//! Context-menu navigation/action injection seams for `TrackList`, split out
//! of `track_list.rs` to keep that orchestrator under the 600-line UI limit
//! (`scripts/check-architecture.sh`). These four setters wire the unified
//! track context menu's cross-widget actions — "Go to album/artist", the
//! Queue's "Move to top", and "Show in Missing files" — to `window.rs`, using
//! the same `RefCell<Option<Rc<dyn Fn>>>` seam shape as every other
//! `set_on_*` on `TrackList`. The callbacks themselves live on `Shared`
//! (`track_list.rs`); this module owns only their public injection surface.

use std::rc::Rc;

use super::queue_row_mapping::QueueRow;
use super::TrackList;

impl TrackList {
    pub(in crate::ui) fn open_mix_builder_for_target(
        &self,
        target: reprise_core::mix_planner::ProfileTarget,
    ) {
        super::mix_builder::present_target(&self.shared, target);
    }

    /// Injects playback for the exact visible Mix Builder draft order.
    pub fn set_on_play_mix(&self, callback: impl Fn(Vec<i64>) + 'static) {
        *self.shared.on_play_mix.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the Queue "Move to top" callback (CTX-3/N) — `window.rs` wires
    /// this to `PlayerController::move_queue_rows_to_top`. Returns the number
    /// of rows actually moved to the front of Play Next.
    pub fn set_on_queue_move_to_top(&self, callback: impl Fn(&[QueueRow]) -> usize + 'static) {
        *self.shared.on_queue_move_to_top.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the "Go to album" callback (CTX-4) — `window.rs` navigates to
    /// `ViewSource::Album { album, album_artist }` and switches to the Tracks
    /// view.
    pub fn set_on_go_to_album(&self, callback: impl Fn(i64, String, String) + 'static) {
        *self.shared.on_go_to_album.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the "Go to artist" callback (CTX-4) — `window.rs` navigates to
    /// `ViewSource::Artist(album_artist)`.
    pub fn set_on_go_to_artist(&self, callback: impl Fn(i64, String) + 'static) {
        *self.shared.on_go_to_artist.borrow_mut() = Some(Rc::new(callback));
    }

    /// Injects the "Show in Missing files" callback (CTX-8) — `window.rs`
    /// jumps to the Issues/Missing view.
    pub fn set_on_show_missing_files(&self, callback: impl Fn() + 'static) {
        *self.shared.on_show_missing_files.borrow_mut() = Some(Rc::new(callback));
    }
}
