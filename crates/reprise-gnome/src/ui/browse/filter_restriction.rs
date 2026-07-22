//! FIL-1a/FIL-2 visibility law for the filter row — pure decisions, no GTK.
//! The row is a permanent list header of every track source; the hide
//! preference only governs the idle state, an active restriction always
//! forces it visible (docs/ux-rules.md K).

use reprise_core::queries::BrowseFilter;
use reprise_core::view_source::ViewSource;

pub(in crate::ui) fn is_restricted(search: &str, browse: &BrowseFilter, exclude_ai: bool) -> bool {
    !search.trim().is_empty() || !browse.is_empty() || exclude_ai
}

pub(in crate::ui) fn is_track_source(source: &ViewSource) -> bool {
    !matches!(
        source,
        ViewSource::ImportErrors | ViewSource::MyStats | ViewSource::Device { .. }
    )
}

pub(in crate::ui) fn row_visible(
    is_track_source: bool,
    restricted: bool,
    preference_visible: bool,
) -> bool {
    is_track_source && (restricted || preference_visible)
}

#[cfg(test)]
mod tests {
    use super::*;

    // UX FIL-2: the hide preference only governs the idle state — an active
    // restriction always forces the row visible.
    #[test]
    fn fil_2_row_is_forced_visible_when_restricted_despite_hidden_preference() {
        assert!(row_visible(true, true, false));
        assert!(row_visible(true, true, true));
    }

    // UX FIL-2: idle visibility follows the preference; panel sources never show.
    #[test]
    fn fil_2_row_follows_preference_when_idle() {
        assert!(row_visible(true, false, true));
        assert!(!row_visible(true, false, false));
        assert!(!row_visible(false, true, true));
    }

    // UX FIL-1a: the row is the track table's header — panel and non-list
    // sources have no row for it to describe.
    #[test]
    fn fil_1a_row_never_shows_for_panel_sources() {
        assert!(!is_track_source(&ViewSource::ImportErrors));
        assert!(!is_track_source(&ViewSource::MyStats));
        assert!(!is_track_source(&ViewSource::Device { serial: "x".into() }));
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
}
