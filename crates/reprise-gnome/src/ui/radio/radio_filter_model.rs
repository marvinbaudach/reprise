use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use reprise_core::db::Db;
use reprise_core::radio::StationRow;
use reprise_view::search_scope::{self, SearchScope};

use crate::ui::browse::filter_bar::{
    CountText, FacetDescriptor, FilterModel, SelectionDescriptor, ValueDescriptor,
};
use crate::ui::enumerated::enumerated;
use crate::ui::strings;

const GENRE_KEY: &str = "radio.filter.genre";
const COUNTRY_KEY: &str = "radio.filter.country";
const GENRE_FACET: &str = "genre";
const COUNTRY_FACET: &str = "country";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct RadioFilter {
    pub genre: Option<String>,
    pub country: Option<String>,
    pub query: String,
}

impl RadioFilter {
    pub(super) fn is_active(&self) -> bool {
        self.genre.is_some() || self.country.is_some() || self.has_query()
    }
    pub(super) fn has_query(&self) -> bool {
        !self.query.trim().is_empty()
    }
}

enumerated! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum RadioFilterFacet { Genre, Country, Query }
    const RADIO_FILTER_FACETS;
}

pub(super) fn remove_filter(filter: &RadioFilter, facet: RadioFilterFacet) -> RadioFilter {
    let mut result = filter.clone();
    match facet {
        RadioFilterFacet::Genre => result.genre = None,
        RadioFilterFacet::Country => result.country = None,
        RadioFilterFacet::Query => result.query.clear(),
    }
    result
}

pub(super) fn filter_rows(rows: &[StationRow], filter: &RadioFilter) -> Vec<StationRow> {
    rows.iter()
        .filter(|row| {
            matches_value(row.genre.as_deref(), filter.genre.as_deref())
                && matches_value(row.country_code.as_deref(), filter.country.as_deref())
                && search_scope::matches_query(&row.name, &filter.query)
        })
        .cloned()
        .collect()
}

fn only_facet(filter: &RadioFilter, facet: RadioFilterFacet) -> RadioFilter {
    match facet {
        RadioFilterFacet::Genre => RadioFilter {
            genre: filter.genre.clone(),
            ..RadioFilter::default()
        },
        RadioFilterFacet::Country => RadioFilter {
            country: filter.country.clone(),
            ..RadioFilter::default()
        },
        RadioFilterFacet::Query => RadioFilter {
            query: filter.query.clone(),
            ..RadioFilter::default()
        },
    }
}

fn facet_hides_station(row: &StationRow, filter: &RadioFilter, facet: RadioFilterFacet) -> bool {
    filter_rows(std::slice::from_ref(row), &only_facet(filter, facet)).is_empty()
}

pub(super) fn filter_without_hiding(row: &StationRow, filter: &RadioFilter) -> RadioFilter {
    if !filter_rows(std::slice::from_ref(row), filter).is_empty() {
        return filter.clone();
    }
    RADIO_FILTER_FACETS
        .into_iter()
        .filter(|facet| facet_hides_station(row, filter, *facet))
        .fold(filter.clone(), |filter, facet| {
            remove_filter(&filter, facet)
        })
}

pub(super) fn genre_facets(rows: &[StationRow]) -> Vec<String> {
    facets(rows.iter().filter_map(|row| row.genre.as_deref()))
}

pub(super) fn country_facets(rows: &[StationRow]) -> Vec<String> {
    facets(rows.iter().filter_map(|row| row.country_code.as_deref()))
        .into_iter()
        .map(|value| value.to_ascii_uppercase())
        .collect()
}

fn facets<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    values
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .fold(BTreeMap::<String, String>::new(), |mut values, value| {
            values
                .entry(value.to_lowercase())
                .or_insert_with(|| value.to_owned());
            values
        })
        .into_values()
        .collect()
}

fn matches_value(candidate: Option<&str>, expected: Option<&str>) -> bool {
    let Some(expected) = expected else {
        return true;
    };
    candidate.is_some_and(|candidate| candidate.trim().eq_ignore_ascii_case(expected.trim()))
}

pub(super) fn load_filter(db: &Db) -> Result<RadioFilter, rusqlite::Error> {
    Ok(RadioFilter {
        genre: setting(db, GENRE_KEY)?,
        country: setting(db, COUNTRY_KEY)?,
        query: String::new(),
    })
}

pub(super) fn persist_filter(db: &Db, filter: &RadioFilter) -> Result<(), rusqlite::Error> {
    persist_value(db, GENRE_KEY, filter.genre.as_deref())?;
    persist_value(db, COUNTRY_KEY, filter.country.as_deref())
}

fn setting(db: &Db, key: &str) -> Result<Option<String>, rusqlite::Error> {
    Ok(reprise_core::library::settings::get_setting(db, key)?
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty()))
}

fn persist_value(db: &Db, key: &str, value: Option<&str>) -> Result<(), rusqlite::Error> {
    reprise_core::library::settings::set_setting(db, key, value.unwrap_or_default())
}

pub(super) struct RadioModel {
    conn: Rc<Db>,
    rows: RefCell<Vec<StationRow>>,
}

impl RadioModel {
    pub(super) fn new(conn: Rc<Db>) -> Self {
        Self {
            conn,
            rows: RefCell::new(Vec::new()),
        }
    }
    pub(super) fn set_rows(&self, rows: &[StationRow]) {
        self.rows.replace(rows.to_vec());
    }
}

impl FilterModel for RadioModel {
    type Filter = RadioFilter;

    fn initial_filter(&self) -> Self::Filter {
        load_filter(&self.conn).unwrap_or_default()
    }
    fn facets(&self) -> Vec<FacetDescriptor> {
        vec![
            FacetDescriptor::single(GENRE_FACET, strings::text(strings::RADIO_FILTER_GENRE)),
            FacetDescriptor::single(COUNTRY_FACET, strings::text(strings::RADIO_FILTER_COUNTRY)),
        ]
    }
    fn values(&self, facet_id: &str) -> Vec<ValueDescriptor> {
        let rows = self.rows.borrow();
        match facet_id {
            GENRE_FACET => genre_facets(&rows),
            COUNTRY_FACET => country_facets(&rows),
            _ => Vec::new(),
        }
        .into_iter()
        .map(|value| ValueDescriptor::new(value.clone(), value))
        .collect()
    }
    fn apply(&self, query: &str, selections: &[(String, String)]) -> Self::Filter {
        let value = |facet: &str| {
            selections
                .iter()
                .find(|(id, _)| id == facet)
                .map(|(_, value)| value.clone())
        };
        RadioFilter {
            genre: value(GENRE_FACET),
            country: value(COUNTRY_FACET),
            query: query.trim().to_owned(),
        }
    }
    fn persistence_key(&self) -> &'static str {
        GENRE_KEY
    }
    fn query<'a>(&self, filter: &'a Self::Filter) -> &'a str {
        &filter.query
    }
    fn selections(&self, filter: &Self::Filter) -> Vec<SelectionDescriptor> {
        [
            (GENRE_FACET, filter.genre.as_ref()),
            (COUNTRY_FACET, filter.country.as_ref()),
        ]
        .into_iter()
        .filter_map(|(facet, value)| {
            value.map(|value| SelectionDescriptor::new(facet, value, value))
        })
        .collect()
    }
    fn search_scope(&self) -> SearchScope {
        SearchScope::Radio
    }
    fn add_filter_label(&self) -> String {
        format!("+ {}", strings::text(strings::RADIO_ADD_FILTER))
    }
    fn clear_all_label(&self) -> String {
        strings::text(strings::RADIO_CLEAR_ALL)
    }
    fn count_text(&self, shown: usize, total: usize, active: bool) -> CountText {
        if active {
            CountText::markup(strings::radio_filtered_count_markup(shown, total))
        } else {
            CountText::plain(strings::radio_station_count(total))
        }
    }
    fn persist(&self, previous: &Self::Filter, filter: &Self::Filter) -> Result<(), String> {
        if previous.genre == filter.genre && previous.country == filter.country {
            return Ok(());
        }
        persist_filter(&self.conn, filter).map_err(|error| error.to_string())
    }
}
