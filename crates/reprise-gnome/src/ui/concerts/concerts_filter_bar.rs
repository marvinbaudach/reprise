#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::concerts::config;
use reprise_core::concerts::{ConcertFilter, DateHorizon};
use rusqlite::Connection;

use crate::ui::browse::browse_bar::CHIP_CSS_CLASS;
use crate::ui::strings;

const FILTER_BAR_MIN_HEIGHT: i32 = 34;
const FACET_PAGE: &str = "facets";
const VALUE_PAGE: &str = "values";

type OnChanged = Rc<dyn Fn(ConcertFilter)>;

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

fn persist_filter(conn: &Connection, filter: &ConcertFilter) -> Result<(), rusqlite::Error> {
    let radius = filter
        .radius_km
        .map(|radius| radius.round().to_string())
        .unwrap_or_default();
    reprise_core::library::settings::set_setting(conn, config::FILTER_RADIUS_KEY, &radius)?;
    reprise_core::library::settings::set_setting(
        conn,
        config::FILTER_COUNTRY_KEY,
        filter.country.as_deref().unwrap_or_default(),
    )?;
    let horizon = match filter.horizon {
        DateHorizon::AllUpcoming => "",
        DateHorizon::Next30Days => "next_30_days",
        DateHorizon::Next3Months => "next_3_months",
        DateHorizon::Next6Months => "next_6_months",
    };
    reprise_core::library::settings::set_setting(conn, config::FILTER_HORIZON_KEY, horizon)?;
    reprise_core::library::settings::set_bool(
        conn,
        config::FILTER_INCLUDE_SIMILAR_KEY,
        filter.include_similar,
    )
}

pub(super) struct ConcertsFilterBar {
    root: gtk4::Box,
    conn: Rc<RefCell<Connection>>,
    filter: RefCell<ConcertFilter>,
    section_label: gtk4::Label,
    chips: gtk4::FlowBox,
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
    on_changed: RefCell<Option<OnChanged>>,
}

impl ConcertsFilterBar {
    pub(super) fn new(conn: Rc<RefCell<Connection>>) -> Rc<Self> {
        let filter = config::persisted_filter(&conn.borrow()).unwrap_or_default();
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        root.set_margin_top(6);
        root.set_margin_bottom(6);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.set_size_request(-1, FILTER_BAR_MIN_HEIGHT);
        root.add_css_class("toolbar");

        let section_label = gtk4::Label::new(Some(&strings::text(strings::CONCERTS_FILTER)));
        section_label.add_css_class("dim-label");
        section_label.add_css_class("caption-heading");
        root.append(&section_label);

        let chips = gtk4::FlowBox::builder()
            .selection_mode(gtk4::SelectionMode::None)
            .column_spacing(6)
            .row_spacing(4)
            .hexpand(true)
            .max_children_per_line(20)
            .build();
        root.append(&chips);

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
        chooser_back.set_tooltip_text(Some(&crate::ui::browse_filter_strings::text(
            crate::ui::browse_filter_strings::BACK,
        )));
        value_box.append(&chooser_back);
        value_box.append(&value_list);
        chooser_stack.add_named(&value_box, Some(VALUE_PAGE));

        let popover = gtk4::Popover::new();
        popover.set_child(Some(&chooser_stack));
        let add_filter = gtk4::MenuButton::new();
        add_filter.set_label(&strings::text(strings::CONCERTS_ADD_FILTER));
        add_filter.add_css_class("pill");
        add_filter.set_popover(Some(&popover));

        let result_label = gtk4::Label::new(None);
        result_label.add_css_class("dim-label");
        result_label.add_css_class("caption");
        root.append(&result_label);
        let clear_all = gtk4::Button::with_label(&strings::text(strings::CONCERTS_CLEAR_ALL));
        clear_all.add_css_class("flat");
        clear_all.add_css_class(CHIP_CSS_CLASS);
        root.append(&clear_all);

        let bar = Rc::new(Self {
            root,
            conn,
            filter: RefCell::new(filter),
            section_label,
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
            on_changed: RefCell::new(None),
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
        let filter = config::persisted_filter(&self.conn.borrow())?;
        self.filter.replace(filter);
        self.rebuild();
        Ok(())
    }

    pub(super) fn clear_all(self: &Rc<Self>) {
        self.apply_filter(ConcertFilter::default());
    }

    fn apply_filter(self: &Rc<Self>, filter: ConcertFilter) {
        if let Err(error) = persist_filter(&self.conn.borrow(), &filter) {
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
        if let Some(wrapper) = self
            .add_filter
            .parent()
            .and_downcast::<gtk4::FlowBoxChild>()
        {
            wrapper.set_child(gtk4::Widget::NONE);
        }
        self.chips.remove_all();
        let filter = self.filter();
        let facets = active_facets(&filter);
        let active = !facets.is_empty();
        for facet in facets {
            let button = gtk4::Button::with_label(&format!("{}  ×", chip_label(&filter, facet)));
            button.add_css_class("flat");
            button.add_css_class(CHIP_CSS_CLASS);
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
        self.chips.append(&self.add_filter);
        self.section_label.set_visible(active);
        self.clear_all.set_visible(active);
        let (shown, total) = self.counts.get();
        self.result_label.set_text(&if active {
            strings::concert_count_line(shown, total)
        } else {
            strings::concert_total_line(total)
        });
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
            FilterFacet::Country => countries(&self.conn.borrow())
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

fn countries(conn: &Connection) -> Vec<String> {
    let Ok(mut statement) = conn.prepare(
        "SELECT DISTINCT trim(country) FROM concert_events
         WHERE country IS NOT NULL AND trim(country) <> ''
         ORDER BY lower(trim(country))",
    ) else {
        return Vec::new();
    };
    statement
        .query_map([], |row| row.get(0))
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
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
        let conn = Connection::open_in_memory().unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        persist_filter(&conn, &filtered()).unwrap();
        assert_eq!(config::persisted_filter(&conn).unwrap(), filtered());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn conc_2_filter_header_has_fixed_height_and_disabled_radius_hint() {
        gtk4::init().unwrap();
        let conn = Rc::new(RefCell::new(Connection::open_in_memory().unwrap()));
        reprise_core::db::migrate(&conn.borrow()).unwrap();
        let bar = ConcertsFilterBar::new(conn);
        assert_eq!(bar.root.height_request(), FILTER_BAR_MIN_HEIGHT);
        let radius = bar.facet_list.row_at_index(0).unwrap();
        assert!(!radius.is_sensitive());
        assert_eq!(
            radius.tooltip_text().as_deref(),
            Some("Set a location in Preferences")
        );
    }
}
