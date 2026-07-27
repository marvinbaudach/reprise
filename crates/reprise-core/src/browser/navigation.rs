//! Browser navigation as one pure state machine.
//!
//! Widgets send semantic intents here and render the returned transition.
//! History therefore stores complete [`BrowserPlace`] values instead of
//! reconstructing filters, selection, focus, or scroll state from widgets.

use super::{
    AlbumKey, ArtistKey, BrowserPlace, LibraryScope, TrackAnchor, TrackCollection, TrackFocus,
    TrackViewState,
};

pub const MAX_HISTORY: usize = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarTarget {
    Music,
    RecentlyAdded,
    Queue,
    Playlist(i64),
    Smart(i64),
    Missing,
    ImportErrors,
    MyStats,
    Releases,
    Concerts,
    Podcasts,
    Radio,
    Conversions,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NavigationIntent {
    Sidebar(SidebarTarget),
    OpenAlbum {
        album: AlbumKey,
        anchor_track_id: Option<i64>,
    },
    OpenArtist {
        artist: ArtistKey,
        anchor_track_id: Option<i64>,
    },
    OpenGenre {
        genre: String,
    },
    RevealTrack {
        origin: Box<BrowserPlace>,
        track_id: i64,
    },
    Back,
    Forward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationDirection {
    New,
    Replace,
    Back,
    Forward,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NavigationTransition {
    pub from: BrowserPlace,
    pub to: BrowserPlace,
    pub direction: NavigationDirection,
}

#[derive(Clone, Debug)]
pub struct BrowserNavigation {
    current: BrowserPlace,
    library_root: BrowserPlace,
    back: Vec<BrowserPlace>,
    forward: Vec<BrowserPlace>,
}

impl BrowserNavigation {
    #[must_use]
    pub fn new(initial: BrowserPlace) -> Self {
        let library_root = if is_library_root(&initial) {
            initial.clone()
        } else {
            default_library_root()
        };
        Self {
            current: initial,
            library_root,
            back: Vec::new(),
            forward: Vec::new(),
        }
    }

    /// Restores only the current place and remembered Music root. History is
    /// intentionally empty after process startup.
    #[must_use]
    pub fn restore(current: BrowserPlace, library_root: BrowserPlace) -> Self {
        let library_root = if is_library_root(&library_root) {
            library_root
        } else {
            default_library_root()
        };
        Self {
            current,
            library_root,
            back: Vec::new(),
            forward: Vec::new(),
        }
    }

    #[must_use]
    pub fn current(&self) -> &BrowserPlace {
        &self.current
    }

    #[must_use]
    pub fn library_root(&self) -> &BrowserPlace {
        &self.library_root
    }

    /// Updates transient state at the current place without creating history.
    /// Frontends call this before routing after search, sort, focus, selection,
    /// or scroll bookmarks change.
    pub fn replace_current(&mut self, place: BrowserPlace) -> bool {
        if !same_destination(&self.current, &place) {
            return false;
        }
        if is_library_root(&place) {
            self.library_root = place.clone();
        }
        self.current = place;
        true
    }

    #[must_use]
    pub fn back_len(&self) -> usize {
        self.back.len()
    }

    pub fn navigate(&mut self, intent: NavigationIntent) -> Option<NavigationTransition> {
        match intent {
            NavigationIntent::Back => self.go_back(),
            NavigationIntent::Forward => self.go_forward(),
            NavigationIntent::Sidebar(target) => {
                let target = self.sidebar_place(target);
                if same_destination(&self.current, &target) {
                    return None;
                }
                self.go_new(target)
            }
            NavigationIntent::OpenAlbum {
                album,
                anchor_track_id,
            } => {
                if album.album.is_empty() {
                    return None;
                }
                self.go_metadata_scope(BrowserPlace::tracks(
                    TrackCollection::Library(LibraryScope::Album(album)),
                    fresh_target_state(anchor_track_id),
                ))
            }
            NavigationIntent::OpenArtist {
                artist,
                anchor_track_id,
            } => {
                if artist.artist.is_empty() {
                    return None;
                }
                self.go_metadata_scope(BrowserPlace::tracks(
                    TrackCollection::Library(LibraryScope::Artist(artist)),
                    fresh_target_state(anchor_track_id),
                ))
            }
            NavigationIntent::OpenGenre { genre } => {
                let genre = genre.trim();
                if genre.is_empty() {
                    return None;
                }
                self.go_metadata_scope(BrowserPlace::tracks(
                    TrackCollection::Library(LibraryScope::Genre(genre.to_owned())),
                    TrackViewState::default(),
                ))
            }
            NavigationIntent::RevealTrack { origin, track_id } => {
                if track_id <= 0 {
                    return None;
                }
                let mut target = if origin.track_state().is_some() {
                    *origin
                } else {
                    self.library_root.clone()
                };
                set_explicit_track_anchor(&mut target, track_id);
                self.go_metadata_scope(target)
            }
        }
    }

    fn sidebar_place(&self, target: SidebarTarget) -> BrowserPlace {
        match target {
            SidebarTarget::Music => self.library_root.clone(),
            SidebarTarget::RecentlyAdded => {
                fresh_tracks(TrackCollection::Library(LibraryScope::RecentlyAdded))
            }
            SidebarTarget::Queue => fresh_tracks(TrackCollection::Queue),
            SidebarTarget::Playlist(id) if id > 0 => fresh_tracks(TrackCollection::Playlist(id)),
            SidebarTarget::Smart(id) if id > 0 => fresh_tracks(TrackCollection::Smart(id)),
            SidebarTarget::Missing => fresh_tracks(TrackCollection::Missing),
            SidebarTarget::ImportErrors => BrowserPlace::ImportErrors,
            SidebarTarget::MyStats => BrowserPlace::MyStats,
            SidebarTarget::Releases => BrowserPlace::Releases,
            SidebarTarget::Concerts => BrowserPlace::Concerts,
            SidebarTarget::Podcasts => BrowserPlace::Podcasts,
            SidebarTarget::Radio => BrowserPlace::Radio,
            SidebarTarget::Conversions => BrowserPlace::Conversions,
            SidebarTarget::Playlist(_) | SidebarTarget::Smart(_) => self.library_root.clone(),
        }
    }

    fn go_metadata_scope(&mut self, target: BrowserPlace) -> Option<NavigationTransition> {
        if self.current == target {
            return None;
        }
        if same_destination(&self.current, &target) {
            let from = std::mem::replace(&mut self.current, target.clone());
            self.remember_library_root();
            self.forward.clear();
            return Some(NavigationTransition {
                from,
                to: target,
                direction: NavigationDirection::Replace,
            });
        }
        self.go_new(target)
    }

    fn go_new(&mut self, target: BrowserPlace) -> Option<NavigationTransition> {
        if self.current == target {
            return None;
        }
        self.remember_library_root();
        let from = std::mem::replace(&mut self.current, target.clone());
        push_bounded(&mut self.back, from.clone());
        self.forward.clear();
        Some(NavigationTransition {
            from,
            to: target,
            direction: NavigationDirection::New,
        })
    }

    fn go_back(&mut self) -> Option<NavigationTransition> {
        let target = self.back.pop()?;
        self.remember_library_root();
        let from = std::mem::replace(&mut self.current, target.clone());
        push_bounded(&mut self.forward, from.clone());
        Some(NavigationTransition {
            from,
            to: target,
            direction: NavigationDirection::Back,
        })
    }

    fn go_forward(&mut self) -> Option<NavigationTransition> {
        let target = self.forward.pop()?;
        self.remember_library_root();
        let from = std::mem::replace(&mut self.current, target.clone());
        push_bounded(&mut self.back, from.clone());
        Some(NavigationTransition {
            from,
            to: target,
            direction: NavigationDirection::Forward,
        })
    }

    fn remember_library_root(&mut self) {
        if is_library_root(&self.current) {
            self.library_root = self.current.clone();
        }
    }
}

fn default_library_root() -> BrowserPlace {
    fresh_tracks(TrackCollection::Library(LibraryScope::All))
}

fn fresh_tracks(collection: TrackCollection) -> BrowserPlace {
    BrowserPlace::tracks(collection, TrackViewState::default())
}

fn fresh_target_state(track_id: Option<i64>) -> TrackViewState {
    let Some(track_id) = track_id.filter(|track_id| *track_id > 0) else {
        return TrackViewState::default();
    };
    TrackViewState {
        anchor: Some(TrackAnchor::new(track_id, 0.0)),
        selected_ids: vec![track_id],
        focus: TrackFocus::Track(track_id),
        ..TrackViewState::default()
    }
}

fn set_explicit_track_anchor(place: &mut BrowserPlace, track_id: i64) {
    let BrowserPlace::Tracks(place) = place else {
        return;
    };
    place.state.anchor = Some(TrackAnchor::new(track_id, 0.0));
    place.state.selected_ids = vec![track_id];
    place.state.focus = TrackFocus::Track(track_id);
}

fn is_library_root(place: &BrowserPlace) -> bool {
    place.collection() == Some(&TrackCollection::Library(LibraryScope::All))
}

fn same_destination(left: &BrowserPlace, right: &BrowserPlace) -> bool {
    match (left, right) {
        (BrowserPlace::Tracks(left), BrowserPlace::Tracks(right)) => {
            left.collection == right.collection
        }
        (BrowserPlace::ImportErrors, BrowserPlace::ImportErrors)
        | (BrowserPlace::MyStats, BrowserPlace::MyStats)
        | (BrowserPlace::Releases, BrowserPlace::Releases)
        | (BrowserPlace::Concerts, BrowserPlace::Concerts)
        | (BrowserPlace::Podcasts, BrowserPlace::Podcasts)
        | (BrowserPlace::Radio, BrowserPlace::Radio)
        | (BrowserPlace::Conversions, BrowserPlace::Conversions) => true,
        _ => false,
    }
}

fn push_bounded(stack: &mut Vec<BrowserPlace>, place: BrowserPlace) {
    stack.push(place);
    if stack.len() > MAX_HISTORY {
        stack.remove(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::{
        AlbumKey, ArtistKey, BrowserPlace, LibraryScope, TrackCollection, TrackFocus,
        TrackViewState,
    };

    fn library() -> BrowserPlace {
        BrowserPlace::tracks(
            TrackCollection::Library(LibraryScope::All),
            TrackViewState::default(),
        )
    }

    #[test]
    fn concerts_and_releases_are_independent_sidebar_places() {
        let mut navigation = BrowserNavigation::new(library());

        let releases = navigation
            .navigate(NavigationIntent::Sidebar(SidebarTarget::Releases))
            .unwrap();
        assert_eq!(releases.to, BrowserPlace::Releases);

        let concerts = navigation
            .navigate(NavigationIntent::Sidebar(SidebarTarget::Concerts))
            .unwrap();
        assert_eq!(concerts.to, BrowserPlace::Concerts);
        assert_eq!(navigation.back_len(), 2);
    }

    #[test]
    fn browse_3_sidebar_music_restores_the_remembered_library_root() {
        let mut navigation = BrowserNavigation::new(library());
        let remembered = TrackViewState {
            search: "shore".into(),
            ..TrackViewState::default()
        };
        navigation.replace_current(BrowserPlace::tracks(
            TrackCollection::Library(LibraryScope::All),
            remembered.clone(),
        ));
        navigation
            .navigate(NavigationIntent::OpenAlbum {
                album: AlbumKey::new("Pain Remains", "Lorna Shore"),
                anchor_track_id: Some(17),
            })
            .unwrap();

        let transition = navigation
            .navigate(NavigationIntent::Sidebar(SidebarTarget::Music))
            .unwrap();

        assert_eq!(transition.to.track_state(), Some(&remembered));
        assert_eq!(navigation.current(), &transition.to);
    }

    #[test]
    fn browse_2_back_and_forward_restore_complete_places() {
        let mut navigation = BrowserNavigation::new(library());
        let root_state = TrackViewState {
            search: "metal".into(),
            ..TrackViewState::default()
        };
        navigation.replace_current(BrowserPlace::tracks(
            TrackCollection::Library(LibraryScope::All),
            root_state.clone(),
        ));
        let album = navigation
            .navigate(NavigationIntent::OpenAlbum {
                album: AlbumKey::new("Blue", "Joni Mitchell"),
                anchor_track_id: Some(8),
            })
            .unwrap()
            .to;

        let back = navigation.navigate(NavigationIntent::Back).unwrap();
        assert_eq!(back.direction, NavigationDirection::Back);
        assert_eq!(back.to.track_state(), Some(&root_state));

        let forward = navigation.navigate(NavigationIntent::Forward).unwrap();
        assert_eq!(forward.direction, NavigationDirection::Forward);
        assert_eq!(forward.to, album);
    }

    #[test]
    fn browse_3_sidebar_destinations_are_absolute_and_active_roots_are_noops() {
        let mut navigation = BrowserNavigation::new(library());
        let queue = navigation
            .navigate(NavigationIntent::Sidebar(SidebarTarget::Queue))
            .unwrap();
        assert_eq!(
            queue.to.collection(),
            Some(&TrackCollection::Queue),
            "Queue must not depend on whichever utility or scope was visible"
        );
        assert!(navigation
            .navigate(NavigationIntent::Sidebar(SidebarTarget::Queue))
            .is_none());

        let stats = navigation
            .navigate(NavigationIntent::Sidebar(SidebarTarget::MyStats))
            .unwrap();
        assert_eq!(stats.to, BrowserPlace::MyStats);

        let podcasts = navigation
            .navigate(NavigationIntent::Sidebar(SidebarTarget::Podcasts))
            .unwrap();
        assert_eq!(podcasts.to, BrowserPlace::Podcasts);

        let radio = navigation
            .navigate(NavigationIntent::Sidebar(SidebarTarget::Radio))
            .unwrap();
        assert_eq!(radio.to, BrowserPlace::Radio);
    }

    #[test]
    fn browse_4_metadata_intents_create_fresh_scopes_with_one_explicit_anchor() {
        let mut navigation = BrowserNavigation::new(library());

        let album = navigation
            .navigate(NavigationIntent::OpenAlbum {
                album: AlbumKey::new("Blue", "Joni Mitchell"),
                anchor_track_id: Some(31),
            })
            .unwrap()
            .to;
        assert_eq!(album.track_state().unwrap().search, "");
        assert_eq!(album.track_state().unwrap().selected_ids, vec![31]);
        assert_eq!(album.track_state().unwrap().focus, TrackFocus::Track(31));

        let artist = navigation
            .navigate(NavigationIntent::OpenArtist {
                artist: ArtistKey::new("Björk"),
                anchor_track_id: None,
            })
            .unwrap()
            .to;
        assert_eq!(
            artist.collection(),
            Some(&TrackCollection::Library(LibraryScope::Artist(
                ArtistKey::new("Björk")
            )))
        );
        assert_eq!(artist.track_state(), Some(&TrackViewState::default()));
    }

    #[test]
    fn fil_1c_genre_scope_uses_the_same_history_path_as_other_library_scopes() {
        let mut navigation = BrowserNavigation::new(library());
        let genre = navigation
            .navigate(NavigationIntent::OpenGenre {
                genre: "  Metalcore  ".into(),
            })
            .unwrap()
            .to;

        assert_eq!(
            genre.collection(),
            Some(&TrackCollection::Library(LibraryScope::Genre(
                "Metalcore".into()
            )))
        );
        assert_eq!(
            navigation.navigate(NavigationIntent::Back).unwrap().to,
            library()
        );
        assert_eq!(
            navigation.navigate(NavigationIntent::Forward).unwrap().to,
            genre
        );
    }

    #[test]
    fn browse_2_new_navigation_clears_forward_and_history_is_bounded() {
        let mut navigation = BrowserNavigation::new(library());
        navigation
            .navigate(NavigationIntent::Sidebar(SidebarTarget::Queue))
            .unwrap();
        navigation.navigate(NavigationIntent::Back).unwrap();
        navigation
            .navigate(NavigationIntent::Sidebar(SidebarTarget::Missing))
            .unwrap();
        assert!(navigation.navigate(NavigationIntent::Forward).is_none());

        for id in 1..=200 {
            navigation
                .navigate(NavigationIntent::Sidebar(SidebarTarget::Playlist(id)))
                .unwrap();
        }
        assert_eq!(navigation.back_len(), MAX_HISTORY);
    }

    #[test]
    fn browse_3_invalid_absolute_targets_fall_back_safely() {
        let mut navigation = BrowserNavigation::new(library());
        navigation
            .navigate(NavigationIntent::Sidebar(SidebarTarget::Queue))
            .unwrap();

        let invalid_playlist = navigation
            .navigate(NavigationIntent::Sidebar(SidebarTarget::Playlist(0)))
            .unwrap();
        assert_eq!(invalid_playlist.to, library());
    }

    #[test]
    fn browse_3_active_sidebar_destination_preserves_its_local_state() {
        let mut navigation = BrowserNavigation::new(library());
        navigation
            .navigate(NavigationIntent::Sidebar(SidebarTarget::Queue))
            .unwrap();
        let queue_state = TrackViewState {
            search: "encore".into(),
            ..TrackViewState::default()
        };
        let queue = BrowserPlace::tracks(TrackCollection::Queue, queue_state.clone());
        navigation.replace_current(queue.clone());

        assert!(navigation
            .navigate(NavigationIntent::Sidebar(SidebarTarget::Queue))
            .is_none());
        assert_eq!(navigation.current(), &queue);
        assert_eq!(navigation.current().track_state(), Some(&queue_state));
    }

    #[test]
    fn browse_4_reveal_track_restores_its_structured_origin_and_exact_anchor() {
        let mut navigation = BrowserNavigation::new(library());
        let origin_state = TrackViewState {
            search: "blue".into(),
            ..TrackViewState::default()
        };
        let origin = BrowserPlace::tracks(TrackCollection::Playlist(7), origin_state);

        let target = navigation
            .navigate(NavigationIntent::RevealTrack {
                origin: Box::new(origin),
                track_id: 42,
            })
            .unwrap()
            .to;

        assert_eq!(target.collection(), Some(&TrackCollection::Playlist(7)));
        let state = target.track_state().unwrap();
        assert_eq!(state.search, "blue");
        assert_eq!(
            state.anchor,
            Some(crate::browser::TrackAnchor::new(42, 0.0))
        );
        assert_eq!(state.selected_ids, vec![42]);
        assert_eq!(state.focus, TrackFocus::Track(42));
    }

    #[test]
    fn browse_4_retargeting_the_same_scope_replaces_without_history_and_discards_forward() {
        let mut navigation = BrowserNavigation::new(library());
        navigation
            .navigate(NavigationIntent::OpenAlbum {
                album: AlbumKey::new("Blue", "Joni Mitchell"),
                anchor_track_id: Some(1),
            })
            .unwrap();
        navigation
            .navigate(NavigationIntent::Sidebar(SidebarTarget::Queue))
            .unwrap();
        navigation.navigate(NavigationIntent::Back).unwrap();

        let replacement = navigation
            .navigate(NavigationIntent::OpenAlbum {
                album: AlbumKey::new("blue", "JONI MITCHELL"),
                anchor_track_id: Some(2),
            })
            .unwrap();

        assert_eq!(replacement.direction, NavigationDirection::Replace);
        assert_eq!(replacement.to.track_state().unwrap().selected_ids, vec![2]);
        assert!(navigation.navigate(NavigationIntent::Forward).is_none());
        assert_eq!(navigation.back_len(), 1);
    }

    #[test]
    fn browse_4_blank_or_invalid_metadata_links_are_noops() {
        let mut navigation = BrowserNavigation::new(library());

        assert!(navigation
            .navigate(NavigationIntent::OpenAlbum {
                album: AlbumKey::new("  ", "Joni Mitchell"),
                anchor_track_id: None,
            })
            .is_none());
        assert!(navigation
            .navigate(NavigationIntent::OpenArtist {
                artist: ArtistKey::new("  "),
                anchor_track_id: None,
            })
            .is_none());
        assert!(navigation
            .navigate(NavigationIntent::RevealTrack {
                origin: Box::new(library()),
                track_id: 0,
            })
            .is_none());
        assert_eq!(navigation.current(), &library());
    }

    #[test]
    fn browse_2_transient_state_updates_cannot_teleport_between_destinations() {
        let mut navigation = BrowserNavigation::new(library());

        assert!(!navigation.replace_current(BrowserPlace::tracks(
            TrackCollection::Queue,
            TrackViewState::default(),
        )));
        assert_eq!(navigation.current(), &library());
        assert_eq!(navigation.back_len(), 0);
    }

    #[test]
    fn browse_4_revealing_inside_music_updates_the_remembered_root() {
        let mut navigation = BrowserNavigation::new(library());
        let target = navigation
            .navigate(NavigationIntent::RevealTrack {
                origin: Box::new(library()),
                track_id: 13,
            })
            .unwrap()
            .to;

        assert_eq!(navigation.library_root(), &target);
        navigation
            .navigate(NavigationIntent::Sidebar(SidebarTarget::Queue))
            .unwrap();
        assert_eq!(
            navigation
                .navigate(NavigationIntent::Sidebar(SidebarTarget::Music))
                .unwrap()
                .to,
            target
        );
    }
}
