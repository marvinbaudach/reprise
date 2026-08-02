//! FIL-1a/FIL-2 visibility law for the filter row — pure decisions, no GTK.
//! The row is a permanent list header of every track source; the hide
//! preference only governs the idle state, an active restriction always
//! forces it visible (docs/ux-rules.md K).

use reprise_core::queries::BrowseFilter;
use reprise_core::view_source::ViewSource;

/// P1a's shared browse boundary must remain safe to move between frontend
/// threads and must never acquire an `Rc` or another thread-local dependency.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<BrowseFilter>();
    assert_send_sync::<ViewSource>();
    assert_send_sync::<String>();
};

pub fn filters_restrict(search: &str, browse: &BrowseFilter, exclude_ai: bool) -> bool {
    !search.trim().is_empty() || !browse.is_empty() || exclude_ai
}

/// Whether `source` is a place the user reached from inside the track list and
/// that therefore has no sidebar row naming it. Only these carry a place pill:
/// everywhere else the sidebar selection *is* the location display, and a pill
/// would be a second one (docs/ux-rules.md K, FIL-1c).
pub fn has_place_pill(source: &ViewSource) -> bool {
    matches!(
        source,
        ViewSource::Artist(_) | ViewSource::Album { .. } | ViewSource::Genre(_)
    )
}

/// A place is not a restriction: only search, facets and the AI-exclude filter
/// withhold rows the location would otherwise show. Kept as its own name
/// because callers read better with the intent than with `filters_restrict`.
pub fn is_restricted(search: &str, browse: &BrowseFilter, exclude_ai: bool) -> bool {
    filters_restrict(search, browse, exclude_ai)
}

/// The bare name of the place, undecorated — the caller adds the pill's back
/// affordance, and `library_shell::scope_title` reuses it for the window title.
pub fn place_pill_label(source: &ViewSource) -> Option<String> {
    match source {
        ViewSource::Artist(artist) => Some(artist.clone()),
        ViewSource::Genre(genre) => Some(genre.clone()),
        ViewSource::Album {
            album,
            album_artist,
        } if album_artist.trim().is_empty() => Some(album.clone()),
        ViewSource::Album {
            album,
            album_artist,
        } => Some(format!("{album} — {album_artist}")),
        _ => None,
    }
}

pub fn is_track_source(source: &ViewSource) -> bool {
    !matches!(
        source,
        ViewSource::ImportErrors | ViewSource::MyStats | ViewSource::Conversions
    )
}

pub fn row_visible(
    is_track_source: bool,
    restricted: bool,
    has_place_pill: bool,
    preference_visible: bool,
) -> bool {
    is_track_source && (restricted || has_place_pill || preference_visible)
}

#[cfg(test)]
mod tests {
    use super::*;

    // UX FIL-2: the hide preference only governs the idle state — an active
    // restriction always forces the row visible.
    #[test]
    fn fil_2_row_is_forced_visible_when_restricted_despite_hidden_preference() {
        assert!(row_visible(true, true, false, false));
        assert!(row_visible(true, true, false, true));
    }

    // UX FIL-2: idle visibility follows the preference; panel sources never show.
    #[test]
    fn fil_2_row_follows_preference_when_idle() {
        assert!(row_visible(true, false, false, true));
        assert!(!row_visible(true, false, false, false));
        assert!(!row_visible(false, true, false, true));
    }

    // UX FIL-1a: the row is the track table's header — panel and non-list
    // sources have no row for it to describe.
    #[test]
    fn fil_1a_row_never_shows_for_panel_sources() {
        assert!(!is_track_source(&ViewSource::ImportErrors));
        assert!(!is_track_source(&ViewSource::MyStats));
        assert!(is_track_source(&ViewSource::Library));
        assert!(is_track_source(&ViewSource::Playlist(3)));
        assert!(is_track_source(&ViewSource::Queue));
        assert!(is_track_source(&ViewSource::Missing));
    }

    #[test]
    fn stats_8_my_stats_source_hides_the_track_filter_row() {
        assert!(!is_track_source(&ViewSource::MyStats));
    }

    // UX FIL-2: a whitespace-only search does not restrict (mirrors the
    // trim in reload's has_filter).
    #[test]
    fn fil_2_whitespace_search_does_not_restrict() {
        assert!(!is_restricted("   ", &BrowseFilter::default(), false));
        assert!(is_restricted("falling", &BrowseFilter::default(), false));
        let browse = BrowseFilter {
            genre: Some("Metal".into()),
            ..BrowseFilter::default()
        };
        assert!(is_restricted("", &browse, false));
    }

    // UX FIL-7: the AI-exclude filter is a restriction on its own — the row
    // force-shows and the count switches to "X of Y" (FIL-2) even with an empty
    // search and no facet.
    #[test]
    fn fil_7_exclude_ai_restricts_on_its_own() {
        assert!(is_restricted("", &BrowseFilter::default(), true));
        assert!(!is_restricted("", &BrowseFilter::default(), false));
    }

    // UX FIL-1c: a place carries a pill but is not a filter — it never turns
    // the row into the restricted state on its own.
    #[test]
    fn fil_1c_places_carry_a_pill_without_restricting() {
        let artist = ViewSource::Artist("Lorna Shore".into());

        assert!(has_place_pill(&artist));
        assert_eq!(place_pill_label(&artist).as_deref(), Some("Lorna Shore"));
        assert!(!is_restricted("", &BrowseFilter::default(), false));
    }

    // UX FIL-1c: album places name album and artist; genre places name the genre.
    #[test]
    fn fil_1c_album_and_genre_places_label_themselves() {
        let album = ViewSource::Album {
            album: "Pain Remains".into(),
            album_artist: "Lorna Shore".into(),
        };

        assert_eq!(
            place_pill_label(&album).as_deref(),
            Some("Pain Remains — Lorna Shore")
        );
        assert_eq!(
            place_pill_label(&ViewSource::Genre("Metalcore".into())).as_deref(),
            Some("Metalcore")
        );
    }

    // UX FIL-1c: places reachable through a sidebar row carry no pill — the
    // sidebar already names the location.
    #[test]
    fn fil_1c_sidebar_places_carry_no_pill() {
        for source in [
            ViewSource::Playlist(7),
            ViewSource::Queue,
            ViewSource::Library,
            ViewSource::Missing,
        ] {
            assert!(!has_place_pill(&source));
            assert_eq!(place_pill_label(&source), None);
        }
    }

    // UX FIL-8: Recently added is a sidebar place and carries no pill.
    #[test]
    fn fil_8_recently_added_is_a_sidebar_place_without_a_pill() {
        let source = ViewSource::RecentlyAdded;

        assert!(!has_place_pill(&source));
        assert_eq!(place_pill_label(&source), None);
    }

    // UX FIL-3: a place is not a restriction, but a filter inside one is — the
    // end-of-results line has to appear there, counting against that place.
    #[test]
    fn fil_3_a_filter_inside_a_place_still_restricts() {
        let browse = BrowseFilter::default();

        assert!(is_restricted("track 2", &browse, false));
        assert!(!is_restricted("", &browse, false));
    }

    // UX FIL-2: a place pill forces the row visible on its own, even with the
    // hide preference set and no filter active.
    #[test]
    fn fil_2_row_shows_for_a_place_pill_without_any_filter() {
        assert!(row_visible(true, false, true, false));
        assert!(!row_visible(true, false, false, false));
        assert!(!row_visible(false, false, true, true));
    }
}
