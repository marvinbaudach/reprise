//! Track-list knowledge about which columns participate in its broad query.
//! The shared search-hit presentation lives in `ui::search_highlight`.

pub(in crate::ui) fn is_searchable_column(sort_id: &str) -> bool {
    matches!(sort_id, "artist" | "album" | "genre")
}

#[cfg(test)]
mod tests {
    use super::*;

    // UX FIL-5a: only fields used by the track query request highlighting;
    // numeric metadata must not imply that it contributed a match.
    #[test]
    fn fil_5a_only_searched_columns_request_highlighting() {
        assert!(is_searchable_column("artist"));
        assert!(is_searchable_column("album"));
        assert!(is_searchable_column("genre"));
        assert!(!is_searchable_column("year"));
        assert!(!is_searchable_column("track_number"));
        assert!(!is_searchable_column("duration"));
        assert!(!is_searchable_column("play_count"));
    }
}
