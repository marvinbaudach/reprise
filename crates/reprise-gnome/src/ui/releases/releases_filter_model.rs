use std::rc::Rc;

use reprise_core::artist_news::{
    persisted_releases_filter, ReleaseTypeSelection, ReleaseWindow, ReleasesFilter,
    RELEASES_FILTER_TYPE_KEY,
};
use reprise_core::db::Db;
use reprise_view::search_scope::SearchScope;

use super::releases_filter_bar::{persist_filter, release_count_presentation, window_label};
use crate::ui::browse::filter_bar::{
    CountText, FacetDescriptor, FilterModel, SelectionDescriptor, ValueDescriptor,
};
use crate::ui::strings;

const TYPE_FACET: &str = "type";
const WINDOW_FACET: &str = "window";
const HIDDEN_FACET: &str = "hidden";

#[derive(Clone, PartialEq)]
pub(super) struct ReleasesState {
    pub(super) filter: ReleasesFilter,
    pub(super) query: String,
}

pub(super) struct ReleasesModel {
    conn: Rc<Db>,
}

impl ReleasesModel {
    pub(super) fn new(conn: Rc<Db>) -> Self {
        Self { conn }
    }
}

impl FilterModel for ReleasesModel {
    type Filter = ReleasesState;

    fn initial_filter(&self) -> Self::Filter {
        ReleasesState {
            filter: persisted_releases_filter(&self.conn).unwrap_or_default(),
            query: String::new(),
        }
    }

    fn facets(&self) -> Vec<FacetDescriptor> {
        vec![
            FacetDescriptor::multiple(TYPE_FACET, strings::text(strings::RELEASES_TYPE)),
            FacetDescriptor::single(WINDOW_FACET, strings::text(strings::RELEASES_DATE)),
            FacetDescriptor::single(HIDDEN_FACET, strings::text(strings::RELEASES_HIDDEN)),
        ]
    }

    fn values(&self, facet_id: &str) -> Vec<ValueDescriptor> {
        match facet_id {
            TYPE_FACET => [
                ("album", strings::RELEASES_ALBUM),
                ("ep", strings::RELEASES_EP),
                ("single", strings::RELEASES_SINGLE),
            ]
            .into_iter()
            .map(|(id, label)| ValueDescriptor::new(id, strings::text(label)))
            .collect(),
            WINDOW_FACET => [
                ReleaseWindow::OneYear,
                ReleaseWindow::FiveYears,
                ReleaseWindow::TenYears,
                ReleaseWindow::All,
            ]
            .into_iter()
            .map(|window| ValueDescriptor::new(window.setting_value(), window_label(window)))
            .collect(),
            HIDDEN_FACET => vec![ValueDescriptor::new(
                "true",
                strings::text(strings::RELEASES_HIDDEN),
            )],
            _ => Vec::new(),
        }
    }

    fn apply(&self, query: &str, selections: &[(String, String)]) -> Self::Filter {
        let selected = |facet: &str, value: &str| {
            selections.iter().any(|(selected_facet, selected_value)| {
                selected_facet == facet && selected_value == value
            })
        };
        let window = selections
            .iter()
            .find(|(facet, _)| facet == WINDOW_FACET)
            .map_or(ReleaseWindow::FiveYears, |(_, value)| {
                match value.as_str() {
                    "1y" => ReleaseWindow::OneYear,
                    "10y" => ReleaseWindow::TenYears,
                    "all" => ReleaseWindow::All,
                    _ => ReleaseWindow::FiveYears,
                }
            });
        Self::Filter {
            filter: ReleasesFilter {
                release_types: ReleaseTypeSelection {
                    album: selected(TYPE_FACET, "album"),
                    ep: selected(TYPE_FACET, "ep"),
                    single: selected(TYPE_FACET, "single"),
                },
                window,
                hidden: selected(HIDDEN_FACET, "true"),
            },
            query: query.trim().to_owned(),
        }
    }

    fn persistence_key(&self) -> &'static str {
        RELEASES_FILTER_TYPE_KEY
    }
    fn query<'a>(&self, state: &'a Self::Filter) -> &'a str {
        &state.query
    }

    fn selections(&self, state: &Self::Filter) -> Vec<SelectionDescriptor> {
        let filter = &state.filter;
        let mut selections = Vec::new();
        for (active, id, label) in [
            (filter.release_types.album, "album", strings::RELEASES_ALBUM),
            (filter.release_types.ep, "ep", strings::RELEASES_EP),
            (
                filter.release_types.single,
                "single",
                strings::RELEASES_SINGLE,
            ),
        ] {
            if active {
                selections.push(SelectionDescriptor::new(
                    TYPE_FACET,
                    id,
                    strings::text(label),
                ));
            }
        }
        selections.push(SelectionDescriptor::picker(
            WINDOW_FACET,
            filter.window.setting_value(),
            window_label(filter.window),
        ));
        if filter.hidden {
            selections.push(SelectionDescriptor::new(
                HIDDEN_FACET,
                "true",
                strings::text(strings::RELEASES_HIDDEN),
            ));
        }
        selections
    }

    fn search_scope(&self) -> SearchScope {
        SearchScope::Releases
    }
    fn add_filter_label(&self) -> String {
        strings::text(strings::RELEASES_ADD_FILTER)
    }
    fn clear_all_label(&self) -> String {
        strings::text(strings::RELEASES_CLEAR_ALL)
    }

    fn count_text(&self, shown: usize, total: usize, active: bool) -> CountText {
        if active && shown != total {
            CountText::markup(strings::release_count_line_markup(shown, total))
        } else {
            CountText::plain(release_count_presentation(shown, total))
        }
    }

    fn clear_filter(&self) -> Self::Filter {
        ReleasesState {
            filter: ReleasesFilter::default(),
            query: String::new(),
        }
    }

    fn is_active(&self, state: &Self::Filter) -> bool {
        state.filter != ReleasesFilter::default() || !state.query.is_empty()
    }

    fn persist(&self, previous: &Self::Filter, state: &Self::Filter) -> Result<(), String> {
        if previous.filter == state.filter {
            return Ok(());
        }
        persist_filter(&self.conn, &state.filter).map_err(|error| error.to_string())
    }
}
