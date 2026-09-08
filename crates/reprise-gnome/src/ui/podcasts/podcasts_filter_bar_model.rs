use std::cell::Cell;
use std::rc::Rc;

use reprise_core::db::Db;
use reprise_core::podcasts::{self, PodcastKind};
use reprise_view::search_scope::SearchScope;

use super::super::podcasts_presentation::{active, LibrarySummary, PodcastFilter};
use crate::ui::browse::filter_bar::{
    CountText, FacetDescriptor, FilterModel, SelectionDescriptor, ValueDescriptor,
};
use crate::ui::strings;

const UNPLAYED_FACET: &str = "unplayed";
const DOWNLOADED_FACET: &str = "downloaded";

pub(in crate::ui::podcasts) struct PodcastsModel {
    conn: Rc<Db>,
    kind: PodcastKind,
    summary: Cell<LibrarySummary>,
    selected_count: Cell<usize>,
    source: Cell<Option<PodcastKind>>,
}

impl PodcastsModel {
    pub(super) fn new(conn: Rc<Db>, kind: PodcastKind) -> Self {
        Self {
            conn,
            kind,
            summary: Cell::new(LibrarySummary::default()),
            selected_count: Cell::new(0),
            source: Cell::new(None),
        }
    }
    pub(super) fn set_context(&self, summary: LibrarySummary, selected_count: usize) {
        self.summary.set(summary);
        self.selected_count.set(selected_count);
    }
    pub(super) fn set_selection_count(&self, selected_count: usize) {
        self.selected_count.set(selected_count);
    }
}

impl FilterModel for PodcastsModel {
    type Filter = PodcastFilter;

    fn initial_filter(&self) -> Self::Filter {
        let stored = podcasts::config::load_filter(&self.conn).unwrap_or_default();
        PodcastFilter::from_facets(&podcasts::config::PodcastFilterConfig {
            source: None,
            ..stored
        })
    }

    fn facets(&self) -> Vec<FacetDescriptor> {
        vec![
            FacetDescriptor::single(
                UNPLAYED_FACET,
                strings::text(strings::PODCAST_FILTER_UNPLAYED),
            ),
            FacetDescriptor::single(
                DOWNLOADED_FACET,
                strings::text(strings::PODCAST_FILTER_DOWNLOADED),
            ),
        ]
    }

    fn values(&self, facet_id: &str) -> Vec<ValueDescriptor> {
        match facet_id {
            UNPLAYED_FACET => vec![ValueDescriptor::new(
                "true",
                strings::text(strings::PODCAST_FILTER_UNPLAYED),
            )],
            DOWNLOADED_FACET => vec![ValueDescriptor::new(
                "true",
                strings::text(strings::PODCAST_FILTER_DOWNLOADED),
            )],
            _ => Vec::new(),
        }
    }

    fn apply(&self, query: &str, selections: &[(String, String)]) -> Self::Filter {
        let selected = |facet: &str| selections.iter().any(|(id, _)| id == facet);
        PodcastFilter {
            unplayed_only: selected(UNPLAYED_FACET),
            source: self.source.get(),
            downloaded_only: selected(DOWNLOADED_FACET),
            query: query.trim().to_owned(),
        }
    }

    fn persistence_key(&self) -> &'static str {
        "podcasts.filter"
    }
    fn query<'a>(&self, filter: &'a Self::Filter) -> &'a str {
        &filter.query
    }
    fn selections(&self, filter: &Self::Filter) -> Vec<SelectionDescriptor> {
        let mut selections = Vec::new();
        if filter.unplayed_only {
            let label = strings::text(strings::PODCAST_FILTER_UNPLAYED);
            selections.push(SelectionDescriptor::new(UNPLAYED_FACET, "true", label));
        }
        if filter.downloaded_only {
            let label = strings::text(strings::PODCAST_FILTER_DOWNLOADED);
            selections.push(SelectionDescriptor::new(DOWNLOADED_FACET, "true", label));
        }
        selections
    }
    fn search_scope(&self) -> SearchScope {
        match self.kind {
            PodcastKind::Rss => SearchScope::Podcasts,
            PodcastKind::Youtube => SearchScope::Youtube,
        }
    }
    fn add_filter_label(&self) -> String {
        format!("+ {}", strings::text(strings::PODCAST_ADD_FILTER))
    }
    fn clear_all_label(&self) -> String {
        strings::text(strings::PODCAST_CLEAR_ALL)
    }
    fn count_text(&self, shown: usize, _total: usize, active: bool) -> CountText {
        let summary = self.summary.get();
        let base = if active {
            match self.kind {
                PodcastKind::Rss => strings::podcast_filtered_count_markup(shown, summary.episodes),
                PodcastKind::Youtube => {
                    strings::youtube_filtered_count_markup(shown, summary.episodes)
                }
            }
        } else {
            match self.kind {
                PodcastKind::Rss => {
                    strings::podcast_library_summary(summary.shows, summary.episodes, summary.new)
                }
                PodcastKind::Youtube => {
                    strings::youtube_library_summary(summary.shows, summary.episodes, summary.new)
                }
            }
        };
        let text = strings::podcast_summary_with_selection(&base, self.selected_count.get());
        if active {
            CountText::markup(text)
        } else {
            CountText::plain(text)
        }
    }
    fn clear_filter(&self) -> Self::Filter {
        self.source.set(None);
        PodcastFilter::default()
    }
    fn is_active(&self, filter: &Self::Filter) -> bool {
        active(filter)
    }
    fn persist(&self, previous: &Self::Filter, filter: &Self::Filter) -> Result<(), String> {
        self.source.set(filter.source);
        if previous.facets() == filter.facets() {
            return Ok(());
        }
        podcasts::config::save_filter(&self.conn, &filter.facets())
            .map_err(|error| error.to_string())
    }
}
