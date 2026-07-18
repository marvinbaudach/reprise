//! GRID-5 coordination for revealing the loaded album from player surfaces.

use std::rc::Rc;

pub(in crate::ui) type RevealCallback = Rc<dyn Fn()>;
pub(in crate::ui) type AlbumRevealCallback = Rc<dyn Fn(&str, &str) -> bool>;

pub(in crate::ui) struct RevealSteps {
    pub current_album: Rc<dyn Fn() -> Option<(String, String)>>,
    pub route_to_albums: Rc<dyn Fn()>,
    pub clear_search: Rc<dyn Fn()>,
    pub reveal_album: AlbumRevealCallback,
    pub fallback_to_track: Rc<dyn Fn()>,
}

pub(in crate::ui) fn coordinator(steps: RevealSteps) -> RevealCallback {
    Rc::new(move || {
        let Some((album, artist)) = (steps.current_album)() else {
            return;
        };
        (steps.route_to_albums)();
        (steps.clear_search)();
        if !(steps.reveal_album)(&album, &artist) {
            (steps.fallback_to_track)();
        }
    })
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;

    #[test]
    fn coordinator_routes_clears_search_and_reveals_before_fallback() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let fallback_calls = Rc::new(Cell::new(0));
        let reveal = coordinator(RevealSteps {
            current_album: Rc::new(|| Some(("Album".into(), "Artist".into()))),
            route_to_albums: {
                let events = events.clone();
                Rc::new(move || events.borrow_mut().push("route"))
            },
            clear_search: {
                let events = events.clone();
                Rc::new(move || events.borrow_mut().push("clear"))
            },
            reveal_album: {
                let events = events.clone();
                Rc::new(move |album, artist| {
                    assert_eq!((album, artist), ("Album", "Artist"));
                    events.borrow_mut().push("reveal");
                    true
                })
            },
            fallback_to_track: {
                let calls = fallback_calls.clone();
                Rc::new(move || calls.set(calls.get() + 1))
            },
        });

        reveal();

        assert_eq!(&*events.borrow(), &["route", "clear", "reveal"]);
        assert_eq!(fallback_calls.get(), 0);
    }

    #[test]
    fn missing_album_invokes_nav_9a_fallback_exactly_once() {
        let fallback_calls = Rc::new(Cell::new(0));
        let reveal = coordinator(RevealSteps {
            current_album: Rc::new(|| Some(("Missing".into(), "Artist".into()))),
            route_to_albums: Rc::new(|| {}),
            clear_search: Rc::new(|| {}),
            reveal_album: Rc::new(|_, _| false),
            fallback_to_track: {
                let calls = fallback_calls.clone();
                Rc::new(move || calls.set(calls.get() + 1))
            },
        });

        reveal();

        assert_eq!(fallback_calls.get(), 1);
    }

    #[test]
    fn idle_without_an_album_is_a_silent_noop() {
        let calls = Rc::new(Cell::new(0));
        let counted = {
            let calls = calls.clone();
            Rc::new(move || calls.set(calls.get() + 1))
        };
        let reveal = coordinator(RevealSteps {
            current_album: Rc::new(|| None),
            route_to_albums: counted.clone(),
            clear_search: counted.clone(),
            reveal_album: Rc::new(|_, _| panic!("idle must not reveal")),
            fallback_to_track: counted,
        });

        reveal();

        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn revealing_albums_deduplicates_the_current_history_place() {
        use reprise_core::view_source::ViewSource;

        use crate::ui::nav_history::{NavHistory, NavPlace};

        let albums = NavPlace {
            source: ViewSource::Library,
            library_tab: Some("albums".into()),
        };
        let queue = NavPlace {
            source: ViewSource::Queue,
            library_tab: Some("tracks".into()),
        };
        let history = NavHistory::default();
        history.record_route(&queue);
        history.record_route(&albums);
        history.record_route(&albums);

        assert_eq!(history.go_back(), Some(queue));
        assert_eq!(history.go_back(), None);
    }
}
