use std::ops::Deref;
use std::rc::Rc;

use reprise_core::db::Db;
use reprise_core::radio::StationRow;

use super::radio_filter_model::RadioModel;
#[cfg(test)]
use super::radio_filter_model::{
    country_facets, genre_facets, load_filter, persist_filter, remove_filter, RadioFilterFacet,
};
pub(super) use super::radio_filter_model::{filter_rows, filter_without_hiding, RadioFilter};
use crate::ui::browse::filter_bar::FilterBar;
#[cfg(test)]
use crate::ui::filter_bar_layout;
#[cfg(test)]
use reprise_view::search_scope::SearchScope;

pub(super) struct RadioFilterBar {
    inner: Rc<FilterBar<RadioModel>>,
}

impl Deref for RadioFilterBar {
    type Target = FilterBar<RadioModel>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl RadioFilterBar {
    pub(super) fn new(conn: Rc<Db>) -> Rc<Self> {
        Rc::new(Self {
            inner: FilterBar::new(RadioModel::new(conn)),
        })
    }

    pub(super) fn widget(&self) -> &gtk4::Widget {
        self.inner.widget()
    }
    pub(super) fn set_on_changed(&self, callback: impl Fn(RadioFilter) + 'static) {
        self.inner.set_on_changed(callback);
    }
    pub(super) fn filter(&self) -> RadioFilter {
        self.inner.filter()
    }
    pub(super) fn clear_all(self: &Rc<Self>) {
        self.inner.clear_all();
    }
    pub(super) fn apply_filter(self: &Rc<Self>, filter: RadioFilter) {
        self.inner.replace_filter(filter);
    }
    pub(super) fn set_rows(&self, rows: &[StationRow]) {
        self.inner.model().set_rows(rows);
        self.inner.refresh();
    }
    pub(super) fn set_counts(&self, visible: usize, total: usize) {
        self.inner.set_counts(visible, total);
    }
    pub(super) fn set_on_query_changed(&self, callback: impl Fn(&str) + 'static) {
        self.inner.set_on_query_changed(callback);
    }
    pub(super) fn set_query(self: &Rc<Self>, query: &str) {
        self.inner.set_query(query);
    }
    pub(super) fn set_committed_query(self: &Rc<Self>, query: &str) {
        self.inner.set_committed_query(query);
    }
}

#[cfg(test)]
#[path = "radio_filter_bar_tests.rs"]
mod tests;
