use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::db::Db;
use reprise_core::radio::StationRow;

use crate::ui::enumerated::enumerated;
use crate::ui::filter_bar_layout::{self, FilterBarLayout};
use crate::ui::strings;
use reprise_view::search_scope::{self, SearchScope};

const GENRE_KEY: &str = "radio.filter.genre";
const COUNTRY_KEY: &str = "radio.filter.country";
type FilterCallback = Rc<dyn Fn(RadioFilter)>;
/// SEARCH-8a: fired when the bar itself changes the query — the chip's ×
/// or "Clear all" — so the header entry stops showing a query the view no
/// longer applies.
type OnQueryChanged = Rc<dyn Fn(&str)>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct RadioFilter {
    pub genre: Option<String>,
    pub country: Option<String>,
    /// SEARCH-8a: this view's transient query, matched against station
    /// names alone (FIL-1d). Never persisted — `persist_filter` writes the
    /// two facets above and nothing else.
    pub query: String,
}

impl RadioFilter {
    pub(super) fn is_active(&self) -> bool {
        self.genre.is_some() || self.country.is_some() || self.has_query()
    }

    pub(super) fn has_query(&self) -> bool {
        !self.query.trim().is_empty()
    }

    pub(super) fn with_query(&self, query: &str) -> Self {
        Self {
            query: query.trim().to_owned(),
            ..self.clone()
        }
    }
}

enumerated! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum RadioFilterFacet {
        Genre,
        Country,
        /// SEARCH-8a: the query relaxes like any other facet when a jump to
        /// the connected station would otherwise land nowhere.
        Query,
    }

    /// Generated from the declaration above: a facet that is missing here is
    /// never relaxed, so a jump to the connected station silently does
    /// nothing when that facet is what hides it.
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

/// FIL-1d: the query reads station names — the chip says "in station names",
/// and nothing else here may quietly widen that.
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

/// A filter containing only the selected facet, keeping `filter_rows` as the
/// single source of matching behavior.
fn only_facet(filter: &RadioFilter, facet: RadioFilterFacet) -> RadioFilter {
    match facet {
        RadioFilterFacet::Genre => RadioFilter {
            genre: filter.genre.clone(),
            country: None,
            ..RadioFilter::default()
        },
        RadioFilterFacet::Country => RadioFilter {
            genre: None,
            country: filter.country.clone(),
            ..RadioFilter::default()
        },
        RadioFilterFacet::Query => RadioFilter {
            query: filter.query.clone(),
            ..RadioFilter::default()
        },
    }
}

pub(super) fn facet_hides_station(
    row: &StationRow,
    filter: &RadioFilter,
    facet: RadioFilterFacet,
) -> bool {
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

/// SEARCH-8a: restores the two persisted facets. A launch never starts inside
/// somebody's old query, so `query` stays empty here by construction.
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct FilterChoice {
    facet: RadioFilterFacet,
    value: String,
}

pub(super) struct RadioFilterBar {
    root: gtk4::Box,
    layout: FilterBarLayout,
    add_filter: gtk4::MenuButton,
    chips: gtk4::Box,
    count: gtk4::Label,
    clear_all: gtk4::Button,
    chooser: gtk4::ListBox,
    choices: RefCell<Vec<FilterChoice>>,
    conn: Rc<Db>,
    filter: RefCell<RadioFilter>,
    committed_query: RefCell<String>,
    visible_count: Cell<usize>,
    total_count: Cell<usize>,
    on_changed: RefCell<Option<FilterCallback>>,
    on_query_changed: RefCell<Option<OnQueryChanged>>,
}

impl RadioFilterBar {
    pub(super) fn new(conn: Rc<Db>) -> Rc<Self> {
        let add_filter = gtk4::MenuButton::builder()
            .label(format!("+ {}", strings::text(strings::RADIO_ADD_FILTER)))
            .build();
        add_filter.add_css_class("pill");
        filter_bar_layout::style_add_filter(&add_filter);
        let chooser = gtk4::ListBox::new();
        chooser.set_selection_mode(gtk4::SelectionMode::None);
        let popover = gtk4::Popover::builder().child(&chooser).build();
        add_filter.set_popover(Some(&popover));

        let chips = filter_bar_layout::facet_row();
        let count = filter_bar_layout::count_label();
        count.set_halign(gtk4::Align::End);
        let clear_all =
            filter_bar_layout::clear_all_button(&strings::text(strings::RADIO_CLEAR_ALL));
        clear_all.set_visible(false);

        let layout = FilterBarLayout::new();
        let root = layout.root().clone();
        layout.fill_facets(&chips);
        layout.fill_add_filter(&add_filter);
        layout.fill_count(&count);
        layout.fill_clear_all(&clear_all);

        let filter = load_filter(&conn).unwrap_or_default();
        let bar = Rc::new(Self {
            root,
            layout,
            add_filter,
            chips,
            count,
            clear_all,
            chooser,
            choices: RefCell::new(Vec::new()),
            conn,
            filter: RefCell::new(filter),
            committed_query: RefCell::new(String::new()),
            visible_count: Cell::new(0),
            total_count: Cell::new(0),
            on_changed: RefCell::new(None),
            on_query_changed: RefCell::new(None),
        });
        {
            let weak = Rc::downgrade(&bar);
            bar.clear_all.connect_clicked(move |_| {
                if let Some(bar) = weak.upgrade() {
                    bar.clear_all();
                }
            });
        }
        wire_chooser(&bar);
        bar.rebuild_chips();
        bar
    }

    pub(super) fn widget(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    pub(super) fn set_on_changed(&self, callback: impl Fn(RadioFilter) + 'static) {
        *self.on_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn filter(&self) -> RadioFilter {
        self.filter.borrow().clone()
    }

    pub(super) fn clear_all(self: &Rc<Self>) {
        self.apply(RadioFilter::default());
    }

    pub(super) fn apply_filter(self: &Rc<Self>, filter: RadioFilter) {
        self.apply(filter);
    }

    pub(super) fn set_rows(&self, rows: &[StationRow]) {
        self.rebuild_choices(rows);
    }

    pub(super) fn set_counts(&self, visible: usize, total: usize) {
        self.visible_count.set(visible);
        self.total_count.set(total);
        // FIL-2a: filtered lists count "N of TOTAL stations" with the shown
        // number accented; an unfiltered one keeps its plain total.
        let text;
        let presentation = if self.filter().is_active() {
            text = strings::radio_filtered_count_markup(visible, total);
            filter_bar_layout::CountPresentation::RestrictedMarkup(&text)
        } else {
            text = strings::radio_station_count(total);
            filter_bar_layout::CountPresentation::Plain(&text)
        };
        filter_bar_layout::present_count(&self.count, presentation);
    }

    pub(super) fn set_on_query_changed(&self, callback: impl Fn(&str) + 'static) {
        *self.on_query_changed.borrow_mut() = Some(Rc::new(callback));
    }

    /// SEARCH-8a: this view's query, handed in by the shell.
    pub(super) fn set_query(self: &Rc<Self>, query: &str) {
        let current = self.filter();
        if current.query == query.trim() {
            return;
        }
        self.apply_internal(current.with_query(query), false);
    }

    pub(super) fn set_committed_query(self: &Rc<Self>, query: &str) {
        if *self.committed_query.borrow() == query {
            return;
        }
        self.committed_query.replace(query.to_owned());
        self.rebuild_chips();
    }

    fn committed_query(&self) -> String {
        self.committed_query.borrow().clone()
    }

    fn apply(self: &Rc<Self>, filter: RadioFilter) {
        self.apply_internal(filter, true);
    }

    /// `announce_query`: whether a query change started here (the chip's ×,
    /// "Clear all", a relaxed jump) and therefore has to be mirrored back
    /// into the header entry. A query arriving *from* the entry is not
    /// echoed, or the two would ping-pong.
    fn apply_internal(self: &Rc<Self>, filter: RadioFilter, announce_query: bool) {
        // SEARCH-8a: only the facets are persisted; the query is transient.
        if let Err(error) = persist_filter(&self.conn, &filter) {
            tracing::warn!(%error, "could not persist radio filters");
        }
        let previous_query = self.filter.replace(filter.clone()).query;
        // Drop the receipt in the same turn as the query — see the note in
        // `concerts_filter_bar::clear_query`. An empty query has no chip under
        // either surface, so this cannot disagree with the commit sink.
        if filter.query.trim().is_empty() {
            self.committed_query.replace(String::new());
        }
        self.rebuild_chips();
        if announce_query && previous_query != filter.query {
            if let Some(callback) = self.on_query_changed.borrow().clone() {
                callback(&filter.query);
            }
        }
        if let Some(callback) = self.on_changed.borrow().clone() {
            callback(filter);
        }
    }

    fn rebuild_choices(&self, rows: &[StationRow]) {
        self.chooser.remove_all();
        let choices = genre_facets(rows)
            .into_iter()
            .map(|value| FilterChoice {
                facet: RadioFilterFacet::Genre,
                value,
            })
            .chain(country_facets(rows).into_iter().map(|value| FilterChoice {
                facet: RadioFilterFacet::Country,
                value,
            }))
            .collect::<Vec<_>>();
        for choice in &choices {
            let row = gtk4::ListBoxRow::new();
            let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            let facet = gtk4::Label::new(Some(&strings::text(match choice.facet {
                RadioFilterFacet::Genre => strings::RADIO_FILTER_GENRE,
                RadioFilterFacet::Country => strings::RADIO_FILTER_COUNTRY,
                // The query is never offered here: it is typed in the header,
                // not chosen from the facet list.
                RadioFilterFacet::Query => continue,
            })));
            facet.add_css_class("dim-label");
            let value = gtk4::Label::new(Some(&choice.value));
            value.set_hexpand(true);
            value.set_xalign(0.0);
            content.append(&facet);
            content.append(&value);
            row.set_child(Some(&content));
            self.chooser.append(&row);
        }
        self.choices.replace(choices);
        self.add_filter
            .set_sensitive(!self.choices.borrow().is_empty());
    }

    fn rebuild_chips(self: &Rc<Self>) {
        while let Some(child) = self.chips.first_child() {
            self.chips.remove(&child);
        }
        let filter = self.filter();
        let committed_query = self.committed_query();
        // FIL-1a/FIL-1d: the search chip comes first, ahead of the facets.
        let weak = Rc::downgrade(self);
        self.layout
            .replace_scoped_search(SearchScope::Radio, &committed_query, move || {
                if let Some(bar) = weak.upgrade() {
                    let cleared = bar.filter().with_query("");
                    bar.apply(cleared);
                }
            });
        for (facet, value) in [
            (RadioFilterFacet::Genre, filter.genre.as_deref()),
            (RadioFilterFacet::Country, filter.country.as_deref()),
        ] {
            let Some(value) = value else {
                continue;
            };
            let button = gtk4::Button::with_label(value);
            button.add_css_class(filter_bar_layout::CHIP_CSS_CLASS);
            button.set_icon_name("window-close-symbolic");
            let weak = Rc::downgrade(self);
            let current = filter.clone();
            button.connect_clicked(move |_| {
                if let Some(bar) = weak.upgrade() {
                    bar.apply(remove_filter(&current, facet));
                }
            });
            self.chips.append(&button);
        }
        self.chips.set_visible(self.chips.first_child().is_some());
        self.clear_all.set_visible(filter.is_active());
        self.set_counts(self.visible_count.get(), self.total_count.get());
    }
}

fn wire_chooser(bar: &Rc<RadioFilterBar>) {
    let weak = Rc::downgrade(bar);
    bar.chooser.connect_row_activated(move |_, row| {
        let Some(bar) = weak.upgrade() else {
            return;
        };
        let Some(choice) = bar.choices.borrow().get(row.index() as usize).cloned() else {
            return;
        };
        let mut filter = bar.filter();
        match choice.facet {
            RadioFilterFacet::Genre => filter.genre = Some(choice.value),
            RadioFilterFacet::Country => filter.country = Some(choice.value),
            RadioFilterFacet::Query => return,
        }
        bar.add_filter.popdown();
        bar.apply(filter);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radio_filter_facets_are_sticky_and_each_chip_removes_one_constraint() {
        let conn = crate::test_db::open().unwrap();
        let filter = RadioFilter {
            genre: Some("Metal".into()),
            country: Some("CH".into()),
            ..RadioFilter::default()
        };

        persist_filter(&conn, &filter).unwrap();
        assert_eq!(load_filter(&conn).unwrap(), filter);
        assert_eq!(
            remove_filter(&filter, RadioFilterFacet::Genre),
            RadioFilter {
                genre: None,
                country: Some("CH".into()),
                ..RadioFilter::default()
            }
        );
        assert_eq!(
            remove_filter(&filter, RadioFilterFacet::Country),
            RadioFilter {
                genre: Some("Metal".into()),
                country: None,
                ..RadioFilter::default()
            }
        );
    }

    #[test]
    fn filters_match_case_insensitively_and_facets_are_distinct() {
        let rows = vec![
            test_station(1, Some("Metal"), Some("CH")),
            test_station(2, Some("metal"), Some("DE")),
            test_station(3, Some("Jazz"), Some("CH")),
        ];
        let filtered = filter_rows(
            &rows,
            &RadioFilter {
                genre: Some("METAL".into()),
                country: None,
                ..RadioFilter::default()
            },
        );
        assert_eq!(
            filtered.iter().map(|row| row.id).collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(genre_facets(&rows), ["Jazz", "Metal"]);
        assert_eq!(country_facets(&rows), ["CH", "DE"]);
    }

    #[test]
    fn src_13_only_the_hiding_facet_is_dropped_for_a_station() {
        let station = test_station(1, Some("Metal"), Some("DE"));
        let filter = RadioFilter {
            genre: Some("Metal".into()),
            country: Some("CH".into()),
            ..RadioFilter::default()
        };

        assert_eq!(
            filter_without_hiding(&station, &filter),
            RadioFilter {
                genre: Some("Metal".into()),
                country: None,
                ..RadioFilter::default()
            }
        );
    }

    #[test]
    fn src_13_a_visible_station_leaves_every_facet_standing() {
        let station = test_station(1, Some("Metal"), Some("CH"));
        let filter = RadioFilter {
            genre: Some("Metal".into()),
            country: Some("CH".into()),
            ..RadioFilter::default()
        };

        assert_eq!(filter_without_hiding(&station, &filter), filter);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_4a_radio_clear_path_removes_query_and_chip() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let bar = RadioFilterBar::new(Rc::new(crate::test_db::open().unwrap()));
        bar.set_query("nova");
        bar.set_committed_query("nova");

        bar.layout
            .slot_child(crate::ui::filter_bar_layout::FilterBarSlot::Search)
            .and_downcast::<gtk4::Button>()
            .expect("Radio search chip")
            .emit_clicked();

        bar.layout.assert_search_cleared(&bar.filter().query);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_2a_radio_fills_filters_count_and_clear_slots_in_order() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let bar = RadioFilterBar::new(conn);
        bar.set_query("nova");
        bar.set_committed_query("nova");
        bar.set_counts(3, 44);

        assert_eq!(
            bar.root.height_request(),
            filter_bar_layout::FILTER_BAR_MIN_HEIGHT
        );
        assert!(bar.layout.slot_contains(
            crate::ui::filter_bar_layout::FilterBarSlot::Facets,
            &bar.chips
        ));
        assert!(bar.layout.slot_contains(
            crate::ui::filter_bar_layout::FilterBarSlot::AddFilter,
            &bar.add_filter
        ));
        assert!(bar.layout.slot_contains(
            crate::ui::filter_bar_layout::FilterBarSlot::Count,
            &bar.count
        ));
        assert!(bar.layout.slot_contains(
            crate::ui::filter_bar_layout::FilterBarSlot::ClearAll,
            &bar.clear_all
        ));
        let first = bar
            .layout
            .slot_child(crate::ui::filter_bar_layout::FilterBarSlot::Search)
            .expect("search chip");
        assert!(first
            .downcast::<gtk4::Button>()
            .ok()
            .and_then(|button| button.label())
            .is_some_and(|label| label.starts_with('⌕')));
        assert!(bar.add_filter.has_css_class("pill"));
        assert!(bar.clear_all.is_visible());
    }

    /// UX FIL-1d: the Radio query matches **station names**, case-insensitively
    /// and mid-word, and composes with the facet chips instead of replacing
    /// them. A genre the query does not name is still withheld by its chip.
    #[test]
    fn fil_1d_radio_query_matches_station_names_and_composes_with_facets() {
        let mut nova = test_station(1, Some("Jazz"), Some("de"));
        nova.name = "Radio Nova".into();
        let mut werk = test_station(2, Some("Jazz"), Some("de"));
        werk.name = "Werkstatt FM".into();
        let mut antwerp = test_station(3, Some("Rock"), Some("be"));
        antwerp.name = "Antwerpen Live".into();
        let rows = vec![nova, werk, antwerp];

        let names = |filter: &RadioFilter| {
            filter_rows(&rows, filter)
                .into_iter()
                .map(|row| row.name)
                .collect::<Vec<_>>()
        };

        assert_eq!(
            names(&RadioFilter {
                query: "wer".into(),
                ..RadioFilter::default()
            }),
            ["Werkstatt FM", "Antwerpen Live"],
            "leading and mid-word matches, case-insensitively"
        );
        assert_eq!(
            names(&RadioFilter {
                genre: Some("Jazz".into()),
                query: "wer".into(),
                ..RadioFilter::default()
            }),
            ["Werkstatt FM"],
            "the query narrows what the genre chip already returned"
        );
        assert_eq!(names(&RadioFilter::default()).len(), 3);
        assert!(RadioFilter {
            query: "wer".into(),
            ..RadioFilter::default()
        }
        .is_active());
        assert!(!RadioFilter::default().is_active());
    }

    /// UX SEARCH-8a: the query is transient — `load_filter` restores the two
    /// persisted facets and never a query.
    #[test]
    fn search_8a_radio_query_is_never_restored_from_settings() {
        let db = crate::test_db::open().unwrap();
        persist_filter(
            &db,
            &RadioFilter {
                genre: Some("Jazz".into()),
                query: "wer".into(),
                ..RadioFilter::default()
            },
        )
        .unwrap();

        let restored = load_filter(&db).unwrap();

        assert_eq!(restored.genre.as_deref(), Some("Jazz"));
        assert_eq!(restored.query, "");
    }

    fn test_station(
        id: i64,
        genre: Option<&str>,
        country: Option<&str>,
    ) -> reprise_core::radio::StationRow {
        reprise_core::radio::StationRow {
            id,
            uuid: None,
            name: format!("Station {id}"),
            stream_url: format!("https://radio.example/{id}"),
            homepage: None,
            favicon_url: None,
            genre: genre.map(str::to_owned),
            codec: None,
            bitrate_kbps: None,
            country_code: country.map(str::to_owned),
            votes: None,
            added_at: 10,
            removed_at: None,
        }
    }
}
