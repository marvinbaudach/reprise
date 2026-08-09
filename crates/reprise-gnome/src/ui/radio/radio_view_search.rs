//! SEARCH-8a / FIL-1d: the Radio view's query surface.
//!
//! Its own file so `radio_view` does not grow further past the
//! repository's source-size gate; the three methods are the whole
//! contract between the shell and this view about searching.

use super::radio_view::RadioView;

impl RadioView {
    /// SEARCH-8a: applies this view's query (FIL-1d: station names).
    pub(in crate::ui) fn set_search_query(&self, query: &str) {
        self.shared.filter_bar.set_query(query);
    }

    /// SEARCH-8a: the bar removed the query itself, so the header entry has to
    /// follow.
    pub(in crate::ui) fn set_on_search_query_changed(&self, callback: impl Fn(&str) + 'static) {
        self.shared.filter_bar.set_on_query_changed(callback);
    }

    /// FIL-2a: "Clear all" for this view — its query and its facets.
    pub(in crate::ui) fn clear_all_filters(&self) {
        self.shared.filter_bar.clear_all();
    }
}
