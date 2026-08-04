use super::*;

fn library() -> BrowserPlace {
    BrowserPlace::tracks(
        TrackCollection::Library(LibraryScope::All),
        TrackViewState::default(),
    )
}

#[test]
fn browse_4_reveal_episode_targets_the_place_of_its_kind() {
    let mut podcasts = BrowserNavigation::new(library());
    let podcast = podcasts
        .navigate(NavigationIntent::RevealEpisode {
            subscription_id: 7,
            episode_id: Some(11),
            kind: SourceKind::Podcasts,
        })
        .unwrap();
    assert_eq!(podcast.to, BrowserPlace::Podcasts);

    let mut youtube = BrowserNavigation::new(library());
    let video = youtube
        .navigate(NavigationIntent::RevealEpisode {
            subscription_id: 8,
            episode_id: Some(12),
            kind: SourceKind::Youtube,
        })
        .unwrap();
    assert_eq!(video.to, BrowserPlace::Youtube);
}

#[test]
fn browse_4_reveal_episode_from_the_library_records_back_history() {
    let mut navigation = BrowserNavigation::new(library());
    let transition = navigation
        .navigate(NavigationIntent::RevealEpisode {
            subscription_id: 7,
            episode_id: Some(11),
            kind: SourceKind::Podcasts,
        })
        .unwrap();

    assert_eq!(transition.direction, NavigationDirection::New);
    assert_eq!(navigation.back_len(), 1);
    assert_eq!(
        navigation.navigate(NavigationIntent::Back).unwrap().to,
        library()
    );
}

#[test]
fn browse_4_reveal_station_from_the_library_records_back_history() {
    let mut navigation = BrowserNavigation::new(library());
    let transition = navigation
        .navigate(NavigationIntent::RevealStation { station_id: 5 })
        .unwrap();

    assert_eq!(transition.to, BrowserPlace::Radio);
    assert_eq!(transition.direction, NavigationDirection::New);
    assert_eq!(navigation.back_len(), 1);
}

#[test]
fn browse_4_a_reveal_in_the_open_source_view_yields_no_transition() {
    let mut podcast = BrowserNavigation::new(BrowserPlace::Podcasts);
    assert!(podcast
        .navigate(NavigationIntent::RevealEpisode {
            subscription_id: 7,
            episode_id: None,
            kind: SourceKind::Podcasts,
        })
        .is_none());

    let mut radio = BrowserNavigation::new(BrowserPlace::Radio);
    assert!(radio
        .navigate(NavigationIntent::RevealStation { station_id: 5 })
        .is_none());
}

#[test]
fn browse_4_invalid_source_ids_have_no_target() {
    let invalid = [
        NavigationIntent::RevealEpisode {
            subscription_id: 0,
            episode_id: Some(11),
            kind: SourceKind::Podcasts,
        },
        NavigationIntent::RevealEpisode {
            subscription_id: 7,
            episode_id: Some(0),
            kind: SourceKind::Youtube,
        },
        NavigationIntent::RevealStation { station_id: -1 },
    ];

    for intent in invalid {
        assert_eq!(intent.source_target(), None);
        let mut navigation = BrowserNavigation::new(library());
        assert!(navigation.navigate(intent).is_none());
        assert_eq!(navigation.current(), &library());
    }
}

#[test]
fn browse_4_two_youtube_places_are_the_same_destination() {
    assert!(same_destination(
        &BrowserPlace::Youtube,
        &BrowserPlace::Youtube
    ));
}
