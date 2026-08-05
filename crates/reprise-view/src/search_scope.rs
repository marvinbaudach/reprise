//! Which section owns a search query, and which fields that query matches.
//!
//! SEARCH-8: the header search belongs to the section it was typed in, not to
//! the window. A section is identified by a [`SearchScope`]; the runtime keeps
//! one query per scope and swaps the entry text when the visible section
//! changes. FIL-1d: the scope also decides what the search chip claims to
//! match, so a chip never promises a field its view does not read.

use reprise_core::view_source::ViewSource;

/// The list views a query can belong to, plus the one state for sections that
/// have no list at all (My Stats, device Sync, Import errors — their rows are
/// their own panels, not a filterable list).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SearchScope {
    /// Music and every other track table: Library, Recently added, playlists,
    /// smart playlists, the Queue, and the Artist/Album/Genre places.
    #[default]
    Tracks,
    /// Missing files — a list of gaps keyed by their path, not of tracks.
    Missing,
    Podcasts,
    Youtube,
    Radio,
    Releases,
    Concerts,
    /// A section without a list. The lens is insensitive here (SEARCH-8).
    Unsupported,
}

/// The scope a source belongs to. Every track source shares one scope: they
/// are one section of the shell and the track list already resets its query
/// when the source changes.
#[must_use]
pub fn scope_for(source: &ViewSource) -> SearchScope {
    match source {
        ViewSource::Library
        | ViewSource::RecentlyAdded
        | ViewSource::Playlist(_)
        | ViewSource::Smart(_)
        | ViewSource::Queue
        | ViewSource::Album { .. }
        | ViewSource::Artist(_)
        | ViewSource::Genre(_) => SearchScope::Tracks,
        ViewSource::Missing => SearchScope::Missing,
        ViewSource::Podcasts => SearchScope::Podcasts,
        ViewSource::Youtube => SearchScope::Youtube,
        ViewSource::Radio => SearchScope::Radio,
        ViewSource::Releases => SearchScope::Releases,
        ViewSource::Concerts => SearchScope::Concerts,
        ViewSource::MyStats | ViewSource::ImportErrors | ViewSource::Conversions => {
            SearchScope::Unsupported
        }
    }
}

/// Whether this section has a list for a query to narrow. Where it does not,
/// the header lens is insensitive, Ctrl+F is a no-op, and the bar cannot be
/// revealed.
#[must_use]
pub const fn supports_search(scope: SearchScope) -> bool {
    !matches!(scope, SearchScope::Unsupported)
}

/// Case-insensitive substring match — the one matching rule every scoped
/// search uses, mid-word included ("wer" matches "Antwerpen").
#[must_use]
pub fn matches_query(haystack: &str, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    haystack.to_lowercase().contains(&query.to_lowercase())
}

/// Whether any of the fields a scope reads matches the query.
#[must_use]
pub fn matches_any<'a>(fields: impl IntoIterator<Item = &'a str>, query: &str) -> bool {
    if query.trim().is_empty() {
        return true;
    }
    fields.into_iter().any(|field| matches_query(field, query))
}

#[cfg(test)]
mod tests {
    use super::*;

    // UX SEARCH-8: every section the shell can show resolves to exactly one
    // scope, so a query can never be stored under two names or none.
    #[test]
    fn search_8_every_source_resolves_to_one_scope() {
        assert_eq!(scope_for(&ViewSource::Library), SearchScope::Tracks);
        assert_eq!(scope_for(&ViewSource::Queue), SearchScope::Tracks);
        assert_eq!(scope_for(&ViewSource::Playlist(3)), SearchScope::Tracks);
        assert_eq!(scope_for(&ViewSource::Smart(4)), SearchScope::Tracks);
        assert_eq!(scope_for(&ViewSource::RecentlyAdded), SearchScope::Tracks);
        assert_eq!(
            scope_for(&ViewSource::Artist("Lorna Shore".into())),
            SearchScope::Tracks
        );
        assert_eq!(scope_for(&ViewSource::Missing), SearchScope::Missing);
        assert_eq!(scope_for(&ViewSource::Podcasts), SearchScope::Podcasts);
        assert_eq!(scope_for(&ViewSource::Youtube), SearchScope::Youtube);
        assert_eq!(scope_for(&ViewSource::Radio), SearchScope::Radio);
        assert_eq!(scope_for(&ViewSource::Releases), SearchScope::Releases);
        assert_eq!(scope_for(&ViewSource::Concerts), SearchScope::Concerts);
    }

    // UX SEARCH-8: where there is no list there is no search.
    #[test]
    fn search_8_sections_without_a_list_do_not_support_search() {
        assert_eq!(scope_for(&ViewSource::MyStats), SearchScope::Unsupported);
        assert_eq!(
            scope_for(&ViewSource::ImportErrors),
            SearchScope::Unsupported
        );
        assert!(!supports_search(SearchScope::Unsupported));
        for scope in [
            SearchScope::Tracks,
            SearchScope::Missing,
            SearchScope::Podcasts,
            SearchScope::Youtube,
            SearchScope::Radio,
            SearchScope::Releases,
            SearchScope::Concerts,
        ] {
            assert!(supports_search(scope), "{scope:?}");
        }
    }

    #[test]
    fn matching_is_case_insensitive_and_matches_mid_word() {
        assert!(matches_query(
            "Antwerpen: Wie ein Hafen funktioniert",
            "wer"
        ));
        assert!(matches_query("Werkzeuge", "WER"));
        assert!(matches_query("Auswertung der Hörerumfrage", " wer "));
        assert!(!matches_query("Signal & Rauschen", "wer"));
        // An empty query withholds nothing.
        assert!(matches_query("anything", "   "));
    }

    #[test]
    fn matches_any_reads_every_field_the_scope_names() {
        assert!(matches_any(["Pain Remains", "Lorna Shore"], "lorna"));
        assert!(matches_any(["Pain Remains", "Lorna Shore"], "remains"));
        assert!(!matches_any(["Pain Remains", "Lorna Shore"], "cattle"));
        assert!(matches_any(std::iter::empty(), ""));
    }
}
