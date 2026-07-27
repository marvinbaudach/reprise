//! Compatibility edge around the canonical core browser router.
//!
//! GTK routing still speaks in `NavPlace` while the browser migration is in
//! progress, but all current-place and Back/Forward state lives in
//! [`BrowserNavigation`]. Album, artist, and genre destinations are ordinary scoped
//! track places; there is no parallel library-tab history.

use std::cell::{Cell, RefCell};

use reprise_core::browser::navigation::{BrowserNavigation, NavigationIntent, SidebarTarget};
use reprise_core::browser::{AlbumKey, ArtistKey, BrowserPlace};
use reprise_core::view_source::ViewSource;

#[derive(Clone, Debug, PartialEq)]
pub(in crate::ui) struct NavPlace {
    browser: BrowserPlace,
}

impl NavPlace {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(in crate::ui) fn browser(browser: BrowserPlace) -> Self {
        Self { browser }
    }

    pub(in crate::ui) fn source(source: ViewSource) -> Self {
        Self {
            browser: BrowserPlace::from(source),
        }
    }

    pub(in crate::ui) fn view_source(&self) -> ViewSource {
        self.browser.view_source()
    }

    pub(in crate::ui) fn browser_place(&self) -> &BrowserPlace {
        &self.browser
    }
}

#[derive(Default)]
pub(in crate::ui) struct NavHistory {
    navigation: RefCell<Option<BrowserNavigation>>,
    replaying_history: Cell<bool>,
}

impl NavHistory {
    /// Seeds the router on the first call, then records an absolute user
    /// navigation through the canonical core state machine.
    pub(in crate::ui) fn record_route(&self, new: &NavPlace) {
        if self.replaying_history.get() {
            return;
        }
        let mut navigation = self.navigation.borrow_mut();
        let Some(router) = navigation.as_mut() else {
            *navigation = Some(BrowserNavigation::new(new.browser.clone()));
            return;
        };
        let _ = router.navigate(intent_for(&new.browser));
    }

    pub(in crate::ui) fn restore(&self, current: BrowserPlace, library_root: BrowserPlace) {
        *self.navigation.borrow_mut() = Some(BrowserNavigation::restore(current, library_root));
        self.replaying_history.set(false);
    }

    pub(in crate::ui) fn session_places(
        &self,
        visible_track_place: BrowserPlace,
    ) -> Option<(BrowserPlace, BrowserPlace)> {
        self.replace_current(visible_track_place);
        let navigation = self.navigation.borrow();
        let navigation = navigation.as_ref()?;
        Some((
            navigation.current().clone(),
            navigation.library_root().clone(),
        ))
    }

    pub(in crate::ui) fn record_route_from(&self, new: &NavPlace, current: BrowserPlace) {
        self.replace_current(current);
        self.record_route(new);
    }

    pub(in crate::ui) fn navigate_from(
        &self,
        intent: NavigationIntent,
        current: BrowserPlace,
    ) -> Option<NavPlace> {
        self.replace_current(current);
        let transition = self.navigation.borrow_mut().as_mut()?.navigate(intent)?;
        Some(NavPlace {
            browser: transition.to,
        })
    }

    pub(in crate::ui) fn go_back(&self) -> Option<NavPlace> {
        let transition = self
            .navigation
            .borrow_mut()
            .as_mut()?
            .navigate(NavigationIntent::Back)?;
        Some(NavPlace {
            browser: transition.to,
        })
    }

    pub(in crate::ui) fn go_back_from(&self, current: BrowserPlace) -> Option<NavPlace> {
        self.replace_current(current);
        self.go_back()
    }

    pub(in crate::ui) fn go_forward(&self) -> Option<NavPlace> {
        let transition = self
            .navigation
            .borrow_mut()
            .as_mut()?
            .navigate(NavigationIntent::Forward)?;
        Some(NavPlace {
            browser: transition.to,
        })
    }

    pub(in crate::ui) fn go_forward_from(&self, current: BrowserPlace) -> Option<NavPlace> {
        self.replace_current(current);
        self.go_forward()
    }

    pub(in crate::ui) fn begin_back(&self) {
        self.replaying_history.set(true);
    }

    pub(in crate::ui) fn end_back(&self) {
        self.replaying_history.set(false);
    }

    fn replace_current(&self, current: BrowserPlace) {
        if let Some(router) = self.navigation.borrow_mut().as_mut() {
            let _ = router.replace_current(current);
        }
    }
}

fn intent_for(place: &BrowserPlace) -> NavigationIntent {
    match place {
        BrowserPlace::Tracks(track_place) => match &track_place.collection {
            reprise_core::browser::TrackCollection::Library(
                reprise_core::browser::LibraryScope::All,
            ) => NavigationIntent::Sidebar(SidebarTarget::Music),
            reprise_core::browser::TrackCollection::Library(
                reprise_core::browser::LibraryScope::RecentlyAdded,
            ) => NavigationIntent::Sidebar(SidebarTarget::RecentlyAdded),
            reprise_core::browser::TrackCollection::Library(
                reprise_core::browser::LibraryScope::Album(key),
            ) => NavigationIntent::OpenAlbum {
                album: AlbumKey::new(&key.album, &key.album_artist),
                anchor_track_id: None,
            },
            reprise_core::browser::TrackCollection::Library(
                reprise_core::browser::LibraryScope::Artist(key),
            ) => NavigationIntent::OpenArtist {
                artist: ArtistKey::new(&key.artist),
                anchor_track_id: None,
            },
            reprise_core::browser::TrackCollection::Library(
                reprise_core::browser::LibraryScope::Genre(genre),
            ) => NavigationIntent::OpenGenre {
                genre: genre.clone(),
            },
            reprise_core::browser::TrackCollection::Playlist(id) => {
                NavigationIntent::Sidebar(SidebarTarget::Playlist(*id))
            }
            reprise_core::browser::TrackCollection::Smart(id) => {
                NavigationIntent::Sidebar(SidebarTarget::Smart(*id))
            }
            reprise_core::browser::TrackCollection::Queue => {
                NavigationIntent::Sidebar(SidebarTarget::Queue)
            }
            reprise_core::browser::TrackCollection::Missing => {
                NavigationIntent::Sidebar(SidebarTarget::Missing)
            }
        },
        BrowserPlace::ImportErrors => NavigationIntent::Sidebar(SidebarTarget::ImportErrors),
        BrowserPlace::MyStats => NavigationIntent::Sidebar(SidebarTarget::MyStats),
        BrowserPlace::Releases => NavigationIntent::Sidebar(SidebarTarget::Releases),
        BrowserPlace::Concerts => NavigationIntent::Sidebar(SidebarTarget::Concerts),
        BrowserPlace::Podcasts => NavigationIntent::Sidebar(SidebarTarget::Podcasts),
        BrowserPlace::Radio => NavigationIntent::Sidebar(SidebarTarget::Radio),
        BrowserPlace::Conversions => NavigationIntent::Sidebar(SidebarTarget::Conversions),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(source: ViewSource) -> NavPlace {
        NavPlace::source(source)
    }

    fn simulate(nav: &NavHistory, target: Option<NavPlace>) -> Option<NavPlace> {
        let target = target?;
        nav.begin_back();
        nav.record_route(&target);
        nav.end_back();
        Some(target)
    }

    #[test]
    fn browse_1_routes_album_and_artist_as_track_places_in_one_history() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library));
        let album = place(ViewSource::Album {
            album: "Blue".into(),
            album_artist: "Joni Mitchell".into(),
        });
        let artist = place(ViewSource::Artist("Joni Mitchell".into()));

        nav.record_route(&album);
        nav.record_route(&artist);

        assert_eq!(simulate(&nav, nav.go_back()), Some(album.clone()));
        assert_eq!(
            simulate(&nav, nav.go_back()),
            Some(place(ViewSource::Library))
        );
        assert_eq!(simulate(&nav, nav.go_forward()), Some(album));
    }

    #[test]
    fn updates_full_views_round_trip_through_navigation_history() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library));
        nav.record_route(&place(ViewSource::Releases));
        nav.record_route(&place(ViewSource::Concerts));

        assert_eq!(
            simulate(&nav, nav.go_back()),
            Some(place(ViewSource::Releases))
        );
        assert_eq!(
            simulate(&nav, nav.go_forward()),
            Some(place(ViewSource::Concerts))
        );
    }

    #[test]
    fn new_navigation_after_back_discards_forward_places() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library));
        nav.record_route(&place(ViewSource::Queue));
        simulate(&nav, nav.go_back());

        nav.record_route(&place(ViewSource::Missing));

        assert_eq!(nav.go_forward(), None);
    }

    #[test]
    fn browse_2_back_restores_the_complete_track_place_captured_on_leave() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library));
        let mut current = BrowserPlace::from(ViewSource::Library);
        let BrowserPlace::Tracks(track_place) = &mut current else {
            unreachable!();
        };
        track_place.state.search = "shore".into();
        track_place.state.selected_ids = vec![42];
        track_place.state.focus = reprise_core::browser::TrackFocus::Track(42);
        let album = place(ViewSource::Album {
            album: "Pain Remains".into(),
            album_artist: "Lorna Shore".into(),
        });

        nav.record_route_from(&album, current.clone());
        let restored = nav
            .go_back_from(album.browser_place().clone())
            .expect("Library must be in Back history");

        assert_eq!(restored.browser_place(), &current);
    }

    #[test]
    fn browse_4_metadata_intents_share_one_anchored_navigation_path() {
        let nav = NavHistory::default();
        let library = BrowserPlace::from(ViewSource::Library);
        nav.record_route(&NavPlace::browser(library.clone()));

        let album = nav
            .navigate_from(
                NavigationIntent::OpenAlbum {
                    album: AlbumKey::new("Pain Remains", "Lorna Shore"),
                    anchor_track_id: Some(42),
                },
                library.clone(),
            )
            .unwrap();
        let state = album.browser_place().track_state().unwrap();
        assert_eq!(state.selected_ids, vec![42]);
        assert_eq!(state.focus, reprise_core::browser::TrackFocus::Track(42));
        assert_eq!(
            nav.go_back_from(album.browser_place().clone())
                .unwrap()
                .browser_place(),
            &library
        );
    }

    #[test]
    fn browse_5_restore_keeps_current_and_library_root_but_drops_history() {
        let nav = NavHistory::default();
        let mut root = BrowserPlace::from(ViewSource::Library);
        root.track_state_mut().unwrap().search = "root query".into();
        let current = BrowserPlace::fresh_album("Blue", "Joni Mitchell");

        nav.restore(current.clone(), root.clone());

        assert_eq!(nav.session_places(current.clone()), Some((current, root)));
        assert_eq!(nav.go_back(), None);
        assert_eq!(nav.go_forward(), None);
    }
}
