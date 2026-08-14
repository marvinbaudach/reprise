use super::*;
use crate::browser::{
    BrowseFilter, BrowserPlace, LibraryScope, SortDirection, TrackCollection, TrackFocus,
    TrackSort, TrackViewState,
};

fn library_with_state(state: TrackViewState) -> BrowserPlace {
    BrowserPlace::tracks(TrackCollection::Library(LibraryScope::All), state)
}

fn narrowed_state() -> TrackViewState {
    TrackViewState {
        search: "needle".into(),
        browse: BrowseFilter {
            genre: Some("Ambient".into()),
            album: Some("Structures".into()),
            ..BrowseFilter::default()
        },
        sort: TrackSort::new("album", SortDirection::Descending),
        ..TrackViewState::default()
    }
}

#[test]
fn browse_14_revealing_a_track_drops_the_origins_query_and_facets() {
    let origin = library_with_state(narrowed_state());
    let mut navigation = BrowserNavigation::new(origin.clone());

    let transition = navigation
        .navigate(NavigationIntent::RevealTrack {
            origin: Box::new(origin),
            track_id: 42,
        })
        .unwrap();

    let state = transition.to.track_state().unwrap();
    assert!(state.search.is_empty());
    assert_eq!(state.browse, BrowseFilter::default());
    assert_eq!(
        state.sort,
        TrackSort::new("album", SortDirection::Descending)
    );
    assert_eq!(state.anchor, Some(TrackAnchor::new(42, 0.0)));
    assert_eq!(state.selected_ids, vec![42]);
    assert_eq!(state.focus, TrackFocus::Track(42));
}

#[test]
fn browse_14_the_narrowed_place_the_jump_leaves_stays_on_back() {
    let origin = library_with_state(narrowed_state());
    let mut navigation = BrowserNavigation::new(origin.clone());

    let reveal = navigation
        .navigate(NavigationIntent::RevealTrack {
            origin: Box::new(origin.clone()),
            track_id: 42,
        })
        .unwrap();
    assert_eq!(reveal.direction, NavigationDirection::New);

    let back = navigation.navigate(NavigationIntent::Back).unwrap();
    assert_eq!(back.direction, NavigationDirection::Back);
    assert_eq!(back.to, origin);
    assert_eq!(back.to.track_state().unwrap().search, "needle");
}

#[test]
fn browse_14_a_reveal_without_restrictions_still_replaces_instead_of_pushing() {
    let origin = library_with_state(TrackViewState::default());
    let mut navigation = BrowserNavigation::new(origin.clone());

    let reveal = navigation
        .navigate(NavigationIntent::RevealTrack {
            origin: Box::new(origin),
            track_id: 42,
        })
        .unwrap();

    assert_eq!(reveal.direction, NavigationDirection::Replace);
    assert_eq!(navigation.back_len(), 0);
    assert!(navigation.navigate(NavigationIntent::Back).is_none());
}

#[test]
fn browse_4_reveal_track_restores_its_structured_origin_and_exact_anchor() {
    let mut navigation = BrowserNavigation::new(library_with_state(TrackViewState::default()));
    let origin = BrowserPlace::tracks(TrackCollection::Playlist(7), TrackViewState::default());

    let target = navigation
        .navigate(NavigationIntent::RevealTrack {
            origin: Box::new(origin),
            track_id: 42,
        })
        .unwrap()
        .to;

    assert_eq!(target.collection(), Some(&TrackCollection::Playlist(7)));
    let state = target.track_state().unwrap();
    assert!(state.search.is_empty());
    assert_eq!(state.anchor, Some(TrackAnchor::new(42, 0.0)));
    assert_eq!(state.selected_ids, vec![42]);
    assert_eq!(state.focus, TrackFocus::Track(42));
}

#[test]
fn browse_4_revealing_inside_music_updates_the_remembered_root() {
    let library = library_with_state(TrackViewState::default());
    let mut navigation = BrowserNavigation::new(library.clone());
    let target = navigation
        .navigate(NavigationIntent::RevealTrack {
            origin: Box::new(library),
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
