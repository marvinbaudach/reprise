/// Whether the editable search surface is currently presented.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchSurface {
    Open,
    Closed,
}

/// SEARCH-11/12/13: what the filter bar's search slot should show.
pub fn committed_query(query: &str, surface: SearchSurface) -> Option<&str> {
    if surface == SearchSurface::Open {
        return None;
    }
    let query = query.trim();
    (!query.is_empty()).then_some(query)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_surface_never_commits_the_live_query() {
        assert_eq!(committed_query("wer", SearchSurface::Open), None);
    }

    #[test]
    fn closed_surface_commits_a_non_empty_query() {
        assert_eq!(committed_query("wer", SearchSurface::Closed), Some("wer"));
    }

    #[test]
    fn closed_surface_does_not_commit_whitespace() {
        assert_eq!(committed_query("   ", SearchSurface::Closed), None);
    }

    #[test]
    fn closed_surface_commits_the_trimmed_query() {
        assert_eq!(
            committed_query("  wer  ", SearchSurface::Closed),
            Some("wer")
        );
    }
}
