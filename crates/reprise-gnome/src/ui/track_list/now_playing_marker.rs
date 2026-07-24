//! Trigger-free now-playing marker updates.
//!
//! The track list has no per-row `GObject`; its model hands out fresh
//! `BoxedAnyObject<Track>` clones, so the only way it could push a changed
//! now-playing marker to a visible cell used to be `items_changed(pos, 1, 1)`
//! — a *fake* remove+insert. GtkColumnView reacts to that by replacing the row
//! widget under the pointer/focus and snapping the viewport back to the top:
//! the long-standing "double-clicking a row to play it jumps the table to the
//! top" bug (and the same for the previous-row and stop clears).
//!
//! Instead, each column's `connect_bind` registers a small re-applier closure
//! (keyed by its `ListItem`) that reasserts *this* cell's marker against the
//! current [`Shared::playing_track_id`]. A playback change sets
//! `playing_track_id` and calls [`Shared::reapply_now_playing_markers`], which
//! runs every registered closure — the marker moves to the new row and off the
//! old one by mutating already-realised widgets in place, with no model signal
//! and therefore no viewport jump.
//!
//! No `connect_unbind` counterpart is needed: GTK recycles a bounded pool of
//! `ListItem`s, and every `bind` re-registers, replacing that item's entry
//! (dedup by `ListItem` identity) with the current track. So the registry
//! stays the size of the pool, dead entries self-heal on the next pass, and a
//! stale applier on a pooled-but-unbound (hence invisible) cell only ever
//! toggles a class no one can see — never a visible wrong marker.

use std::rc::Rc;

use gtk4::prelude::*;

use crate::ui::track_list::Shared;

/// Registers `apply` as the now-playing re-applier for `item`'s cell. Call at
/// the end of each column's `connect_bind`, capturing that cell's own widgets
/// and its bound track id; `apply` re-runs this column's marker logic (class,
/// and for the title column the equaliser + bold title) against the current
/// `playing_track_id`. The re-applier holds only a weak reference to `shared`,
/// so it no-ops rather than resurrecting the list once it is gone.
pub(in crate::ui) fn register_cell(
    shared: &Rc<Shared>,
    item: &gtk4::ListItem,
    apply: impl Fn(&Shared) + 'static,
) {
    let weak = Rc::downgrade(shared);
    shared.register_now_playing_marker(
        item,
        Rc::new(move || {
            if let Some(shared) = weak.upgrade() {
                apply(&shared);
            }
        }),
    );
}

/// One registered cell's marker re-applier plus a weak handle to the
/// `ListItem` it is bound to, used both to drop the entry when that item dies
/// and to de-duplicate on rebind.
pub(in crate::ui) struct NowPlayingMarker {
    item: gtk4::glib::WeakRef<gtk4::ListItem>,
    apply: Rc<dyn Fn()>,
}

impl Shared {
    /// Registers (or, on rebind of the same `ListItem`, replaces) the marker
    /// re-applier for a cell. Also drops any entries whose `ListItem` has since
    /// died, so the registry can never grow without bound.
    pub(in crate::ui) fn register_now_playing_marker(
        &self,
        item: &gtk4::ListItem,
        apply: Rc<dyn Fn()>,
    ) {
        let target = item.as_ptr();
        let mut markers = self.now_playing_markers.borrow_mut();
        markers.retain(|marker| {
            marker
                .item
                .upgrade()
                .is_some_and(|live| live.as_ptr() != target)
        });
        let weak = gtk4::glib::WeakRef::new();
        weak.set(Some(item));
        markers.push(NowPlayingMarker { item: weak, apply });
    }

    /// Re-runs every registered cell's marker application against the current
    /// [`Shared::playing_track_id`]. Clones the closures out of the `RefCell`
    /// before invoking any — a closure re-entering the registry (a rebind
    /// triggered mid-apply) must never find the borrow still held.
    pub(in crate::ui) fn reapply_now_playing_markers(&self) {
        let appliers: Vec<Rc<dyn Fn()>> = {
            let mut markers = self.now_playing_markers.borrow_mut();
            markers.retain(|marker| marker.item.upgrade().is_some());
            markers.iter().map(|marker| marker.apply.clone()).collect()
        };
        for apply in appliers {
            apply();
        }
    }

    /// Re-applies the now-playing markers viewport-neutrally: it runs on the
    /// next idle tick (out of the row-activation handler) and pins the scroll
    /// offset across the update.
    ///
    /// Applying the marker mutates the visible now-playing cell (equaliser
    /// shown, bold title). In the release build's timing some GtkColumnView
    /// states re-anchor the viewport when that mutation happens *inside* the
    /// double-click activation — the "double-click-to-play jumps the table to
    /// the top and snaps back" report. Deferring the mutation out of the
    /// activation handler, and restoring the captured scroll value afterwards
    /// (synchronously and once more on idle, since GTK's re-anchor can land a
    /// frame later), keeps the marker from ever moving the list. The
    /// intentional center-on-track reveal (explicit transport / auto-advance)
    /// keeps the plain, synchronous `reapply_now_playing_markers` — it owns the
    /// viewport itself and must not be pinned.
    pub(in crate::ui) fn reapply_now_playing_markers_pinned(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        gtk4::glib::idle_add_local_once(move || {
            let Some(shared) = weak.upgrade() else {
                return;
            };
            let saved = gtk4::prelude::ScrollableExt::vadjustment(&shared.column_view)
                .map(|adjustment| (adjustment.clone(), adjustment.value()));
            shared.reapply_now_playing_markers();
            if let Some((adjustment, value)) = saved {
                adjustment.set_value(value);
                gtk4::glib::idle_add_local_once(move || adjustment.set_value(value));
            }
        });
    }
}
