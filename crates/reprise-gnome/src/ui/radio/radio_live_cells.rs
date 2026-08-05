//! Trigger-free live-state updates for the radio table.
//!
//! Two things in a radio row depend on playback rather than on the station
//! record: the shared playing marker plus the `reprise-radio-playing`
//! class) and the "Now playing" title. Both used to reach their cells only
//! because every snapshot rebuilt the whole `ListStore` — which is exactly
//! what moved the selection to row 0 and reset the scroll offset (see
//! [`super::radio_model::RadioModel::replace`]).
//!
//! This is the track list's `now_playing_marker` answer, scoped to radio: each
//! column's `connect_bind` registers a re-applier closure keyed by its
//! `ListItem`, `connect_unbind` drops it again, and a playback change re-runs
//! every registered closure. Cells mutate in place — no `items_changed`, so
//! neither the selection, the focus row nor the viewport can move.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

/// One bound cell's live-state re-applier, plus a weak handle to the
/// `ListItem` it belongs to so dead entries can be pruned.
struct LiveCell {
    item: glib::WeakRef<gtk4::ListItem>,
    apply: Rc<dyn Fn()>,
}

/// The bound cells of one radio table.
#[derive(Default)]
pub(super) struct RadioLiveCells {
    cells: RefCell<HashMap<usize, LiveCell>>,
}

/// Registry key for a cell: the `ListItem`'s address, which is what identifies
/// a cell across rebinds — the same key `now_playing_marker` uses.
fn cell_key(item: &gtk4::ListItem) -> usize {
    item.as_ptr() as usize
}

impl RadioLiveCells {
    /// Registers (or, on rebind of the same `ListItem`, replaces) the live
    /// re-applier for a cell. Call at the end of a column's `connect_bind`,
    /// capturing that cell's own widgets and its bound station id.
    pub(super) fn register(&self, item: &gtk4::ListItem, apply: Rc<dyn Fn()>) {
        let weak = glib::WeakRef::new();
        weak.set(Some(item));
        self.cells
            .borrow_mut()
            .insert(cell_key(item), LiveCell { item: weak, apply });
    }

    /// Drops the entry for `item`'s cell. Required on `connect_unbind`:
    /// `GtkColumnView` keeps unbound `ListItem`s alive in a recycle pool, so
    /// without this the registry — and the cell widgets its closures capture —
    /// would grow with every scroll.
    pub(super) fn unregister(&self, item: &gtk4::ListItem) {
        self.cells.borrow_mut().remove(&cell_key(item));
    }

    /// Re-runs every registered cell's live-state application. The closures
    /// are cloned out of the `RefCell` before any of them runs, so a rebind
    /// triggered mid-apply never finds the borrow still held.
    pub(super) fn reapply(&self) {
        let appliers: Vec<Rc<dyn Fn()>> = {
            let mut cells = self.cells.borrow_mut();
            cells.retain(|_, cell| cell.item.upgrade().is_some());
            cells.values().map(|cell| cell.apply.clone()).collect()
        };
        for apply in appliers {
            apply();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn a_rebind_replaces_the_entry_and_unbind_drops_it() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let cells = RadioLiveCells::default();
        // `GtkListItem` has no public constructor — the list view makes them —
        // but the registry only ever uses it as an identity plus a weak ref.
        let item: gtk4::ListItem = glib::Object::new();

        let runs = Rc::new(Cell::new(0_u32));
        let counter = runs.clone();
        cells.register(&item, Rc::new(move || counter.set(counter.get() + 1)));
        // A rebind of the same cell must not accumulate a second applier.
        let counter = runs.clone();
        cells.register(&item, Rc::new(move || counter.set(counter.get() + 1)));

        cells.reapply();
        assert_eq!(runs.get(), 1);

        cells.unregister(&item);
        cells.reapply();
        assert_eq!(runs.get(), 1);
    }
}
