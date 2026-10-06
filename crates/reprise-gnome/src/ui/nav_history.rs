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

/// The place the user is leaving, and what produced it (`BROWSE-4a`).
///
/// The kind decides how the router may use it: only a page that names a
/// section of its own may enter history as a place the router never held.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::ui) enum Origin {
    /// A section page that names its own place (Podcasts, the Library Doctor).
    Section(BrowserPlace),
    /// The track list's own page: its place refreshes the router's current one.
    TrackList(BrowserPlace),
    /// A page with no place of its own (the device card, an unrecognised
    /// page). The track list's place is handed over but, as for `TrackList`,
    /// never enters history.
    Unknown(BrowserPlace),
}

impl Origin {
    fn into_place(self) -> BrowserPlace {
        match self {
            Self::Section(place) | Self::TrackList(place) | Self::Unknown(place) => place,
        }
    }
}

impl From<BrowserPlace> for Origin {
    fn from(place: BrowserPlace) -> Self {
        Self::TrackList(place)
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
        if let Some(intent) = intent_for(&new.browser) {
            let _ = router.navigate(intent);
        }
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
        // BROWSE-12: the Doctor is a utility overlay, process-local like the
        // rest; a restart opens the place behind it, or the Music root.
        let current = match navigation.current() {
            BrowserPlace::LibraryDoctor => navigation
                .previous()
                .filter(|place| **place != BrowserPlace::LibraryDoctor)
                .unwrap_or_else(|| navigation.library_root()),
            current => current,
        };
        Some((current.clone(), navigation.library_root().clone()))
    }

    /// Records a sidebar-driven switch, capturing the page it leaves.
    ///
    /// The sidebar's `on_select` also runs *after* the router has moved —
    /// replaying Back or Forward, or routing a metadata intent through a
    /// source row — while the old page is still on screen. Observing then
    /// would pull the router back to where it left, so the origin only enters
    /// history when this is a fresh switch to a destination the router does
    /// not already hold.
    pub(in crate::ui) fn record_route_from(&self, new: &NavPlace, origin: impl Into<Origin>) {
        let fresh = !self.replaying_history.get() && !self.holds(new.browser_place());
        match origin.into() {
            Origin::Section(place) if fresh => self.observe_origin(place),
            origin => self.replace_current(origin.into_place()),
        }
        self.record_route(new);
    }

    pub(in crate::ui) fn navigate_from(
        &self,
        intent: NavigationIntent,
        origin: impl Into<Origin>,
    ) -> Option<NavPlace> {
        self.navigate_observing(origin.into(), intent)
    }

    #[cfg(test)]
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

    pub(in crate::ui) fn go_back_from(&self, origin: impl Into<Origin>) -> Option<NavPlace> {
        self.navigate_observing(origin.into(), NavigationIntent::Back)
    }

    #[cfg(test)]
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

    pub(in crate::ui) fn go_forward_from(&self, origin: impl Into<Origin>) -> Option<NavPlace> {
        self.navigate_observing(origin.into(), NavigationIntent::Forward)
    }

    pub(in crate::ui) fn begin_back(&self) {
        self.replaying_history.set(true);
    }

    pub(in crate::ui) fn end_back(&self) {
        self.replaying_history.set(false);
    }

    /// Refreshes the router's current place from the track list. A place the
    /// router does not hold is ignored.
    fn replace_current(&self, current: BrowserPlace) {
        if let Some(router) = self.navigation.borrow_mut().as_mut() {
            let _ = router.replace_current(current);
        }
    }

    /// Captures a section the router never held, only for an intent that
    /// goes somewhere (`BROWSE-4a`).
    fn navigate_observing(&self, origin: Origin, intent: NavigationIntent) -> Option<NavPlace> {
        let mut navigation = self.navigation.borrow_mut();
        let router = navigation.as_mut()?;
        let transition = match origin {
            Origin::Section(place) => router.navigate_observing(place, intent),
            Origin::TrackList(place) | Origin::Unknown(place) => {
                let _ = router.replace_current(place);
                router.navigate(intent)
            }
        }?;
        Some(NavPlace {
            browser: transition.to,
        })
    }

    /// Enters a section the router never held as a new place.
    fn observe_origin(&self, place: BrowserPlace) {
        if let Some(router) = self.navigation.borrow_mut().as_mut() {
            let _ = router.observe_visible(place);
        }
    }

    fn holds(&self, place: &BrowserPlace) -> bool {
        self.navigation
            .borrow()
            .as_ref()
            .is_some_and(|router| router.holds(place))
    }
}

/// The intent that routes to `place`. None for the Library Doctor: no intent
/// produces it, it enters history only as an observed origin (`BROWSE-4a`).
fn intent_for(place: &BrowserPlace) -> Option<NavigationIntent> {
    Some(match place {
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
        BrowserPlace::Youtube => NavigationIntent::Sidebar(SidebarTarget::Youtube),
        BrowserPlace::Radio => NavigationIntent::Sidebar(SidebarTarget::Radio),
        BrowserPlace::Conversions => NavigationIntent::Sidebar(SidebarTarget::Conversions),
        BrowserPlace::LibraryDoctor => return None,
    })
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
    fn session_history_restore_keeps_current_and_library_root_but_drops_history() {
        let nav = NavHistory::default();
        let mut root = BrowserPlace::from(ViewSource::Library);
        root.track_state_mut().unwrap().search = "root query".into();
        let current = BrowserPlace::fresh_album("Blue", "Joni Mitchell");

        // START-3 restores these two places without reconstructing history.
        nav.restore(current.clone(), root.clone());

        assert_eq!(nav.session_places(current.clone()), Some((current, root)));
        assert_eq!(nav.go_back(), None);
        assert_eq!(nav.go_forward(), None);
    }

    fn doctor() -> Origin {
        Origin::Section(BrowserPlace::LibraryDoctor)
    }

    fn section(place: BrowserPlace) -> Origin {
        Origin::Section(place)
    }

    fn open_album(nav: &NavHistory, visible: Origin) -> NavPlace {
        nav.navigate_from(
            NavigationIntent::OpenAlbum {
                album: AlbumKey::new("Blue", "Joni Mitchell"),
                anchor_track_id: None,
            },
            visible,
        )
        .expect("an album is a new destination")
    }

    #[test]
    fn browse_4a_back_after_a_jump_from_podcasts_returns_to_podcasts() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library));
        nav.record_route_from(
            &place(ViewSource::Podcasts),
            BrowserPlace::from(ViewSource::Library),
        );

        // The track list still holds the library place behind the section.
        let album = open_album(&nav, section(BrowserPlace::Podcasts));

        assert_eq!(
            nav.go_back_from(album.browser_place().clone())
                .unwrap()
                .browser_place(),
            &BrowserPlace::Podcasts
        );
    }

    #[test]
    fn browse_4a_back_after_a_jump_from_the_doctor_returns_to_the_doctor_not_the_last_list() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library));
        nav.record_route_from(
            &place(ViewSource::Radio),
            BrowserPlace::from(ViewSource::Library),
        );

        let album = open_album(&nav, doctor());

        let back = nav.go_back_from(album.browser_place().clone()).unwrap();
        assert_eq!(back.browser_place(), &BrowserPlace::LibraryDoctor);
        let back = nav.go_back_from(doctor()).unwrap();
        assert_eq!(back.browser_place(), &BrowserPlace::Radio);
    }

    #[test]
    fn browse_4a_alt_left_out_of_the_doctor_returns_to_the_section_it_was_opened_from() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library));
        nav.record_route_from(
            &place(ViewSource::Podcasts),
            BrowserPlace::from(ViewSource::Library),
        );

        let back = nav.go_back_from(doctor()).unwrap();

        assert_eq!(back.browser_place(), &BrowserPlace::Podcasts);
    }

    /// Back re-routes through the sidebar, whose `on_select` records the place
    /// again while the old page is still on screen. That replay must neither
    /// move the router back nor wipe Forward.
    #[test]
    fn browse_4a_the_sidebar_replay_of_a_back_between_sections_keeps_the_router_put() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library));
        nav.record_route_from(
            &place(ViewSource::Podcasts),
            BrowserPlace::from(ViewSource::Library),
        );
        nav.record_route_from(&place(ViewSource::Radio), section(BrowserPlace::Podcasts));

        let back = nav.go_back_from(section(BrowserPlace::Radio)).unwrap();
        assert_eq!(back.browser_place(), &BrowserPlace::Podcasts);
        nav.begin_back();
        // `on_select` while the Radio page is still the visible one.
        nav.record_route_from(&back, section(BrowserPlace::Radio));
        nav.end_back();

        let again = nav.go_back_from(section(BrowserPlace::Podcasts)).unwrap();
        assert_eq!(
            again.browser_place(),
            &BrowserPlace::from(ViewSource::Library)
        );
        let forward = nav.go_forward_from(BrowserPlace::from(ViewSource::Library));
        assert_eq!(
            forward.unwrap().browser_place(),
            &BrowserPlace::Podcasts,
            "Forward survives the replayed record"
        );
    }

    /// A metadata intent that routes through the sidebar (a playlist from
    /// Quick Open) is recorded by `on_select` after the router already moved.
    #[test]
    fn browse_4a_a_sidebar_routed_jump_from_the_doctor_still_returns_to_the_doctor() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library));
        nav.record_route_from(
            &place(ViewSource::Podcasts),
            BrowserPlace::from(ViewSource::Library),
        );

        let playlist = nav
            .navigate_from(
                NavigationIntent::Sidebar(SidebarTarget::Playlist(7)),
                doctor(),
            )
            .unwrap();
        // `on_select` runs before the stack leaves the Doctor page.
        nav.record_route_from(&playlist, doctor());

        let back = nav.go_back_from(playlist.browser_place().clone()).unwrap();
        assert_eq!(back.browser_place(), &BrowserPlace::LibraryDoctor);
        let back = nav.go_back_from(doctor()).unwrap();
        assert_eq!(back.browser_place(), &BrowserPlace::Podcasts);
    }

    /// Exit A: the Doctor was opened from its row, which never told the router.
    #[test]
    fn browse_4a_a_sidebar_click_out_of_an_unrecorded_doctor_enters_it_into_history() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library));
        nav.record_route_from(
            &place(ViewSource::Podcasts),
            BrowserPlace::from(ViewSource::Library),
        );

        nav.record_route_from(&place(ViewSource::Radio), doctor());

        let back = nav.go_back_from(section(BrowserPlace::Radio)).unwrap();
        assert_eq!(back.browser_place(), &BrowserPlace::LibraryDoctor);
        let back = nav.go_back_from(doctor()).unwrap();
        assert_eq!(back.browser_place(), &BrowserPlace::Podcasts);
    }

    /// Exit B: the Doctor was reached by Back, so the router holds it. Both
    /// exits must read the same.
    #[test]
    fn browse_4a_a_sidebar_click_out_of_a_doctor_reached_by_back_enters_it_into_history() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library));
        nav.record_route_from(
            &place(ViewSource::Podcasts),
            BrowserPlace::from(ViewSource::Library),
        );
        let album = open_album(&nav, doctor());
        let back = nav.go_back_from(album.browser_place().clone()).unwrap();
        assert_eq!(back.browser_place(), &BrowserPlace::LibraryDoctor);

        nav.record_route_from(&place(ViewSource::Radio), doctor());

        let back = nav.go_back_from(section(BrowserPlace::Radio)).unwrap();
        assert_eq!(back.browser_place(), &BrowserPlace::LibraryDoctor);
        let back = nav.go_back_from(doctor()).unwrap();
        assert_eq!(back.browser_place(), &BrowserPlace::Podcasts);
    }

    fn podcasts_after_library() -> NavHistory {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::Library));
        nav.record_route_from(
            &place(ViewSource::Podcasts),
            BrowserPlace::from(ViewSource::Library),
        );
        nav
    }

    fn assert_doctor_left_no_trace(nav: &NavHistory) {
        let back = nav.go_back_from(section(BrowserPlace::Podcasts)).unwrap();
        assert_eq!(
            back.browser_place(),
            &BrowserPlace::from(ViewSource::Library),
            "an intent that goes nowhere must not enter the Doctor into history"
        );
    }

    #[test]
    fn browse_4a_an_empty_album_intent_leaves_a_visible_doctor_out_of_history() {
        let nav = podcasts_after_library();

        let moved = nav.navigate_from(
            NavigationIntent::OpenAlbum {
                album: AlbumKey::new("", "Anyone"),
                anchor_track_id: None,
            },
            doctor(),
        );

        assert_eq!(moved, None);
        assert_doctor_left_no_trace(&nav);
    }

    #[test]
    fn browse_4a_a_reveal_of_no_track_leaves_a_visible_doctor_out_of_history() {
        let nav = podcasts_after_library();

        let moved = nav.navigate_from(
            NavigationIntent::RevealTrack {
                origin: Box::new(BrowserPlace::from(ViewSource::Library)),
                track_id: 0,
            },
            doctor(),
        );

        assert_eq!(moved, None);
        assert_doctor_left_no_trace(&nav);
    }

    #[test]
    fn browse_4a_forward_with_nothing_ahead_neither_enters_the_doctor_nor_clears_forward() {
        let nav = podcasts_after_library();
        let back = nav.go_back_from(section(BrowserPlace::Podcasts)).unwrap();
        assert_eq!(
            back.browser_place(),
            &BrowserPlace::from(ViewSource::Library)
        );

        assert_eq!(nav.go_forward_from(doctor()), None);

        let forward = nav.go_forward_from(BrowserPlace::from(ViewSource::Library));
        assert_eq!(forward.unwrap().browser_place(), &BrowserPlace::Podcasts);
    }

    /// A page with no place of its own hands over the track list's place, and
    /// a stale trackless place there must not enter history: ImportErrors,
    /// Podcasts, the device card, a jump, Back.
    #[test]
    fn browse_4a_a_page_without_a_place_never_enters_a_stale_track_place_into_history() {
        let nav = NavHistory::default();
        nav.record_route(&place(ViewSource::ImportErrors));
        nav.record_route_from(&place(ViewSource::Podcasts), BrowserPlace::ImportErrors);

        let album = nav
            .navigate_from(
                NavigationIntent::OpenAlbum {
                    album: AlbumKey::new("Blue", "Joni Mitchell"),
                    anchor_track_id: None,
                },
                Origin::Unknown(BrowserPlace::ImportErrors),
            )
            .unwrap();

        let back = nav.go_back_from(album.browser_place().clone()).unwrap();
        assert_eq!(back.browser_place(), &BrowserPlace::Podcasts);
    }

    /// The session keeps the place behind the Doctor, not the Music root.
    #[test]
    fn browse_12_a_doctor_reached_by_back_saves_the_place_behind_it() {
        let nav = NavHistory::default();
        let playlist = BrowserPlace::from(ViewSource::Playlist(7));
        nav.restore(playlist.clone(), BrowserPlace::from(ViewSource::Library));
        let album = open_album(&nav, doctor());
        let back = nav.go_back_from(album.browser_place().clone()).unwrap();
        assert_eq!(back.browser_place(), &BrowserPlace::LibraryDoctor);

        let saved = nav.session_places(BrowserPlace::from(ViewSource::Library));

        assert_eq!(
            saved,
            Some((playlist, BrowserPlace::from(ViewSource::Library)))
        );
    }

    #[test]
    fn browse_12_the_doctor_is_never_saved_as_the_last_destination() {
        let nav = NavHistory::default();
        let mut root = BrowserPlace::from(ViewSource::Library);
        root.track_state_mut().unwrap().search = "root query".into();
        nav.restore(root.clone(), root.clone());

        // The real path to Doctor-as-current: a jump out of it, then Back.
        let album = open_album(&nav, doctor());
        let back = nav.go_back_from(album.browser_place().clone()).unwrap();
        assert_eq!(back.browser_place(), &BrowserPlace::LibraryDoctor);

        let saved = nav.session_places(root.clone());

        assert_eq!(saved, Some((root.clone(), root)));
    }
}
