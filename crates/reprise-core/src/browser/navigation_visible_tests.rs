use super::*;

fn library() -> BrowserPlace {
    BrowserPlace::tracks(
        TrackCollection::Library(LibraryScope::All),
        TrackViewState::default(),
    )
}

fn open_album(navigation: &mut BrowserNavigation) -> BrowserPlace {
    navigation
        .navigate(NavigationIntent::OpenAlbum {
            album: AlbumKey::new("Blue", "Joni Mitchell"),
            anchor_track_id: None,
        })
        .expect("an album is a new destination")
        .to
}

/// The Library Doctor is a placeless view: showing it never told the router.
/// Back after a jump out of it must return to the Doctor, and the section the
/// user came from stays one step behind it.
#[test]
fn browse_4a_back_after_a_jump_from_the_doctor_returns_to_the_doctor() {
    let mut navigation = BrowserNavigation::new(library());
    navigation
        .navigate(NavigationIntent::Sidebar(SidebarTarget::Podcasts))
        .unwrap();

    assert!(navigation.observe_visible(BrowserPlace::LibraryDoctor));
    let album = open_album(&mut navigation);

    let back = navigation.navigate(NavigationIntent::Back).unwrap();
    assert_eq!(back.from, album);
    assert_eq!(back.to, BrowserPlace::LibraryDoctor);
    let back = navigation.navigate(NavigationIntent::Back).unwrap();
    assert_eq!(back.to, BrowserPlace::Podcasts);
}

#[test]
fn browse_4a_a_section_the_router_already_holds_is_not_pushed_again() {
    let mut navigation = BrowserNavigation::new(library());
    navigation
        .navigate(NavigationIntent::Sidebar(SidebarTarget::Radio))
        .unwrap();
    let before = navigation.back_len();

    assert!(navigation.observe_visible(BrowserPlace::Radio));
    open_album(&mut navigation);

    assert_eq!(navigation.back_len(), before + 1);
    assert_eq!(
        navigation.navigate(NavigationIntent::Back).unwrap().to,
        BrowserPlace::Radio
    );
}

/// A track list can lag behind the section page that replaced it, so a
/// diverging track place never overwrites or pushes the router's place.
#[test]
fn browse_4a_a_stale_track_place_behind_a_section_page_is_not_trusted() {
    let mut navigation = BrowserNavigation::new(library());
    navigation
        .navigate(NavigationIntent::Sidebar(SidebarTarget::Podcasts))
        .unwrap();
    let before = navigation.back_len();

    assert!(!navigation.observe_visible(library()));

    assert_eq!(navigation.current(), &BrowserPlace::Podcasts);
    assert_eq!(navigation.back_len(), before);
}

#[test]
fn browse_4a_observing_a_diverging_section_clears_forward_history() {
    let mut navigation = BrowserNavigation::new(library());
    navigation
        .navigate(NavigationIntent::Sidebar(SidebarTarget::Radio))
        .unwrap();
    navigation.navigate(NavigationIntent::Back).unwrap();

    assert!(navigation.observe_visible(BrowserPlace::LibraryDoctor));

    assert!(navigation.navigate(NavigationIntent::Forward).is_none());
}

#[test]
fn browse_4a_an_intent_that_goes_nowhere_records_nothing_and_keeps_forward() {
    let mut navigation = BrowserNavigation::new(library());
    navigation
        .navigate(NavigationIntent::Sidebar(SidebarTarget::Podcasts))
        .unwrap();
    navigation.navigate(NavigationIntent::Back).unwrap();
    let back_len = navigation.back_len();

    let empty_album = NavigationIntent::OpenAlbum {
        album: AlbumKey::new("", "Anyone"),
        anchor_track_id: None,
    };
    for intent in [empty_album, NavigationIntent::Forward] {
        let moved = navigation.navigate_observing(BrowserPlace::LibraryDoctor, intent);
        assert!(moved.is_none());
        assert_eq!(navigation.back_len(), back_len);
        assert_eq!(navigation.current(), &library());
    }
    assert_eq!(
        navigation.navigate(NavigationIntent::Forward).unwrap().to,
        BrowserPlace::Podcasts,
        "Forward history survived"
    );
}

#[test]
fn browse_4a_a_section_the_router_holds_is_refreshed_not_entered() {
    let mut navigation = BrowserNavigation::new(library());
    navigation
        .navigate(NavigationIntent::Sidebar(SidebarTarget::Radio))
        .unwrap();
    let before = navigation.back_len();

    assert!(navigation.holds(&BrowserPlace::Radio));
    assert!(!navigation.holds(&BrowserPlace::LibraryDoctor));
    navigation
        .navigate_observing(
            BrowserPlace::Radio,
            NavigationIntent::Sidebar(SidebarTarget::Podcasts),
        )
        .unwrap();

    assert_eq!(navigation.back_len(), before + 1);
    assert_eq!(navigation.previous(), Some(&BrowserPlace::Radio));
}
