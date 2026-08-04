//! Trigger-free updates for realised rating cells.
//!
//! A `GListModel::items_changed` notification represents replacement, even
//! when emitted as `(position, 1, 1)`. `GtkColumnView` may re-anchor the
//! viewport while replacing that row. Rating-only Tag Editor saves therefore
//! update their cached data and realised `RatingWidget`s through this
//! registry, without emitting a model signal.

use std::rc::Rc;

use gtk4::prelude::*;

use super::now_playing_marker::cell_key;
use super::Shared;

type RatingApplier = Rc<dyn Fn(i32)>;

pub(in crate::ui) struct RatingCellMarker {
    item: gtk4::glib::WeakRef<gtk4::ListItem>,
    track_id: i64,
    apply: RatingApplier,
}

pub(in crate::ui) fn register_cell(
    shared: &Rc<Shared>,
    item: &gtk4::ListItem,
    track_id: i64,
    apply: impl Fn(i32) + 'static,
) {
    shared.register_rating_cell(item, track_id, Rc::new(apply));
}

pub(in crate::ui) fn unregister_cell(shared: &Shared, item: &gtk4::ListItem) {
    shared.unregister_rating_cell(item);
}

impl Shared {
    fn register_rating_cell(&self, item: &gtk4::ListItem, track_id: i64, apply: RatingApplier) {
        let weak = gtk4::glib::WeakRef::new();
        weak.set(Some(item));
        // Keyed insert: rebinding the same cell replaces its entry, which is
        // the dedup the previous linear `retain` performed. Pruning entries
        // whose `ListItem` died moved to `refresh_realised_ratings`, which runs
        // per rating save rather than per bind. A recycled address colliding
        // with a dead entry is harmless — the insert replaces it, and it was
        // dead anyway.
        self.rating_cells
            .borrow_mut()
            .insert(cell_key(item), RatingCellMarker { item: weak, track_id, apply });
    }

    fn unregister_rating_cell(&self, item: &gtk4::ListItem) {
        self.rating_cells.borrow_mut().remove(&cell_key(item));
    }

    pub(in crate::ui) fn refresh_realised_ratings(&self, ratings: &[(i64, i32)]) {
        let appliers: Vec<(i64, RatingApplier)> = {
            let mut markers = self.rating_cells.borrow_mut();
            markers.retain(|_, marker| marker.item.upgrade().is_some());
            markers
                .values()
                .map(|marker| (marker.track_id, marker.apply.clone()))
                .collect()
        };
        for &(track_id, rating) in ratings {
            for (_, apply) in appliers.iter().filter(|(id, _)| *id == track_id) {
                apply(rating);
            }
        }
    }
}
