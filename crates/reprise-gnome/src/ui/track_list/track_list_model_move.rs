//! Consistent intermediate-list projection for a query-backed block move.

use gtk4::gio::prelude::ListModelExt;
use gtk4::glib::subclass::prelude::ObjectSubclassIsExt;

use super::diagnostic_trail::{self, Event};
use super::track_list_model::TrackListModel;

pub(super) const fn intermediate_n_items(total: u32, len: u32) -> u32 {
    total - len
}

pub(super) const fn intermediate_position(position: u32, to: u32, len: u32) -> u32 {
    if position >= to {
        position + len
    } else {
        position
    }
}

/// Emits a block move as removal followed by insertion. The pending-insert
/// overlay exists only across the first signal, when GTK synchronously reads
/// the shorter intermediate list; it is cleared before the insertion signal.
impl TrackListModel {
    pub(super) fn emit_block_move(&self, from: u32, to: u32, len: u32, total: u32) {
        self.imp().state.borrow_mut().pending_insert = Some((to, len));
        let mut overlay = PendingInsertOverlay(self);
        diagnostic_trail::record(Event::ItemsChanged {
            position: from,
            removed: len,
            added: 0,
        });
        self.items_changed(from, len, 0);
        overlay.clear();
        diagnostic_trail::record(Event::ItemsChanged {
            position: to,
            removed: 0,
            added: len,
        });
        self.items_changed(to, 0, len);
        debug_assert_eq!(self.n_items(), total);
    }
}

struct PendingInsertOverlay<'a>(&'a TrackListModel);

impl PendingInsertOverlay<'_> {
    fn clear(&mut self) {
        self.0.imp().state.borrow_mut().pending_insert = None;
    }
}

impl Drop for PendingInsertOverlay<'_> {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intermediate_model_hides_the_pending_insert() {
        assert_eq!(intermediate_n_items(20, 8), 12);
    }

    #[test]
    fn intermediate_positions_skip_the_pending_insert() {
        assert_eq!(intermediate_position(1, 2, 8), 1);
        assert_eq!(intermediate_position(2, 2, 8), 10);
        assert_eq!(intermediate_position(11, 2, 8), 19);
    }
}
