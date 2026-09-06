use std::cell::{Cell, RefCell};
use std::rc::Rc;

use reprise_core::concerts::config;
use reprise_core::concerts::{ConcertFilter, DateHorizon};
use reprise_core::db::Db;
use reprise_view::search_scope::SearchScope;

use super::concerts_filter_bar::{
    horizon_label, persist_filter, radius_off_label, source_facet_visible,
};
use crate::ui::browse::filter_bar::{
    CountText, FacetDescriptor, FilterModel, SelectionDescriptor, ValueDescriptor,
};
use crate::ui::strings;

const RADIUS_FACET: &str = "radius";
const COUNTRY_FACET: &str = "country";
const HORIZON_FACET: &str = "horizon";
const SOURCE_FACET: &str = "source";
const SAVED_RADIUS: &str = "saved-radius";

type Callback = Rc<dyn Fn()>;

#[derive(Clone, PartialEq)]
pub(super) struct ConcertsState {
    pub(super) filter: ConcertFilter,
    pub(super) query: String,
}

pub(super) struct ConcertsModel {
    conn: Rc<Db>,
    has_location: Cell<bool>,
    location_name: RefCell<Option<String>>,
    similar_enabled: Cell<bool>,
    has_similar_rows: Cell<bool>,
    on_open_location: RefCell<Option<Callback>>,
}

impl ConcertsModel {
    pub(super) fn new(conn: Rc<Db>) -> Self {
        Self {
            conn,
            has_location: Cell::new(false),
            location_name: RefCell::new(None),
            similar_enabled: Cell::new(false),
            has_similar_rows: Cell::new(false),
            on_open_location: RefCell::new(None),
        }
    }

    pub(super) fn set_context(
        &self,
        location: Option<&reprise_core::location::AppLocation>,
        similar_enabled: bool,
        has_similar_rows: bool,
    ) {
        self.has_location.set(location.is_some());
        self.location_name
            .replace(location.map(|location| location.name.clone()));
        self.similar_enabled.set(similar_enabled);
        self.has_similar_rows.set(has_similar_rows);
    }

    pub(super) fn set_on_open_location(&self, callback: impl Fn() + 'static) {
        self.on_open_location.replace(Some(Rc::new(callback)));
    }

    pub(super) fn db(&self) -> &Db {
        &self.conn
    }
}

impl FilterModel for ConcertsModel {
    type Filter = ConcertsState;

    fn initial_filter(&self) -> Self::Filter {
        ConcertsState {
            filter: config::persisted_filter(&self.conn).unwrap_or_default(),
            query: String::new(),
        }
    }

    fn facets(&self) -> Vec<FacetDescriptor> {
        let radius = FacetDescriptor::single(RADIUS_FACET, strings::text(strings::CONCERTS_RADIUS));
        let radius = if self.has_location.get() {
            radius
        } else {
            radius.disabled(strings::text(strings::CONCERTS_SET_LOCATION_TOOLTIP))
        };
        let mut facets = vec![
            radius,
            FacetDescriptor::single(COUNTRY_FACET, strings::text(strings::CONCERTS_COUNTRY)),
            FacetDescriptor::single(HORIZON_FACET, strings::text(strings::CONCERTS_DATE_RANGE)),
        ];
        if source_facet_visible(self.similar_enabled.get(), self.has_similar_rows.get()) {
            facets.push(FacetDescriptor::single(
                SOURCE_FACET,
                strings::text(strings::CONCERTS_SOURCE),
            ));
        }
        facets
    }

    fn values(&self, facet_id: &str) -> Vec<ValueDescriptor> {
        match facet_id {
            RADIUS_FACET => std::iter::once(None)
                .chain(
                    reprise_core::location::RADIUS_PRESETS_KM
                        .into_iter()
                        .map(|radius| Some(f64::from(radius))),
                )
                .map(|radius| {
                    ValueDescriptor::new(
                        radius.map_or_else(|| "off".to_owned(), |value| value.to_string()),
                        radius.map_or_else(
                            || strings::text(strings::CONCERTS_OFF),
                            |value| strings::concerts_radius_km(value as u32),
                        ),
                    )
                })
                .collect(),
            COUNTRY_FACET => reprise_core::concerts::known_countries(&self.conn)
                .unwrap_or_default()
                .into_iter()
                .map(|country| ValueDescriptor::new(&country, country.clone()))
                .collect(),
            HORIZON_FACET => [
                ("all", DateHorizon::AllUpcoming),
                ("30d", DateHorizon::Next30Days),
                ("3m", DateHorizon::Next3Months),
                ("6m", DateHorizon::Next6Months),
            ]
            .into_iter()
            .map(|(id, horizon)| ValueDescriptor::new(id, horizon_label(horizon)))
            .collect(),
            SOURCE_FACET => [("library", false), ("similar", true)]
                .into_iter()
                .map(|(id, similar)| {
                    ValueDescriptor::new(
                        id,
                        strings::text(if similar {
                            strings::CONCERTS_INCLUDE_SIMILAR
                        } else {
                            strings::CONCERTS_LIBRARY_ARTISTS_ONLY
                        }),
                    )
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn apply(&self, query: &str, selections: &[(String, String)]) -> Self::Filter {
        let selected = |facet: &str| {
            selections
                .iter()
                .find(|(selected_facet, _)| selected_facet == facet)
                .map(|(_, value)| value.as_str())
        };
        let radius_km = selected(RADIUS_FACET)
            .or_else(|| selected(SAVED_RADIUS))
            .and_then(|value| (value != "off").then(|| value.parse().ok()).flatten());
        let horizon = match selected(HORIZON_FACET) {
            Some("30d") => DateHorizon::Next30Days,
            Some("3m") => DateHorizon::Next3Months,
            Some("6m") => DateHorizon::Next6Months,
            _ => DateHorizon::AllUpcoming,
        };
        ConcertsState {
            filter: ConcertFilter {
                radius_km,
                country: selected(COUNTRY_FACET).map(str::to_owned),
                horizon,
                include_similar: selected(SOURCE_FACET) == Some("similar"),
            },
            query: query.trim().to_owned(),
        }
    }

    fn persistence_key(&self) -> &'static str {
        config::FILTER_RADIUS_KEY
    }

    fn query<'a>(&self, state: &'a Self::Filter) -> &'a str {
        &state.query
    }

    fn selections(&self, state: &Self::Filter) -> Vec<SelectionDescriptor> {
        let filter = &state.filter;
        let mut selections = Vec::new();
        if let Some(radius) = filter.radius_km {
            let value = radius.to_string();
            if self.has_location.get() {
                let label = self
                    .location_name
                    .borrow()
                    .as_deref()
                    .filter(|name| !name.trim().is_empty())
                    .map_or_else(
                        || strings::concerts_radius_km(radius.round().max(0.0) as u32),
                        |name| {
                            strings::concerts_location_radius(name, radius.round().max(0.0) as u32)
                        },
                    );
                selections.push(SelectionDescriptor::new(RADIUS_FACET, value, label));
            } else if let Some(label) = radius_off_label(filter) {
                selections.push(SelectionDescriptor::action(
                    SAVED_RADIUS,
                    value,
                    label,
                    "reprise-concerts-radius-off",
                ));
            }
        }
        if let Some(country) = &filter.country {
            selections.push(SelectionDescriptor::new(COUNTRY_FACET, country, country));
        }
        if filter.horizon != DateHorizon::AllUpcoming {
            let id = match filter.horizon {
                DateHorizon::AllUpcoming => "all",
                DateHorizon::Next30Days => "30d",
                DateHorizon::Next3Months => "3m",
                DateHorizon::Next6Months => "6m",
            };
            selections.push(SelectionDescriptor::new(
                HORIZON_FACET,
                id,
                horizon_label(filter.horizon),
            ));
        }
        if filter.include_similar {
            selections.push(SelectionDescriptor::new(
                SOURCE_FACET,
                "similar",
                strings::text(strings::CONCERTS_INCLUDE_SIMILAR),
            ));
        }
        selections
    }

    fn search_scope(&self) -> SearchScope {
        SearchScope::Concerts
    }

    fn add_filter_label(&self) -> String {
        strings::text(strings::CONCERTS_ADD_FILTER)
    }

    fn clear_all_label(&self) -> String {
        strings::text(strings::CONCERTS_CLEAR_ALL)
    }

    fn count_text(&self, shown: usize, total: usize, active: bool) -> CountText {
        if active {
            CountText::markup(strings::concert_count_line_markup(shown, total))
        } else {
            CountText::plain(strings::concert_total_line(total))
        }
    }

    fn clear_filter(&self) -> Self::Filter {
        ConcertsState {
            filter: ConcertFilter::default(),
            query: String::new(),
        }
    }

    fn is_active(&self, state: &Self::Filter) -> bool {
        let filter = &state.filter;
        !state.query.is_empty()
            || (self.has_location.get() && filter.radius_km.is_some())
            || filter.country.is_some()
            || filter.horizon != DateHorizon::AllUpcoming
            || filter.include_similar
    }

    fn persist(&self, previous: &Self::Filter, state: &Self::Filter) -> Result<(), String> {
        if previous.filter == state.filter {
            return Ok(());
        }
        persist_filter(&self.conn, &state.filter).map_err(|error| error.to_string())
    }

    fn activate_selection(&self, selection: &SelectionDescriptor) -> bool {
        if selection.facet_id != SAVED_RADIUS {
            return false;
        }
        let callback = self.on_open_location.borrow().clone();
        if let Some(callback) = callback {
            callback();
        }
        true
    }
}
