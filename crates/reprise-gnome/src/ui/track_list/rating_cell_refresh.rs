//! Trigger-free updates for realised rating cells.
//!
//! A `GListModel::items_changed` notification represents replacement, even
//! when emitted as `(position, 1, 1)`. `GtkColumnView` may re-anchor the
//! viewport while replacing that row. Rating-only Tag Editor saves therefore
//! update their cached data and realised `RatingWidget`s through this
//! registry, without emitting a model signal.

use std::rc::Rc;

use gtk4::prelude::*;

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
        let target = item.as_ptr();
        let mut markers = self.rating_cells.borrow_mut();
        markers.retain(|marker| {
            marker
                .item
                .upgrade()
                .is_some_and(|live| live.as_ptr() != target)
        });
        let weak = gtk4::glib::WeakRef::new();
        weak.set(Some(item));
        markers.push(RatingCellMarker {
            item: weak,
            track_id,
            apply,
        });
    }

    fn unregister_rating_cell(&self, item: &gtk4::ListItem) {
        let target = item.as_ptr();
        self.rating_cells.borrow_mut().retain(|marker| {
            marker
                .item
                .upgrade()
                .is_some_and(|live| live.as_ptr() != target)
        });
    }

    pub(in crate::ui) fn refresh_realised_ratings(&self, ratings: &[(i64, i32)]) {
        let appliers: Vec<(i64, RatingApplier)> = {
            let mut markers = self.rating_cells.borrow_mut();
            markers.retain(|marker| marker.item.upgrade().is_some());
            markers
                .iter()
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
