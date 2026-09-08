use std::ops::Deref;
use std::rc::Rc;

#[cfg(test)]
use reprise_core::artist_news::{persisted_releases_filter, ReleaseTypeSelection};
use reprise_core::artist_news::{
    ReleaseWindow, ReleasesFilter, RELEASES_FILTER_HIDDEN_KEY, RELEASES_FILTER_TYPE_KEY,
    RELEASES_FILTER_WINDOW_KEY,
};
use reprise_core::db::Db;
#[cfg(test)]
use reprise_view::search_scope::SearchScope;

use super::releases_filter_model::{ReleasesModel, ReleasesState};
use crate::ui::browse::filter_bar::FilterBar;
use crate::ui::strings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(test)]
enum TypeChip {
    Album,
    Ep,
}

#[cfg(test)]
fn toggle_type(
    selection: ReleaseTypeSelection,
    chip: TypeChip,
    active: bool,
) -> ReleaseTypeSelection {
    match chip {
        TypeChip::Album => ReleaseTypeSelection {
            album: active,
            ..selection
        },
        TypeChip::Ep => ReleaseTypeSelection {
            ep: active,
            ..selection
        },
    }
}

pub(super) fn persist_filter(db: &Db, filter: &ReleasesFilter) -> Result<(), rusqlite::Error> {
    reprise_core::library::settings::set_setting(
        db,
        RELEASES_FILTER_TYPE_KEY,
        &filter.release_types.setting_value(),
    )?;
    reprise_core::library::settings::set_setting(
        db,
        RELEASES_FILTER_WINDOW_KEY,
        filter.window.setting_value(),
    )?;
    reprise_core::library::settings::set_bool(db, RELEASES_FILTER_HIDDEN_KEY, filter.hidden)
}

pub(super) struct ReleasesFilterBar {
    inner: Rc<FilterBar<ReleasesModel>>,
}

impl Deref for ReleasesFilterBar {
    type Target = FilterBar<ReleasesModel>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl ReleasesFilterBar {
    pub(super) fn new(conn: Rc<Db>) -> Rc<Self> {
        Rc::new(Self {
            inner: FilterBar::new(ReleasesModel::new(conn)),
        })
    }

    pub(super) fn widget(&self) -> &gtk4::Widget {
        self.inner.widget()
    }
    pub(super) fn filter(&self) -> ReleasesFilter {
        self.inner.filter().filter
    }
    pub(super) fn set_on_changed(&self, callback: impl Fn(ReleasesFilter) + 'static) {
        self.inner
            .set_on_changed(move |state| callback(state.filter));
    }
    pub(super) fn set_on_query_changed(&self, callback: impl Fn(&str) + 'static) {
        self.inner.set_on_query_changed(callback);
    }
    pub(super) fn query(&self) -> String {
        self.inner.filter().query
    }
    pub(super) fn set_query(self: &Rc<Self>, query: &str) {
        self.inner.set_query(query);
    }
    pub(super) fn set_committed_query(self: &Rc<Self>, query: &str) {
        self.inner.set_committed_query(query);
    }
    pub(super) fn set_counts(self: &Rc<Self>, shown: usize, total: usize) {
        self.inner.set_counts(shown, total);
    }
    pub(super) fn clear_all(self: &Rc<Self>) {
        self.inner.clear_all();
    }
    pub(super) fn show_widest(self: &Rc<Self>) {
        self.inner.replace_filter(ReleasesState {
            filter: ReleasesFilter::widest(false),
            query: String::new(),
        });
    }
    #[cfg(test)]
    fn apply_filter(self: &Rc<Self>, filter: ReleasesFilter) {
        self.inner.replace_filter(ReleasesState {
            filter,
            query: self.query(),
        });
    }
}

pub(super) fn release_count_presentation(shown: usize, total: usize) -> String {
    if shown == total {
        strings::release_total_line(total)
    } else {
        strings::release_count_line(shown, total)
    }
}

pub(super) fn window_label(window: ReleaseWindow) -> String {
    strings::text(match window {
        ReleaseWindow::OneYear => strings::RELEASES_WINDOW_ONE_YEAR,
        ReleaseWindow::FiveYears => strings::RELEASES_WINDOW_FIVE_YEARS,
        ReleaseWindow::TenYears => strings::RELEASES_WINDOW_TEN_YEARS,
        ReleaseWindow::All => strings::RELEASES_WINDOW_ALL,
    })
}

#[cfg(test)]
#[path = "releases_filter_bar_tests.rs"]
mod tests;
