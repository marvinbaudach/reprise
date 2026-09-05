use std::ops::Deref;
use std::rc::Rc;

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
use gtk4::prelude::*;
use reprise_core::concerts::config;
use reprise_core::concerts::{ConcertFilter, DateHorizon};
use reprise_core::db::Db;

use super::concerts_filter_model::ConcertsModel;
use crate::ui::browse::filter_bar::FilterBar;
#[cfg(test)]
use crate::ui::filter_bar_layout;
use crate::ui::strings;
#[cfg(test)]
use reprise_view::search_scope::SearchScope;

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FilterFacet {
    Radius,
    Country,
    Horizon,
    Source,
}

#[cfg(test)]
fn remove_filter(filter: &ConcertFilter, facet: FilterFacet) -> ConcertFilter {
    match facet {
        FilterFacet::Radius => ConcertFilter {
            radius_km: None,
            ..filter.clone()
        },
        FilterFacet::Country => ConcertFilter {
            country: None,
            ..filter.clone()
        },
        FilterFacet::Horizon => ConcertFilter {
            horizon: DateHorizon::AllUpcoming,
            ..filter.clone()
        },
        FilterFacet::Source => ConcertFilter {
            include_similar: false,
            ..filter.clone()
        },
    }
}

#[cfg(test)]
fn active_facets(filter: &ConcertFilter, has_location: bool) -> Vec<FilterFacet> {
    let mut facets = Vec::new();
    if has_location && filter.radius_km.is_some() {
        facets.push(FilterFacet::Radius);
    }
    if filter.country.is_some() {
        facets.push(FilterFacet::Country);
    }
    if filter.horizon != DateHorizon::AllUpcoming {
        facets.push(FilterFacet::Horizon);
    }
    if filter.include_similar {
        facets.push(FilterFacet::Source);
    }
    facets
}

#[cfg(test)]
fn chip_label(filter: &ConcertFilter, facet: FilterFacet, location_name: Option<&str>) -> String {
    match facet {
        FilterFacet::Radius => {
            let radius = filter.radius_km.unwrap_or_default().round().max(0.0) as u32;
            location_name
                .filter(|name| !name.trim().is_empty())
                .map_or_else(
                    || strings::concerts_radius_km(radius),
                    |name| strings::concerts_location_radius(name, radius),
                )
        }
        FilterFacet::Country => filter.country.clone().unwrap_or_default(),
        FilterFacet::Horizon => horizon_label(filter.horizon),
        FilterFacet::Source => strings::text(strings::CONCERTS_INCLUDE_SIMILAR),
    }
}

pub(super) fn source_facet_visible(similar_enabled: bool, has_similar_rows: bool) -> bool {
    similar_enabled || has_similar_rows
}

pub(super) fn radius_off_label(filter: &ConcertFilter) -> Option<String> {
    filter
        .radius_km
        .map(|radius| strings::concerts_radius_off(radius.round().max(0.0) as u32))
}

pub(super) fn horizon_label(horizon: DateHorizon) -> String {
    strings::text(match horizon {
        DateHorizon::AllUpcoming => strings::CONCERTS_ALL_UPCOMING,
        DateHorizon::Next30Days => strings::CONCERTS_NEXT_30_DAYS,
        DateHorizon::Next3Months => strings::CONCERTS_NEXT_3_MONTHS,
        DateHorizon::Next6Months => strings::CONCERTS_NEXT_6_MONTHS,
    })
}

pub(super) fn persist_filter(db: &Db, filter: &ConcertFilter) -> Result<(), rusqlite::Error> {
    let radius = filter
        .radius_km
        .map(|radius| radius.round().to_string())
        .unwrap_or_default();
    reprise_core::library::settings::set_setting(db, config::FILTER_RADIUS_KEY, &radius)?;
    reprise_core::library::settings::set_setting(
        db,
        config::FILTER_COUNTRY_KEY,
        filter.country.as_deref().unwrap_or_default(),
    )?;
    let horizon = match filter.horizon {
        DateHorizon::AllUpcoming => "",
        DateHorizon::Next30Days => "next_30_days",
        DateHorizon::Next3Months => "next_3_months",
        DateHorizon::Next6Months => "next_6_months",
    };
    reprise_core::library::settings::set_setting(db, config::FILTER_HORIZON_KEY, horizon)?;
    reprise_core::library::settings::set_bool(
        db,
        config::FILTER_INCLUDE_SIMILAR_KEY,
        filter.include_similar,
    )
}

pub(super) struct ConcertsFilterBar {
    inner: Rc<FilterBar<ConcertsModel>>,
}

impl Deref for ConcertsFilterBar {
    type Target = FilterBar<ConcertsModel>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl ConcertsFilterBar {
    pub(super) fn new(conn: Rc<Db>) -> Rc<Self> {
        Rc::new(Self {
            inner: FilterBar::new(ConcertsModel::new(conn)),
        })
    }

    pub(super) fn widget(&self) -> &gtk4::Widget {
        self.inner.widget()
    }

    pub(super) fn filter(&self) -> ConcertFilter {
        self.inner.filter().filter
    }

    pub(super) fn set_on_changed(&self, callback: impl Fn(ConcertFilter) + 'static) {
        self.inner
            .set_on_changed(move |state| callback(state.filter));
    }

    pub(super) fn query(&self) -> String {
        self.inner.filter().query
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

    pub(super) fn set_counts(self: &Rc<Self>, shown: usize, total: usize) {
        self.inner.set_counts(shown, total);
    }

    pub(super) fn clear_all(self: &Rc<Self>) {
        self.inner.clear_all();
    }

    pub(super) fn set_context(
        self: &Rc<Self>,
        location: Option<&reprise_core::location::AppLocation>,
        similar_enabled: bool,
        has_similar_rows: bool,
    ) {
        self.inner
            .model()
            .set_context(location, similar_enabled, has_similar_rows);
        self.inner.refresh();
    }

    pub(super) fn set_on_open_location(&self, callback: impl Fn() + 'static) {
        self.inner.model().set_on_open_location(callback);
    }

    pub(super) fn reload_persisted(self: &Rc<Self>) -> Result<(), rusqlite::Error> {
        let state = self.inner.model().persisted_state(self.query())?;
        self.inner.replace_filter(state);
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn result_text_for_test(&self) -> String {
        self.inner.count_text()
    }
}

#[cfg(test)]
#[path = "concerts_filter_bar_tests.rs"]
mod tests;
