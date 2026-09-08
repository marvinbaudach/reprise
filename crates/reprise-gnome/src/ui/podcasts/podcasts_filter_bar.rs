use std::ops::Deref;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::db::Db;
use reprise_core::podcasts::PodcastKind;

#[path = "podcasts_filter_bar_model.rs"]
mod filter_model;

use super::podcasts_presentation::{LibrarySummary, PodcastFilter};
use crate::ui::browse::filter_bar::FilterBar;
#[cfg(test)]
use crate::ui::filter_bar_layout;
use crate::ui::strings;
#[cfg(test)]
use crate::ui::style::buttons;
use filter_model::PodcastsModel;
#[cfg(test)]
use reprise_core::podcasts;
#[cfg(test)]
use reprise_view::search_scope::SearchScope;

pub(super) struct PodcastsFilterBar {
    inner: Rc<FilterBar<PodcastsModel>>,
    clear_selection: gtk4::Button,
}

impl Deref for PodcastsFilterBar {
    type Target = FilterBar<PodcastsModel>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl PodcastsFilterBar {
    pub(super) fn new(conn: Rc<Db>, kind: PodcastKind) -> Rc<Self> {
        let inner = FilterBar::new(PodcastsModel::new(conn, kind));
        let clear_selection =
            gtk4::Button::with_label(&strings::text(strings::PODCAST_CLEAR_SELECTION));
        clear_selection.add_css_class("flat");
        clear_selection.set_visible(false);
        clear_selection.set_action_name(Some("podcasts.clear-selection"));
        inner.layout.fill_trailing_action(&clear_selection);
        Rc::new(Self {
            inner,
            clear_selection,
        })
    }

    pub(super) fn widget(&self) -> &gtk4::Widget {
        self.inner.widget()
    }
    pub(super) fn filter(&self) -> PodcastFilter {
        self.inner.filter()
    }
    pub(super) fn result_text(&self) -> String {
        self.inner.count_text()
    }
    pub(super) fn set_on_changed(&self, callback: impl Fn(PodcastFilter) + 'static) {
        self.inner.set_on_changed(callback);
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
    pub(super) fn set_context(
        self: &Rc<Self>,
        shown: usize,
        summary: LibrarySummary,
        selected_count: usize,
    ) {
        self.inner.model().set_context(summary, selected_count);
        self.clear_selection.set_visible(selected_count > 0);
        self.inner.set_counts(shown, summary.episodes);
    }
    pub(super) fn set_selection_count(&self, selected_count: usize) {
        self.inner.model().set_selection_count(selected_count);
        self.clear_selection.set_visible(selected_count > 0);
        self.inner.refresh();
    }
    pub(super) fn clear_all(self: &Rc<Self>) {
        self.inner.clear_all();
    }
    pub(super) fn apply_filter(self: &Rc<Self>, filter: PodcastFilter) {
        self.inner.replace_filter(filter);
    }
}

#[cfg(test)]
#[path = "podcasts_filter_bar_tests.rs"]
mod tests;
