//! GRID-5 coordination for revealing the loaded album from player surfaces.

use std::rc::Rc;

use crate::ui::nav_history::{NavHistory, NavPlace};
use reprise_core::view_source::ViewSource;

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

/// Records the public Albums destination while suppressing the sidebar's
/// internal Library/Tracks route from becoming a second Back entry.
pub(in crate::ui) fn route_with_history(
    history: &NavHistory,
    target: &NavPlace,
    route: impl FnOnce(),
) {
    history.record_route(target);
    history.begin_back();
    route();
    history.end_back();
}

pub(in crate::ui) fn route_back_restoring_album_focus(
    current_source: &ViewSource,
    target: &NavPlace,
    route: impl FnOnce(),
    focus_album: &AlbumRevealCallback,
) {
    let album = match current_source {
        ViewSource::Album {
            album,
            album_artist,
        } if target.source == ViewSource::Library
            && target.library_tab.as_deref() == Some(super::library_shell::LIBRARY_VIEW_ALBUMS) =>
        {
            Some((album.clone(), album_artist.clone()))
        }
        _ => None,
    };
    route();
    if let Some((album, artist)) = album {
        focus_album(&album, &artist);
    }
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
    fn revealing_albums_deduplicates_sidebar_track_routing_from_history() {
        let albums = NavPlace::source(ViewSource::Library, Some("albums".into()));
        let queue = NavPlace::source(ViewSource::Queue, Some("tracks".into()));
        let history = NavHistory::default();
        history.record_route(&queue);
        route_with_history(&history, &albums, || {
            // `route_to_place` reaches Library through the sidebar, whose
            // canonical route is Tracks before the requested Albums tab is
            // restored. GRID-5 must suppress that internal implementation step.
            history.record_route(&NavPlace::source(
                ViewSource::Library,
                Some("tracks".into()),
            ));
            history.note_library_tab("albums");
        });

        assert_eq!(history.go_back(), Some(queue));
        assert_eq!(history.go_back(), None);

        let already_albums = NavHistory::default();
        already_albums.record_route(&albums);
        route_with_history(&already_albums, &albums, || {
            already_albums.record_route(&NavPlace::source(
                ViewSource::Library,
                Some("tracks".into()),
            ));
            already_albums.note_library_tab("albums");
        });
        assert_eq!(already_albums.go_back(), None);
    }

    #[test]
    fn grid_6_back_from_album_detail_restores_departed_album_focus() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let focus_album: AlbumRevealCallback = {
            let events = events.clone();
            Rc::new(move |album, artist| {
                events.borrow_mut().push(format!("focus:{album}:{artist}"));
                true
            })
        };
        let target = NavPlace::source(ViewSource::Library, Some("albums".into()));
        route_back_restoring_album_focus(
            &ViewSource::Album {
                album: "Kid A".into(),
                album_artist: "Radiohead".into(),
            },
            &target,
            {
                let events = events.clone();
                move || events.borrow_mut().push("route".into())
            },
            &focus_album,
        );

        assert_eq!(&*events.borrow(), &["route", "focus:Kid A:Radiohead"]);
    }
}
