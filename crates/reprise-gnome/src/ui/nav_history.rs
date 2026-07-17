//! NAV-2: the global navigation history. Every routed source switch (the
//! sidebar's `on_select` — the single choke point all switches flow
//! through, including NAV-9 jumps) records the place it LEFT; "Back"
//! (Alt+←) pops and re-routes there. Back itself routes with the
//! suppression flag set, so returning never pushes.

use std::cell::{Cell, RefCell};

use reprise_core::view_source::ViewSource;

/// Upper bound on remembered places. Far beyond any real session's
/// navigation depth; just keeps a pathological click loop from growing
/// the stack forever.
const MAX_HISTORY: usize = 50;

#[derive(Default)]
pub(in crate::ui) struct NavHistory {
    stack: RefCell<Vec<ViewSource>>,
    /// The source currently routed to — the value `record_route` will push
    /// as "the place we left" on the NEXT route. `None` until the first
    /// route after startup (a fresh window has no place to go back to).
    last: RefCell<Option<ViewSource>>,
    navigating_back: Cell<bool>,
}

impl NavHistory {
    /// Called from the routing choke point with the source being routed TO.
    /// Pushes the previously-routed source (consecutive duplicates and
    /// back-navigation re-routes excluded), then remembers `new` as current.
    pub(in crate::ui) fn record_route(&self, new: &ViewSource) {
        let previous = self.last.borrow_mut().replace(new.clone());
        if self.navigating_back.get() {
            return;
        }
        let Some(previous) = previous else {
            return;
        };
        if previous == *new {
            return;
        }
        let mut stack = self.stack.borrow_mut();
        if stack.last() == Some(&previous) {
            return;
        }
        stack.push(previous);
        if stack.len() > MAX_HISTORY {
            stack.remove(0);
        }
    }

    /// Pops the most recent place. The caller routes there wrapped in
    /// `begin_back`/`end_back` so the resulting `record_route` stays silent.
    pub(in crate::ui) fn pop(&self) -> Option<ViewSource> {
        self.stack.borrow_mut().pop()
    }

    pub(in crate::ui) fn begin_back(&self) {
        self.navigating_back.set(true);
    }

    pub(in crate::ui) fn end_back(&self) {
        self.navigating_back.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_push_the_left_place_and_pop_in_reverse_order() {
        let nav = NavHistory::default();
        nav.record_route(&ViewSource::Library); // startup: nothing left yet
        nav.record_route(&ViewSource::Queue); // left Library
        nav.record_route(&ViewSource::Playlist(7)); // left Queue
        assert_eq!(nav.pop(), Some(ViewSource::Queue));
        assert_eq!(nav.pop(), Some(ViewSource::Library));
        assert_eq!(nav.pop(), None);
    }

    #[test]
    fn back_navigation_does_not_push() {
        let nav = NavHistory::default();
        nav.record_route(&ViewSource::Library);
        nav.record_route(&ViewSource::Queue);
        let target = nav.pop().unwrap();
        nav.begin_back();
        nav.record_route(&target);
        nav.end_back();
        // Returning to Library must not have recorded "left Queue".
        assert_eq!(nav.pop(), None);
        // …but the next forward route records leaving Library again.
        nav.record_route(&ViewSource::Missing);
        assert_eq!(nav.pop(), Some(ViewSource::Library));
    }

    #[test]
    fn consecutive_duplicates_are_not_stacked() {
        let nav = NavHistory::default();
        nav.record_route(&ViewSource::Library);
        nav.record_route(&ViewSource::Library);
        nav.record_route(&ViewSource::Queue);
        assert_eq!(nav.pop(), Some(ViewSource::Library));
        assert_eq!(nav.pop(), None);
    }

    #[test]
    fn history_is_bounded() {
        let nav = NavHistory::default();
        nav.record_route(&ViewSource::Library);
        for id in 0..200 {
            nav.record_route(&ViewSource::Playlist(id));
        }
        assert!(nav.stack.borrow().len() <= MAX_HISTORY);
        assert_eq!(nav.pop(), Some(ViewSource::Playlist(198)));
    }
}
