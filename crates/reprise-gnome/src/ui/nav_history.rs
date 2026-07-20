//! Compatibility edge around the canonical core browser router.
//!
//! GTK routing still speaks in `NavPlace` while the browser migration is in
//! progress, but all current-place and Back/Forward state lives in
//! [`BrowserNavigation`]. Album and artist destinations are ordinary scoped
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

    pub(in crate::ui) fn is_new_releases(&self) -> bool {
        self.browser == BrowserPlace::NewReleases
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

    pub(in crate::ui) fn record_route_from(&self, new: &NavPlace, current: BrowserPlace) {
        self.replace_current(current);
        self.record_route(new);
    }

    pub(in crate::ui) fn record_new_releases(&self) -> Option<NavPlace> {
        let mut navigation = self.navigation.borrow_mut();
        let router = navigation.as_mut()?;
        let transition = router.navigate(NavigationIntent::OpenNewReleases)?;
        Some(NavPlace {
            browser: transition.to,
        })
    }

    pub(in crate::ui) fn record_new_releases_from(
        &self,
        current: BrowserPlace,
    ) -> Option<NavPlace> {
        self.replace_current(current);
        self.record_new_releases()
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
        BrowserPlace::NewReleases => NavigationIntent::OpenNewReleases,
        BrowserPlace::ImportErrors => NavigationIntent::Sidebar(SidebarTarget::ImportErrors),
        BrowserPlace::MyStats => NavigationIntent::Sidebar(SidebarTarget::MyStats),
        BrowserPlace::Device { serial } => {
            NavigationIntent::Sidebar(SidebarTarget::Device(serial.clone()))
        }
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
    fn new_navigation_after_back_discards_forward_places() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library));
        nav.record_route(&place(ViewSource::Queue));
        simulate(&nav, nav.go_back());

        nav.record_route(&place(ViewSource::Missing));

        assert_eq!(nav.go_forward(), None);
    }

    #[test]
    fn new_releases_is_a_regular_back_forward_place() {
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
}
