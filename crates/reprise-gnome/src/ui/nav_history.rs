//! NAV-2: the global navigation history. Every routed source switch (the
//! sidebar's `on_select` choke point) and every cross-navigation that
//! bypasses it (album cards, artist deep-links — see `library_shell`)
//! records the place it LEFT; "Back" (Alt+← / mouse back button) pops and
//! re-routes there. A place is the routed `ViewSource` plus which library
//! tab was showing — the visual Albums/Artists grids are tabs of the same
//! `ViewSource::Library`, so the source alone cannot express "the album
//! grid I was looking at". Back itself routes with the suppression flag
//! set, so returning never pushes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use reprise_core::view_source::ViewSource;

/// Upper bound on remembered places. Far beyond any real session's
/// navigation depth; just keeps a pathological click loop from growing
/// the stack forever.
const MAX_HISTORY: usize = 50;

type AvailabilityCallback = Rc<dyn Fn(bool)>;

/// A place the user can return to: the routed source plus the library
/// tab (`library_shell::LIBRARY_VIEW_*`) that was visible there. The tab
/// is semantically meaningful only for sources rendered inside the
/// library tab stack; for Device/MyStats it is carried along but the
/// content-stack switch on the way back makes it invisible.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) struct NavPlace {
    pub(in crate::ui) source: ViewSource,
    pub(in crate::ui) library_tab: Option<String>,
    transient: Option<TransientPlace>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransientPlace {
    NewReleases,
}

impl NavPlace {
    pub(in crate::ui) fn source(source: ViewSource, library_tab: Option<String>) -> Self {
        Self {
            source,
            library_tab,
            transient: None,
        }
    }

    pub(in crate::ui) fn is_new_releases(&self) -> bool {
        self.transient == Some(TransientPlace::NewReleases)
    }
}

#[derive(Default)]
pub(in crate::ui) struct NavHistory {
    stack: RefCell<Vec<NavPlace>>,
    /// Places "ahead" of the current one, populated by `go_back` and
    /// invalidated by any real forward navigation — browser semantics.
    forward: RefCell<Vec<NavPlace>>,
    /// The place currently routed to — the value `record_route` will push
    /// as "the place we left" on the NEXT route. `None` until the first
    /// route after startup (a fresh window has no place to go back to).
    /// Its `library_tab` is kept fresh by `note_library_tab`.
    last: RefCell<Option<NavPlace>>,
    navigating_back: Cell<bool>,
    can_go_back_changed: RefCell<Vec<AvailabilityCallback>>,
}

impl NavHistory {
    pub(in crate::ui) fn connect_can_go_back_changed(&self, callback: impl Fn(bool) + 'static) {
        callback(self.can_go_back());
        self.can_go_back_changed
            .borrow_mut()
            .push(Rc::new(callback));
    }

    fn can_go_back(&self) -> bool {
        !self.stack.borrow().is_empty()
    }

    fn notify_can_go_back_if_changed(&self, was_available: bool) {
        let available = self.can_go_back();
        if available == was_available {
            return;
        }
        let callbacks = self.can_go_back_changed.borrow().clone();
        for callback in callbacks {
            callback(available);
        }
    }

    /// Called from the routing paths with the place being routed TO.
    /// Pushes the previously-routed place (consecutive duplicates and
    /// back/forward re-routes excluded), then remembers `new` as current.
    /// A real navigation also clears the forward stack — like a browser,
    /// where following a link discards the "forward" pages.
    pub(in crate::ui) fn record_route(&self, new: &NavPlace) {
        let was_available = self.can_go_back();
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
        self.forward.borrow_mut().clear();
        let mut stack = self.stack.borrow_mut();
        if stack.last() == Some(&previous) {
            return;
        }
        stack.push(previous);
        if stack.len() > MAX_HISTORY {
            stack.remove(0);
        }
        drop(stack);
        self.notify_can_go_back_if_changed(was_available);
    }

    /// Records a tab-only navigation within the current source — e.g. the
    /// album card's artist deep-link jumping the Albums tab to the Artists
    /// tab. A no-op before the first routed place exists.
    pub(in crate::ui) fn record_tab_route(&self, tab: &str) {
        let current = self.last.borrow().clone();
        let Some(mut place) = current else {
            return;
        };
        place.library_tab = Some(tab.to_owned());
        self.record_route(&place);
    }

    /// Keeps the current place's tab fresh while the user switches library
    /// tabs WITHOUT routing — the Tracks/Albums/Artists switcher is a mode
    /// switch, not a history entry, but the next push must remember the
    /// tab the user actually left. Wired to the library stack's
    /// `visible-child-name` notify in `library_shell`.
    pub(in crate::ui) fn note_library_tab(&self, tab: &str) {
        if let Some(last) = self.last.borrow_mut().as_mut() {
            last.library_tab = Some(tab.to_owned());
        }
    }

    /// Records the row-less New Releases digest over the current routed
    /// source. The retained source/tab is the place Back restores, while the
    /// transient marker makes Forward return to the digest itself.
    pub(in crate::ui) fn record_new_releases(&self) -> Option<NavPlace> {
        let mut place = self.last.borrow().clone()?;
        place.transient = Some(TransientPlace::NewReleases);
        self.record_route(&place);
        Some(place)
    }

    /// Pops the most recent place and remembers the CURRENT place on the
    /// forward stack, so `go_forward` can return here. The caller routes
    /// to the returned place wrapped in `begin_back`/`end_back` (plus a
    /// `record_route` of the target) so the re-route stays silent.
    pub(in crate::ui) fn go_back(&self) -> Option<NavPlace> {
        let was_available = self.can_go_back();
        let target = self.stack.borrow_mut().pop()?;
        let current = self.last.borrow().clone();
        if let Some(current) = current {
            self.forward.borrow_mut().push(current);
        }
        self.notify_can_go_back_if_changed(was_available);
        Some(target)
    }

    /// The inverse of `go_back`: pops the nearest "ahead" place and pushes
    /// the current one back onto the back stack. Same caller contract as
    /// `go_back` (wrap the re-route in `begin_back`/`end_back`).
    pub(in crate::ui) fn go_forward(&self) -> Option<NavPlace> {
        let was_available = self.can_go_back();
        let target = self.forward.borrow_mut().pop()?;
        let current = self.last.borrow().clone();
        if let Some(current) = current {
            let mut stack = self.stack.borrow_mut();
            stack.push(current);
            if stack.len() > MAX_HISTORY {
                stack.remove(0);
            }
        }
        self.notify_can_go_back_if_changed(was_available);
        Some(target)
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
    use std::rc::Rc;

    fn place(source: ViewSource) -> NavPlace {
        NavPlace::source(source, None)
    }

    #[test]
    fn nav_12_back_availability_tracks_history_depth() {
        let nav = NavHistory::default();
        let observed = Rc::new(RefCell::new(Vec::new()));
        let observed_for_callback = observed.clone();
        nav.connect_can_go_back_changed(move |available| {
            observed_for_callback.borrow_mut().push(available);
        });

        nav.record_route(&place(ViewSource::Library));
        nav.record_route(&place(ViewSource::Queue));
        assert_eq!(nav.go_back(), Some(place(ViewSource::Library)));

        assert_eq!(&*observed.borrow(), &[false, true, false]);
    }

    #[test]
    fn routes_push_the_left_place_and_pop_in_reverse_order() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library)); // startup: nothing left yet
        nav.record_route(&place(ViewSource::Queue)); // left Library
        nav.record_route(&place(ViewSource::Playlist(7))); // left Queue
        assert_eq!(nav.go_back(), Some(place(ViewSource::Queue)));
        assert_eq!(nav.go_back(), Some(place(ViewSource::Library)));
        assert_eq!(nav.go_back(), None);
    }

    #[test]
    fn back_navigation_does_not_push() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library));
        nav.record_route(&place(ViewSource::Queue));
        let target = nav.go_back().unwrap();
        nav.begin_back();
        nav.record_route(&target);
        nav.end_back();
        // Returning to Library must not have recorded "left Queue".
        assert_eq!(nav.go_back(), None);
        // …but the next forward route records leaving Library again.
        nav.record_route(&place(ViewSource::Missing));
        assert_eq!(nav.go_back(), Some(place(ViewSource::Library)));
    }

    #[test]
    fn consecutive_duplicates_are_not_stacked() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library));
        nav.record_route(&place(ViewSource::Library));
        nav.record_route(&place(ViewSource::Queue));
        assert_eq!(nav.go_back(), Some(place(ViewSource::Library)));
        assert_eq!(nav.go_back(), None);
    }

    #[test]
    fn history_is_bounded() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library));
        for id in 0..200 {
            nav.record_route(&place(ViewSource::Playlist(id)));
        }
        assert!(nav.stack.borrow().len() <= MAX_HISTORY);
        assert_eq!(nav.go_back(), Some(place(ViewSource::Playlist(198))));
    }

    #[test]
    fn cross_navigation_pushes_the_grid_tab_the_user_left() {
        let nav = NavHistory::default();
        nav.record_route(&NavPlace {
            source: ViewSource::Library,
            library_tab: Some("tracks".into()),
            transient: None,
        });
        // User clicks the Albums tab (mode switch, not a route)…
        nav.note_library_tab("albums");
        // …then opens an album from the grid (cross-navigation).
        nav.record_route(&NavPlace {
            source: ViewSource::Album {
                album: "OK Computer".into(),
                album_artist: "Radiohead".into(),
            },
            library_tab: Some("tracks".into()),
            transient: None,
        });
        // Back must return to the Albums GRID, not the Tracks tab.
        assert_eq!(
            nav.go_back(),
            Some(NavPlace {
                source: ViewSource::Library,
                library_tab: Some("albums".into()),
                transient: None,
            })
        );
    }

    #[test]
    fn artist_deep_link_records_the_tab_it_left() {
        let nav = NavHistory::default();
        nav.record_route(&NavPlace {
            source: ViewSource::Library,
            library_tab: Some("tracks".into()),
            transient: None,
        });
        nav.note_library_tab("albums");
        // Album card's artist link: same source, Albums → Artists tab.
        nav.record_tab_route("artists");
        assert_eq!(
            nav.go_back(),
            Some(NavPlace {
                source: ViewSource::Library,
                library_tab: Some("albums".into()),
                transient: None,
            })
        );
        // The deep-link target became current: the next route pushes it.
        nav.record_route(&place(ViewSource::Queue));
        assert_eq!(
            nav.go_back(),
            Some(NavPlace {
                source: ViewSource::Library,
                library_tab: Some("artists".into()),
                transient: None,
            })
        );
    }

    #[test]
    fn tab_route_before_any_route_is_a_noop() {
        let nav = NavHistory::default();
        nav.record_tab_route("albums");
        assert_eq!(nav.go_back(), None);
    }

    /// Mirrors the real back/forward handlers: route to the returned
    /// target inside the suppression window.
    fn simulate(nav: &NavHistory, target: Option<NavPlace>) -> Option<NavPlace> {
        let target = target?;
        nav.begin_back();
        nav.record_route(&target);
        nav.end_back();
        Some(target)
    }

    #[test]
    fn forward_returns_to_the_place_back_left() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library));
        nav.record_route(&place(ViewSource::Queue));
        assert_eq!(
            simulate(&nav, nav.go_back()),
            Some(place(ViewSource::Library))
        );
        assert_eq!(
            simulate(&nav, nav.go_forward()),
            Some(place(ViewSource::Queue))
        );
        // Ping-pong keeps working.
        assert_eq!(
            simulate(&nav, nav.go_back()),
            Some(place(ViewSource::Library))
        );
        assert_eq!(
            simulate(&nav, nav.go_forward()),
            Some(place(ViewSource::Queue))
        );
    }

    #[test]
    fn a_new_navigation_clears_the_forward_stack() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library));
        nav.record_route(&place(ViewSource::Queue));
        simulate(&nav, nav.go_back());
        // Navigating somewhere new discards the "forward" page — browser
        // semantics.
        nav.record_route(&place(ViewSource::Missing));
        assert_eq!(nav.go_forward(), None);
    }

    #[test]
    fn forward_without_a_back_is_a_noop() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library));
        nav.record_route(&place(ViewSource::Queue));
        assert_eq!(nav.go_forward(), None);
    }

    #[test]
    fn nr_4_digest_is_a_regular_back_forward_place() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library));

        let digest = nav.record_new_releases().unwrap();
        assert!(digest.is_new_releases());
        assert_eq!(
            simulate(&nav, nav.go_back()),
            Some(place(ViewSource::Library))
        );
        assert_eq!(simulate(&nav, nav.go_forward()), Some(digest));
    }
}
