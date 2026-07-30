#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::artist_news::{
    persisted_releases_filter, ReleaseTypeFilter, ReleasesFilter, RELEASES_FILTER_HIDDEN_KEY,
    RELEASES_FILTER_TYPE_KEY,
};
use reprise_core::db::Db;

use crate::ui::browse::browse_bar::CHIP_CSS_CLASS;
use crate::ui::strings;

const FILTER_BAR_MIN_HEIGHT: i32 = 34;
const FACET_PAGE: &str = "facets";
const VALUE_PAGE: &str = "values";

type OnChanged = Rc<dyn Fn(ReleasesFilter)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FilterFacet {
    Type,
    Hidden,
}

pub(super) fn remove_filter(filter: &ReleasesFilter, facet: FilterFacet) -> ReleasesFilter {
    match facet {
        FilterFacet::Type => ReleasesFilter {
            release_type: None,
            ..filter.clone()
        },
        FilterFacet::Hidden => ReleasesFilter {
            hidden: false,
            ..filter.clone()
        },
    }
}

fn active_facets(filter: &ReleasesFilter) -> Vec<FilterFacet> {
    let mut facets = Vec::new();
    if filter.release_type.is_some() {
        facets.push(FilterFacet::Type);
    }
    if filter.hidden {
        facets.push(FilterFacet::Hidden);
    }
    facets
}

fn facet_label(facet: FilterFacet) -> String {
    strings::text(match facet {
        FilterFacet::Type => strings::RELEASES_TYPE,
        FilterFacet::Hidden => strings::RELEASES_HIDDEN,
    })
}

fn chip_label(filter: &ReleasesFilter, facet: FilterFacet) -> String {
    match facet {
        FilterFacet::Type => strings::text(match filter.release_type {
            Some(ReleaseTypeFilter::Album) => strings::RELEASES_ALBUM,
            Some(ReleaseTypeFilter::Ep) => strings::RELEASES_EP,
            None => strings::RELEASES_TYPE,
        }),
        _ => facet_label(facet),
    }
}

fn persist_filter(db: &Db, filter: &ReleasesFilter) -> Result<(), rusqlite::Error> {
    let conn = &db;
    reprise_core::library::settings::set_setting(
        conn,
        RELEASES_FILTER_TYPE_KEY,
        filter
            .release_type
            .map_or("", ReleaseTypeFilter::setting_value),
    )?;
    reprise_core::library::settings::set_bool(conn, RELEASES_FILTER_HIDDEN_KEY, filter.hidden)
}

pub(super) struct ReleasesFilterBar {
    root: gtk4::Box,
    conn: Rc<Db>,
    filter: RefCell<ReleasesFilter>,
    section_label: gtk4::Label,
    chips: gtk4::FlowBox,
    add_filter: gtk4::MenuButton,
    popover: gtk4::Popover,
    chooser_stack: gtk4::Stack,
    facet_list: gtk4::ListBox,
    value_list: gtk4::ListBox,
    chooser_back: gtk4::Button,
    chooser_facets: RefCell<Vec<FilterFacet>>,
    chooser_values: RefCell<Vec<(String, ReleasesFilter)>>,
    result_label: gtk4::Label,
    clear_all: gtk4::Button,
    counts: Cell<(usize, usize)>,
    on_changed: RefCell<Option<OnChanged>>,
}

impl ReleasesFilterBar {
    pub(super) fn new(conn: Rc<Db>) -> Rc<Self> {
        let filter = persisted_releases_filter(&conn).unwrap_or_default();
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        root.set_margin_top(6);
        root.set_margin_bottom(6);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.set_size_request(-1, FILTER_BAR_MIN_HEIGHT);
        root.add_css_class("toolbar");

        let section_label = gtk4::Label::new(Some(&strings::text(strings::RELEASES_FILTER)));
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
        value_box.append(&chooser_back);
        value_box.append(&value_list);
        chooser_stack.add_named(&value_box, Some(VALUE_PAGE));
        let popover = gtk4::Popover::new();
        popover.set_child(Some(&chooser_stack));
        let add_filter = gtk4::MenuButton::new();
        add_filter.set_label(&strings::text(strings::RELEASES_ADD_FILTER));
        add_filter.add_css_class("pill");
        add_filter.set_popover(Some(&popover));

        let result_label = gtk4::Label::new(None);
        result_label.add_css_class("dim-label");
        result_label.add_css_class("caption");
        root.append(&result_label);
        let clear_all = gtk4::Button::with_label(&strings::text(strings::RELEASES_CLEAR_ALL));
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

    pub(super) fn filter(&self) -> ReleasesFilter {
        self.filter.borrow().clone()
    }

    pub(super) fn set_on_changed(&self, callback: impl Fn(ReleasesFilter) + 'static) {
        *self.on_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn set_counts(self: &Rc<Self>, shown: usize, total: usize) {
        self.counts.set((shown, total));
        self.rebuild();
    }

    pub(super) fn clear_all(self: &Rc<Self>) {
        self.apply_filter(ReleasesFilter::default());
    }

    fn apply_filter(self: &Rc<Self>, filter: ReleasesFilter) {
        if let Err(error) = persist_filter(&self.conn, &filter) {
            tracing::warn!(%error, "could not persist Releases filter");
            return;
        }
        self.filter.replace(filter.clone());
        self.popover.popdown();
        self.rebuild();
        if let Some(callback) = self.on_changed.borrow().clone() {
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
            strings::release_count_line(shown, total)
        } else {
            strings::release_total_line(total)
        });
        self.rebuild_facets();
    }

    fn rebuild_facets(&self) {
        self.facet_list.remove_all();
        let active = active_facets(&self.filter());
        let facets = [FilterFacet::Type, FilterFacet::Hidden]
            .into_iter()
            .filter(|facet| !active.contains(facet))
            .collect::<Vec<_>>();
        for facet in &facets {
            self.facet_list.append(&chooser_row(&facet_label(*facet)));
        }
        self.chooser_facets.replace(facets);
        self.chooser_stack.set_visible_child_name(FACET_PAGE);
    }

    fn show_values(&self, facet: FilterFacet) {
        self.value_list.remove_all();
        let current = self.filter();
        let values = match facet {
            FilterFacet::Hidden => vec![(
                strings::text(strings::RELEASES_HIDDEN),
                ReleasesFilter {
                    hidden: true,
                    ..current
                },
            )],
            FilterFacet::Type => [ReleaseTypeFilter::Album, ReleaseTypeFilter::Ep]
                .into_iter()
                .map(|release_type| {
                    let filter = ReleasesFilter {
                        release_type: Some(release_type),
                        ..current.clone()
                    };
                    (chip_label(&filter, FilterFacet::Type), filter)
                })
                .collect(),
        };
        for (label, _) in &values {
            self.value_list.append(&chooser_row(label));
        }
        self.chooser_values.replace(values);
        self.chooser_stack.set_visible_child_name(VALUE_PAGE);
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

fn wire(bar: &Rc<ReleasesFilterBar>) {
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
            let filter = bar
                .chooser_values
                .borrow()
                .get(row.index() as usize)
                .map(|(_, filter)| filter.clone());
            if let Some(filter) = filter {
                bar.apply_filter(filter);
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
    use reprise_core::artist_news::ReleaseTypeFilter;

    #[test]
    fn nr_17_each_release_chip_removes_only_its_own_constraint() {
        let filter = ReleasesFilter {
            release_type: Some(ReleaseTypeFilter::Ep),
            hidden: true,
        };
        assert_eq!(
            remove_filter(&filter, FilterFacet::Type),
            ReleasesFilter {
                release_type: None,
                ..filter.clone()
            }
        );
        assert_eq!(
            remove_filter(&filter, FilterFacet::Hidden),
            ReleasesFilter {
                hidden: false,
                ..filter
            }
        );
    }

    #[test]
    fn sticky_release_filter_round_trips_every_facet() {
        let conn = crate::test_db::open().unwrap();
        let filter = ReleasesFilter {
            release_type: Some(ReleaseTypeFilter::Album),
            hidden: true,
        };
        persist_filter(&conn, &filter).unwrap();
        assert_eq!(persisted_releases_filter(&conn).unwrap(), filter);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nr_17_filter_header_is_permanent_and_reserves_its_height() {
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let bar = ReleasesFilterBar::new(conn);
        assert_eq!(bar.root.height_request(), FILTER_BAR_MIN_HEIGHT);
        assert!(bar.add_filter.is_visible());
    }
}
