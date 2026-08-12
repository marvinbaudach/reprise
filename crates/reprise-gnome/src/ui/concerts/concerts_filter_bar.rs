#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::concerts::config;
use reprise_core::concerts::{ConcertFilter, DateHorizon};
use reprise_core::db::Db;

use crate::ui::filter_bar_layout::{self, FilterBarLayout};
use crate::ui::strings;
use reprise_view::search_scope::SearchScope;

const FACET_PAGE: &str = "facets";
const VALUE_PAGE: &str = "values";

type OnChanged = Rc<dyn Fn(ConcertFilter)>;
/// SEARCH-8a: requests the shell's shared query transition for the chip's ×,
/// or mirrors a locally completed composite action such as "Clear all".
type OnQueryChanged = Rc<dyn Fn(&str)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FilterFacet {
    Radius,
    Country,
    Horizon,
    Source,
}

#[derive(Clone)]
struct FilterValue {
    label: String,
    apply: Rc<dyn Fn(&ConcertFilter) -> ConcertFilter>,
}

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

fn source_facet_visible(similar_enabled: bool, has_similar_rows: bool) -> bool {
    similar_enabled || has_similar_rows
}

fn active_facets(filter: &ConcertFilter) -> Vec<FilterFacet> {
    let mut facets = Vec::new();
    if filter.radius_km.is_some() {
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

fn facet_label(facet: FilterFacet) -> String {
    strings::text(match facet {
        FilterFacet::Radius => strings::CONCERTS_RADIUS,
        FilterFacet::Country => strings::CONCERTS_COUNTRY,
        FilterFacet::Horizon => strings::CONCERTS_DATE_RANGE,
        FilterFacet::Source => strings::CONCERTS_SOURCE,
    })
}

fn chip_label(filter: &ConcertFilter, facet: FilterFacet) -> String {
    match facet {
        FilterFacet::Radius => {
            strings::concerts_radius_km(filter.radius_km.unwrap_or_default().round().max(0.0) as u32)
        }
        FilterFacet::Country => filter.country.clone().unwrap_or_default(),
        FilterFacet::Horizon => horizon_label(filter.horizon),
        FilterFacet::Source => strings::text(strings::CONCERTS_INCLUDE_SIMILAR),
    }
}

fn horizon_label(horizon: DateHorizon) -> String {
    strings::text(match horizon {
        DateHorizon::AllUpcoming => strings::CONCERTS_ALL_UPCOMING,
        DateHorizon::Next30Days => strings::CONCERTS_NEXT_30_DAYS,
        DateHorizon::Next3Months => strings::CONCERTS_NEXT_3_MONTHS,
        DateHorizon::Next6Months => strings::CONCERTS_NEXT_6_MONTHS,
    })
}

fn persist_filter(db: &Db, filter: &ConcertFilter) -> Result<(), rusqlite::Error> {
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
    root: gtk4::Box,
    layout: FilterBarLayout,
    conn: Rc<Db>,
    filter: RefCell<ConcertFilter>,
    chips: gtk4::Box,
    add_filter: gtk4::MenuButton,
    popover: gtk4::Popover,
    chooser_stack: gtk4::Stack,
    facet_list: gtk4::ListBox,
    value_list: gtk4::ListBox,
    chooser_back: gtk4::Button,
    chooser_facets: RefCell<Vec<FilterFacet>>,
    chooser_values: RefCell<Vec<FilterValue>>,
    result_label: gtk4::Label,
    clear_all: gtk4::Button,
    has_location: Cell<bool>,
    similar_enabled: Cell<bool>,
    has_similar_rows: Cell<bool>,
    counts: Cell<(usize, usize)>,
    /// SEARCH-8a: this view's query, kept beside the persisted
    /// `ConcertFilter` rather than inside it — a query must not be restored
    /// on the next launch.
    query: RefCell<String>,
    committed_query: RefCell<String>,
    on_changed: RefCell<Option<OnChanged>>,
    on_query_changed: RefCell<Option<OnQueryChanged>>,
}

impl ConcertsFilterBar {
    pub(super) fn new(conn: Rc<Db>) -> Rc<Self> {
        let filter = config::persisted_filter(&conn).unwrap_or_default();
        let layout = FilterBarLayout::new();
        let root = layout.root().clone();

        let chips = filter_bar_layout::facet_row();
        layout.fill_facets(&chips);

        let chooser_stack = gtk4::Stack::new();
        chooser_stack.set_transition_type(gtk4::StackTransitionType::SlideLeftRight);
        chooser_stack.set_transition_duration(crate::ui::motion::STANDARD_MS);
        let facet_list = gtk4::ListBox::new();
        facet_list.add_css_class("boxed-list");
        let facet_box = page_box();
        facet_box.append(&facet_list);
        chooser_stack.add_named(&facet_box, Some(FACET_PAGE));

        let value_list = gtk4::ListBox::new();
        value_list.add_css_class("boxed-list");
        let value_box = page_box();
        let chooser_back = gtk4::Button::from_icon_name("go-previous-symbolic");
        chooser_back.add_css_class("flat");
        chooser_back.set_tooltip_text(Some(&crate::ui::filter_bar_strings::text(
            crate::ui::filter_bar_strings::BACK,
        )));
        value_box.append(&chooser_back);
        value_box.append(&value_list);
        chooser_stack.add_named(&value_box, Some(VALUE_PAGE));

        let popover = gtk4::Popover::new();
        popover.set_child(Some(&chooser_stack));
        let add_filter = gtk4::MenuButton::new();
        add_filter.set_label(&strings::text(strings::CONCERTS_ADD_FILTER));
        add_filter.add_css_class("pill");
        filter_bar_layout::style_add_filter(&add_filter);
        add_filter.set_popover(Some(&popover));
        layout.fill_add_filter(&add_filter);

        let result_label = filter_bar_layout::count_label();
        layout.fill_count(&result_label);
        let clear_all =
            filter_bar_layout::clear_all_button(&strings::text(strings::CONCERTS_CLEAR_ALL));
        layout.fill_clear_all(&clear_all);

        let bar = Rc::new(Self {
            root,
            layout,
            conn,
            filter: RefCell::new(filter),
            chips,
            add_filter,
            popover,
            chooser_stack,
            facet_list,
            value_list,
            chooser_back,
            chooser_facets: RefCell::new(Vec::new()),
            chooser_values: RefCell::new(Vec::new()),
            result_label,
            clear_all,
            has_location: Cell::new(false),
            similar_enabled: Cell::new(false),
            has_similar_rows: Cell::new(false),
            counts: Cell::new((0, 0)),
            query: RefCell::new(String::new()),
            committed_query: RefCell::new(String::new()),
            on_changed: RefCell::new(None),
            on_query_changed: RefCell::new(None),
        });
        wire(&bar);
        bar.rebuild();
        bar
    }

    pub(super) fn widget(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    pub(super) fn filter(&self) -> ConcertFilter {
        self.filter.borrow().clone()
    }

    pub(super) fn set_on_changed(&self, callback: impl Fn(ConcertFilter) + 'static) {
        *self.on_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn set_on_query_changed(&self, callback: impl Fn(&str) + 'static) {
        *self.on_query_changed.borrow_mut() = Some(Rc::new(callback));
    }

    /// FIL-1d: matched against artist and venue.
    pub(super) fn query(&self) -> String {
        self.query.borrow().clone()
    }

    /// SEARCH-8a: this view's query, handed in by the shell.
    pub(super) fn set_query(self: &Rc<Self>, query: &str) {
        if *self.query.borrow() == query.trim() {
            return;
        }
        self.query.replace(query.trim().to_owned());
        self.rebuild();
        let callback = self.on_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(self.filter());
        }
    }

    pub(super) fn set_committed_query(self: &Rc<Self>, query: &str) {
        if *self.committed_query.borrow() == query {
            return;
        }
        self.committed_query.replace(query.to_owned());
        self.rebuild();
    }

    fn committed_query(&self) -> String {
        self.committed_query.borrow().clone()
    }

    fn clear_query(self: &Rc<Self>) {
        if self.query.borrow().is_empty() {
            return;
        }
        self.query.replace(String::new());
        // Clear all is a composite local action and rebuilds immediately, so
        // it drops the receipt in the same turn as the query. The search chip
        // itself does not use this helper: it requests the coordinator's
        // shared clear path through `request_query_clear` below.
        self.committed_query.replace(String::new());
        if let Some(callback) = self.on_query_changed.borrow().clone() {
            callback("");
        }
    }

    fn request_query_clear(&self) {
        let callback = self.on_query_changed.borrow().clone();
        if let Some(callback) = callback {
            callback("");
        }
    }

    pub(super) fn set_context(
        self: &Rc<Self>,
        has_location: bool,
        similar_enabled: bool,
        has_similar_rows: bool,
    ) {
        self.has_location.set(has_location);
        self.similar_enabled.set(similar_enabled);
        self.has_similar_rows.set(has_similar_rows);
        self.rebuild();
    }

    pub(super) fn set_counts(self: &Rc<Self>, shown: usize, total: usize) {
        self.counts.set((shown, total));
        self.rebuild();
    }

    pub(super) fn reload_persisted(self: &Rc<Self>) -> Result<(), rusqlite::Error> {
        let filter = config::persisted_filter(&self.conn)?;
        self.filter.replace(filter);
        self.rebuild();
        Ok(())
    }

    /// FIL-2a: "Clear all" for this section — its query and its facets.
    pub(super) fn clear_all(self: &Rc<Self>) {
        self.clear_query();
        self.apply_filter(ConcertFilter::default());
    }

    fn apply_filter(self: &Rc<Self>, filter: ConcertFilter) {
        if let Err(error) = persist_filter(&self.conn, &filter) {
            tracing::warn!(%error, "could not persist concerts filter");
            return;
        }
        self.filter.replace(filter.clone());
        self.popover.popdown();
        self.rebuild();
        let callback = self.on_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(filter);
        }
    }

    fn rebuild(self: &Rc<Self>) {
        while let Some(child) = self.chips.first_child() {
            self.chips.remove(&child);
        }
        let filter = self.filter();
        let query = self.query();
        let committed_query = self.committed_query();
        let facets = active_facets(&filter);
        let active = !facets.is_empty() || !query.is_empty();
        // FIL-1a/FIL-1d: the search chip is the row's first chip.
        let weak = Rc::downgrade(self);
        self.layout
            .replace_scoped_search(SearchScope::Concerts, &committed_query, move || {
                let Some(bar) = weak.upgrade() else {
                    return;
                };
                bar.request_query_clear();
            });
        for facet in facets {
            let button = gtk4::Button::with_label(&format!("{}  ×", chip_label(&filter, facet)));
            button.add_css_class("flat");
            button.add_css_class(filter_bar_layout::CHIP_CSS_CLASS);
            button.set_size_request(-1, 20);
            if facet == FilterFacet::Radius && !self.has_location.get() {
                button.set_sensitive(false);
                button
                    .set_tooltip_text(Some(&strings::text(strings::CONCERTS_SET_LOCATION_TOOLTIP)));
            }
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(bar) = weak.upgrade() {
                    bar.apply_filter(remove_filter(&bar.filter(), facet));
                }
            });
            self.chips.append(&button);
        }
        self.chips.set_visible(self.chips.first_child().is_some());
        self.clear_all.set_visible(active);
        let (shown, total) = self.counts.get();
        // FIL-2a: the shown number is accented while a restriction is active.
        let text;
        let presentation = if active {
            text = strings::concert_count_line_markup(shown, total);
            filter_bar_layout::CountPresentation::RestrictedMarkup(&text)
        } else {
            text = strings::concert_total_line(total);
            filter_bar_layout::CountPresentation::Plain(&text)
        };
        filter_bar_layout::present_count(&self.result_label, presentation);
        self.rebuild_facets();
    }

    fn rebuild_facets(&self) {
        self.facet_list.remove_all();
        let mut facets = vec![
            FilterFacet::Radius,
            FilterFacet::Country,
            FilterFacet::Horizon,
        ];
        if source_facet_visible(self.similar_enabled.get(), self.has_similar_rows.get()) {
            facets.push(FilterFacet::Source);
        }
        for facet in &facets {
            let row = chooser_row(&facet_label(*facet));
            if *facet == FilterFacet::Radius && !self.has_location.get() {
                row.set_sensitive(false);
                row.set_tooltip_text(Some(&strings::text(strings::CONCERTS_SET_LOCATION_TOOLTIP)));
            }
            self.facet_list.append(&row);
        }
        self.chooser_facets.replace(facets);
        self.chooser_stack.set_visible_child_name(FACET_PAGE);
    }

    fn show_values(&self, facet: FilterFacet) {
        self.value_list.remove_all();
        let values = self.values(facet);
        for value in &values {
            self.value_list.append(&chooser_row(&value.label));
        }
        self.chooser_values.replace(values);
        self.chooser_stack.set_visible_child_name(VALUE_PAGE);
    }

    fn values(&self, facet: FilterFacet) -> Vec<FilterValue> {
        match facet {
            FilterFacet::Radius => std::iter::once(None)
                .chain(
                    config::RADIUS_PRESETS_KM
                        .into_iter()
                        .map(|radius| Some(f64::from(radius))),
                )
                .map(|radius| FilterValue {
                    label: radius.map_or_else(
                        || strings::text(strings::CONCERTS_OFF),
                        |radius| strings::concerts_radius_km(radius as u32),
                    ),
                    apply: Rc::new(move |filter| ConcertFilter {
                        radius_km: radius,
                        ..filter.clone()
                    }),
                })
                .collect(),
            FilterFacet::Country => countries(&self.conn)
                .into_iter()
                .map(|country| {
                    let label = country.clone();
                    FilterValue {
                        label,
                        apply: Rc::new(move |filter| ConcertFilter {
                            country: Some(country.clone()),
                            ..filter.clone()
                        }),
                    }
                })
                .collect(),
            FilterFacet::Horizon => [
                DateHorizon::AllUpcoming,
                DateHorizon::Next30Days,
                DateHorizon::Next3Months,
                DateHorizon::Next6Months,
            ]
            .into_iter()
            .map(|horizon| FilterValue {
                label: horizon_label(horizon),
                apply: Rc::new(move |filter| ConcertFilter {
                    horizon,
                    ..filter.clone()
                }),
            })
            .collect(),
            FilterFacet::Source => [false, true]
                .into_iter()
                .map(|include_similar| FilterValue {
                    label: strings::text(if include_similar {
                        strings::CONCERTS_INCLUDE_SIMILAR
                    } else {
                        strings::CONCERTS_LIBRARY_ARTISTS_ONLY
                    }),
                    apply: Rc::new(move |filter| ConcertFilter {
                        include_similar,
                        ..filter.clone()
                    }),
                })
                .collect(),
        }
    }
}

fn page_box() -> gtk4::Box {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    page.set_margin_top(8);
    page.set_margin_bottom(8);
    page.set_margin_start(8);
    page.set_margin_end(8);
    page
}

fn chooser_row(label: &str) -> gtk4::ListBoxRow {
    let label = gtk4::Label::builder()
        .label(label)
        .xalign(0.0)
        .margin_top(7)
        .margin_bottom(7)
        .margin_start(10)
        .margin_end(10)
        .build();
    gtk4::ListBoxRow::builder().child(&label).build()
}

fn countries(db: &Db) -> Vec<String> {
    reprise_core::concerts::known_countries(db).unwrap_or_default()
}

fn wire(bar: &Rc<ConcertsFilterBar>) {
    {
        let weak = Rc::downgrade(bar);
        bar.add_filter.connect_active_notify(move |button| {
            if button.is_active() {
                if let Some(bar) = weak.upgrade() {
                    bar.rebuild_facets();
                }
            }
        });
    }
    {
        let weak = Rc::downgrade(bar);
        bar.facet_list.connect_row_activated(move |_, row| {
            let Some(bar) = weak.upgrade() else {
                return;
            };
            let facet = bar
                .chooser_facets
                .borrow()
                .get(row.index() as usize)
                .copied();
            if let Some(facet) = facet {
                bar.show_values(facet);
            }
        });
    }
    {
        let weak = Rc::downgrade(bar);
        bar.value_list.connect_row_activated(move |_, row| {
            let Some(bar) = weak.upgrade() else {
                return;
            };
            let value = bar
                .chooser_values
                .borrow()
                .get(row.index() as usize)
                .cloned();
            if let Some(value) = value {
                bar.apply_filter((value.apply)(&bar.filter()));
            }
        });
    }
    {
        let weak = Rc::downgrade(bar);
        bar.chooser_back.connect_clicked(move |_| {
            if let Some(bar) = weak.upgrade() {
                bar.rebuild_facets();
            }
        });
    }
    {
        let weak = Rc::downgrade(bar);
        bar.clear_all.connect_clicked(move |_| {
            if let Some(bar) = weak.upgrade() {
                bar.clear_all();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filtered() -> ConcertFilter {
        ConcertFilter {
            radius_km: Some(100.0),
            country: Some("DE".into()),
            horizon: DateHorizon::Next3Months,
            include_similar: true,
        }
    }

    #[test]
    fn conc_2_each_chip_removes_exactly_one_constraint_and_clear_is_default() {
        let filter = filtered();
        assert_eq!(remove_filter(&filter, FilterFacet::Radius).radius_km, None);
        assert_eq!(remove_filter(&filter, FilterFacet::Country).country, None);
        assert_eq!(
            remove_filter(&filter, FilterFacet::Horizon).horizon,
            DateHorizon::AllUpcoming
        );
        assert!(!remove_filter(&filter, FilterFacet::Source).include_similar);
    }

    #[test]
    fn conc_6_source_pill_exists_for_enabled_or_cached_similar_artists() {
        assert!(!source_facet_visible(false, false));
        assert!(source_facet_visible(true, false));
        assert!(source_facet_visible(false, true));
    }

    #[test]
    fn persisted_filter_round_trips_every_sticky_facet() {
        let conn = crate::test_db::open().unwrap();
        persist_filter(&conn, &filtered()).unwrap();
        assert_eq!(config::persisted_filter(&conn).unwrap(), filtered());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_4a_concerts_escape_and_chip_share_the_section_clear_path() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let bar = ConcertsFilterBar::new(Rc::new(crate::test_db::open().unwrap()));
        let entry = gtk4::SearchEntry::new();
        let lens = gtk4::ToggleButton::new();
        let popover = crate::ui::window::search_popover::SearchPopover::new(&lens, &entry);
        let search = crate::ui::window::section_search::SectionSearch::new(&entry, &popover, &lens);
        search.register(
            SearchScope::Concerts,
            {
                let bar = Rc::downgrade(&bar);
                move |query| {
                    if let Some(bar) = bar.upgrade() {
                        bar.set_query(query);
                    }
                }
            },
            {
                let bar = Rc::downgrade(&bar);
                move |query| {
                    if let Some(bar) = bar.upgrade() {
                        bar.set_committed_query(query);
                    }
                }
            },
            || {},
        );
        search.activate(SearchScope::Concerts, "Concerts");
        bar.set_on_query_changed({
            let bar = Rc::downgrade(&bar);
            let search = Rc::downgrade(&search);
            move |query| {
                let bar = bar.upgrade().expect("Concerts bar still exists");
                assert_eq!(
                    bar.query(),
                    "winterthur",
                    "the chip must delegate before mutating"
                );
                if let Some(search) = search.upgrade() {
                    search.set_query(SearchScope::Concerts, query);
                }
            }
        });

        entry.set_text("winterthur");
        bar.set_query("winterthur");
        bar.set_committed_query("winterthur");
        assert_eq!(
            popover.press_close_key(gtk4::gdk::Key::Escape),
            gtk4::glib::Propagation::Stop
        );
        bar.layout.assert_search_cleared(&bar.query());

        entry.set_text("winterthur");
        bar.set_query("winterthur");
        bar.set_committed_query("winterthur");
        bar.layout
            .slot_child(crate::ui::filter_bar_layout::FilterBarSlot::Search)
            .and_downcast::<gtk4::Button>()
            .expect("Concerts search chip")
            .emit_clicked();

        bar.layout.assert_search_cleared(&bar.query());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn conc_2_filter_header_has_fixed_height_and_disabled_radius_hint() {
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let bar = ConcertsFilterBar::new(conn);
        assert_eq!(
            bar.root.height_request(),
            filter_bar_layout::FILTER_BAR_MIN_HEIGHT
        );
        let radius = bar.facet_list.row_at_index(0).unwrap();
        assert!(!radius.is_sensitive());
        assert_eq!(
            radius.tooltip_text().as_deref(),
            Some("Set a location in Preferences")
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_2a_concerts_fill_filters_count_and_clear_slots_in_order() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let bar = ConcertsFilterBar::new(conn);
        bar.set_query("winterthur");
        bar.set_committed_query("winterthur");
        bar.set_counts(3, 44);

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
            &bar.result_label
        ));
        assert!(bar.layout.slot_contains(
            crate::ui::filter_bar_layout::FilterBarSlot::ClearAll,
            &bar.clear_all
        ));
        let first = bar
            .layout
            .slot_child(crate::ui::filter_bar_layout::FilterBarSlot::Search)
            .expect("query fills the search slot");
        assert!(first
            .downcast::<gtk4::Button>()
            .ok()
            .and_then(|button| button.label())
            .is_some_and(|label| label.starts_with('⌕')));
        assert!(!descendant_labels(bar.widget())
            .iter()
            .any(|text| text == "FILTER"));
    }

    fn descendant_labels(widget: &impl IsA<gtk4::Widget>) -> Vec<String> {
        let mut labels = Vec::new();
        let mut child = widget.as_ref().first_child();
        while let Some(current) = child {
            if let Ok(label) = current.clone().downcast::<gtk4::Label>() {
                labels.push(label.text().to_string());
            }
            labels.extend(descendant_labels(&current));
            child = current.next_sibling();
        }
        labels
    }
}
