//! Shared filter-bar geometry and slot ordering.

use gtk4::prelude::*;

pub(in crate::ui) const FILTER_BAR_MIN_HEIGHT: i32 = 34;
pub(in crate::ui) const ADD_FILTER_CSS_CLASS: &str = "reprise-filter-add";
pub(in crate::ui) const CLEAR_ALL_CSS_CLASS: &str = "reprise-filter-clear";

const PLACE_SLOT_NAME: &str = "reprise-filter-slot-place";
const SEARCH_SLOT_NAME: &str = "reprise-filter-slot-search";
const FACETS_SLOT_NAME: &str = "reprise-filter-slot-facets";
const ADD_FILTER_SLOT_NAME: &str = "reprise-filter-slot-add-filter";
const SPACER_SLOT_NAME: &str = "reprise-filter-slot-spacer";
const COUNT_SLOT_NAME: &str = "reprise-filter-slot-count";
const CLEAR_ALL_SLOT_NAME: &str = "reprise-filter-slot-clear-all";
const TRAILING_ACTION_SLOT_NAME: &str = "reprise-filter-slot-trailing-action";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum FilterBarSlot {
    Place,
    Search,
    Facets,
    AddFilter,
    Spacer,
    Count,
    ClearAll,
    TrailingAction,
}

impl FilterBarSlot {
    fn name(self) -> &'static str {
        match self {
            Self::Place => PLACE_SLOT_NAME,
            Self::Search => SEARCH_SLOT_NAME,
            Self::Facets => FACETS_SLOT_NAME,
            Self::AddFilter => ADD_FILTER_SLOT_NAME,
            Self::Spacer => SPACER_SLOT_NAME,
            Self::Count => COUNT_SLOT_NAME,
            Self::ClearAll => CLEAR_ALL_SLOT_NAME,
            Self::TrailingAction => TRAILING_ACTION_SLOT_NAME,
        }
    }
}

#[derive(Clone)]
pub(in crate::ui) struct FilterBarLayout {
    root: gtk4::Box,
    place: gtk4::Box,
    search: gtk4::Box,
    facets: gtk4::Box,
    add_filter: gtk4::Box,
    count: gtk4::Box,
    clear_all: gtk4::Box,
    trailing_action: gtk4::Box,
}

impl FilterBarLayout {
    pub(in crate::ui) fn new() -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        root.set_margin_top(6);
        root.set_margin_bottom(6);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.set_size_request(-1, FILTER_BAR_MIN_HEIGHT);
        root.add_css_class("toolbar");

        let place = slot(FilterBarSlot::Place);
        let search = slot(FilterBarSlot::Search);
        let facets = slot(FilterBarSlot::Facets);
        let add_filter = slot(FilterBarSlot::AddFilter);
        let spacer = slot(FilterBarSlot::Spacer);
        let count = slot(FilterBarSlot::Count);
        let clear_all = slot(FilterBarSlot::ClearAll);
        let trailing_action = slot(FilterBarSlot::TrailingAction);

        // FIL-2a: this is the only place where filter-bar slot order lives.
        root.append(&place);
        root.append(&search);
        root.append(&facets);
        root.append(&add_filter);
        root.append(&spacer);
        root.append(&count);
        root.append(&clear_all);
        root.append(&trailing_action);

        Self {
            root,
            place,
            search,
            facets,
            add_filter,
            count,
            clear_all,
            trailing_action,
        }
    }

    pub(in crate::ui) fn root(&self) -> &gtk4::Box {
        &self.root
    }

    pub(in crate::ui) fn fill_place(&self, widget: &impl IsA<gtk4::Widget>) {
        fill(&self.place, widget);
    }

    pub(in crate::ui) fn fill_search(&self, widget: &impl IsA<gtk4::Widget>) {
        fill(&self.search, widget);
    }

    pub(in crate::ui) fn clear_search(&self) {
        clear(&self.search);
    }

    pub(in crate::ui) fn fill_facets(&self, widget: &impl IsA<gtk4::Widget>) {
        fill(&self.facets, widget);
    }

    pub(in crate::ui) fn fill_add_filter(&self, widget: &impl IsA<gtk4::Widget>) {
        fill(&self.add_filter, widget);
    }

    pub(in crate::ui) fn fill_count(&self, widget: &impl IsA<gtk4::Widget>) {
        fill(&self.count, widget);
    }

    pub(in crate::ui) fn fill_clear_all(&self, widget: &impl IsA<gtk4::Widget>) {
        fill(&self.clear_all, widget);
    }

    pub(in crate::ui) fn fill_trailing_action(&self, widget: &impl IsA<gtk4::Widget>) {
        fill(&self.trailing_action, widget);
    }

    #[cfg(test)]
    pub(in crate::ui) fn slot_order(&self) -> Vec<FilterBarSlot> {
        let mut order = Vec::new();
        let mut child = self.root.first_child();
        while let Some(widget) = child {
            let name = widget.widget_name();
            let slot = [
                FilterBarSlot::Place,
                FilterBarSlot::Search,
                FilterBarSlot::Facets,
                FilterBarSlot::AddFilter,
                FilterBarSlot::Spacer,
                FilterBarSlot::Count,
                FilterBarSlot::ClearAll,
                FilterBarSlot::TrailingAction,
            ]
            .into_iter()
            .find(|slot| name == slot.name())
            .expect("every direct child is a named filter-bar slot");
            order.push(slot);
            child = widget.next_sibling();
        }
        order
    }

    #[cfg(test)]
    pub(in crate::ui) fn slot_contains(
        &self,
        slot: FilterBarSlot,
        widget: &impl IsA<gtk4::Widget>,
    ) -> bool {
        widget
            .as_ref()
            .parent()
            .is_some_and(|parent| parent.widget_name() == slot.name())
    }

    #[cfg(test)]
    pub(in crate::ui) fn slot_child(&self, slot: FilterBarSlot) -> Option<gtk4::Widget> {
        let mut child = self.root.first_child();
        while let Some(widget) = child {
            if widget.widget_name() == slot.name() {
                return widget.first_child();
            }
            child = widget.next_sibling();
        }
        None
    }

    #[cfg(test)]
    fn slot_widget(&self, slot: FilterBarSlot) -> gtk4::Widget {
        let mut child = self.root.first_child();
        while let Some(widget) = child {
            if widget.widget_name() == slot.name() {
                return widget;
            }
            child = widget.next_sibling();
        }
        panic!("missing filter-bar slot {slot:?}");
    }
}

pub(in crate::ui) fn style_add_filter(button: &impl IsA<gtk4::Widget>) {
    button.add_css_class(ADD_FILTER_CSS_CLASS);
}

pub(in crate::ui) fn style_clear_all(button: &impl IsA<gtk4::Widget>) {
    button.add_css_class(CLEAR_ALL_CSS_CLASS);
}

pub(in crate::ui) fn css() -> String {
    format!(
        ".{ADD_FILTER_CSS_CLASS} {{ border: 1px dashed alpha(currentColor, 0.18); \
         border-radius: 9999px; background-color: transparent; }} \
         .{ADD_FILTER_CSS_CLASS}:hover {{ background-color: alpha(currentColor, 0.08); }} \
         .{CLEAR_ALL_CSS_CLASS} {{ border: 1px solid alpha(currentColor, 0.30); \
         border-radius: 9999px; background-color: transparent; }} \
         .{CLEAR_ALL_CSS_CLASS}:hover {{ background-color: alpha(currentColor, 0.08); }}"
    )
}

fn slot(kind: FilterBarSlot) -> gtk4::Box {
    let slot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    slot.set_widget_name(kind.name());
    slot.set_hexpand(kind == FilterBarSlot::Spacer);
    slot
}

fn fill(slot: &gtk4::Box, widget: &impl IsA<gtk4::Widget>) {
    clear(slot);
    widget.set_hexpand(false);
    slot.append(widget);
}

fn clear(slot: &gtk4::Box) {
    while let Some(child) = slot.first_child() {
        slot.remove(&child);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const NARROW_WIDTH: i32 = 800;
    const WIDE_WIDTH: i32 = 1_120;

    #[derive(Clone, Copy)]
    enum FacetContainer {
        FlowBox,
        Box,
    }

    #[derive(Debug)]
    struct Geometry {
        root_width: f32,
        add_gap: f32,
        slot_gap: f32,
        slot_widths: [f32; 8],
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_2a_the_skeleton_owns_the_normative_slot_order() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let layout = FilterBarLayout::new();

        assert_eq!(
            layout.slot_order(),
            [
                FilterBarSlot::Place,
                FilterBarSlot::Search,
                FilterBarSlot::Facets,
                FilterBarSlot::AddFilter,
                FilterBarSlot::Spacer,
                FilterBarSlot::Count,
                FilterBarSlot::ClearAll,
                FilterBarSlot::TrailingAction,
            ]
        );
        assert_eq!(layout.root().height_request(), FILTER_BAR_MIN_HEIGHT);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_2a_only_the_spacer_expands_across_all_six_filter_bars() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();

        for (bar, container) in [
            ("Music", FacetContainer::FlowBox),
            ("Releases", FacetContainer::FlowBox),
            ("Concerts", FacetContainer::FlowBox),
            ("Podcasts", FacetContainer::Box),
            ("YouTube", FacetContainer::Box),
            ("Radio", FacetContainer::Box),
        ] {
            for facets_full in [false, true] {
                let narrow = measure_geometry(container, facets_full, NARROW_WIDTH);
                let wide = measure_geometry(container, facets_full, WIDE_WIDTH);
                let state = if facets_full { "full" } else { "empty" };

                assert_close(
                    wide.add_gap,
                    narrow.add_gap,
                    &format!(
                        "{bar} moved Add filter away from the preceding {state} facets; narrow={narrow:?}; wide={wide:?}"
                    ),
                );
                assert_close(
                    wide.slot_gap,
                    narrow.slot_gap,
                    &format!("{bar} opened a slot gap before Add filter with {state} facets"),
                );

                for (index, slot) in all_slots().into_iter().enumerate() {
                    if slot == FilterBarSlot::Spacer {
                        continue;
                    }
                    assert_close(
                        wide.slot_widths[index],
                        narrow.slot_widths[index],
                        &format!("{bar}'s {slot:?} slot expanded with {state} facets"),
                    );
                }

                let spacer_index = all_slots()
                    .iter()
                    .position(|slot| *slot == FilterBarSlot::Spacer)
                    .expect("the filter bar layout always contains a spacer slot");
                assert_close(
                    wide.slot_widths[spacer_index] - narrow.slot_widths[spacer_index],
                    wide.root_width - narrow.root_width,
                    &format!("{bar}'s spacer did not absorb all growth with {state} facets"),
                );
            }
        }
    }

    fn measure_geometry(container: FacetContainer, facets_full: bool, width: i32) -> Geometry {
        let layout = FilterBarLayout::new();
        let search = gtk4::Button::with_label("⌕ falling");
        layout.fill_search(&search);

        let (facets, last_facet) = facet_container(container, facets_full);
        // The slot contract must withstand any child requesting expansion.
        facets.set_hexpand(true);
        layout.fill_facets(&facets);

        let add_filter = gtk4::Button::with_label("+ Add filter");
        layout.fill_add_filter(&add_filter);
        layout.fill_count(&gtk4::Label::new(Some("44 tracks")));
        layout.fill_clear_all(&gtk4::Button::with_label("Clear all"));

        let window = gtk4::Window::builder()
            .default_width(width)
            .child(layout.root())
            .build();
        window.set_size_request(width, -1);
        window.present();
        assert!(crate::ui::test_settle::settle_until_mapped(layout.root()));
        crate::ui::test_settle::settle_for(Duration::from_millis(20));

        let preceding = last_facet.unwrap_or_else(|| search.clone().upcast());
        let preceding_bounds = preceding
            .compute_bounds(layout.root())
            .expect("preceding chip has filter-bar bounds");
        let add_bounds = add_filter
            .compute_bounds(layout.root())
            .expect("Add filter has filter-bar bounds");
        let facet_slot_bounds = layout
            .slot_widget(FilterBarSlot::Facets)
            .compute_bounds(layout.root())
            .expect("facet slot has filter-bar bounds");
        let add_slot_bounds = layout
            .slot_widget(FilterBarSlot::AddFilter)
            .compute_bounds(layout.root())
            .expect("Add filter slot has filter-bar bounds");
        let slot_widths = all_slots().map(|slot| {
            layout
                .slot_widget(slot)
                .compute_bounds(layout.root())
                .expect("slot has filter-bar bounds")
                .width()
        });
        let geometry = Geometry {
            root_width: layout.root().width() as f32,
            add_gap: add_bounds.x() - preceding_bounds.x() - preceding_bounds.width(),
            slot_gap: add_slot_bounds.x() - facet_slot_bounds.x() - facet_slot_bounds.width(),
            slot_widths,
        };
        window.close();
        geometry
    }

    fn facet_container(
        container: FacetContainer,
        full: bool,
    ) -> (gtk4::Widget, Option<gtk4::Widget>) {
        let facet = full.then(|| gtk4::Button::with_label("Genre: Metal"));
        let widget = match container {
            FacetContainer::FlowBox => {
                let flow = gtk4::FlowBox::builder()
                    .selection_mode(gtk4::SelectionMode::None)
                    .column_spacing(6)
                    .build();
                if let Some(facet) = &facet {
                    flow.insert(facet, -1);
                }
                flow.upcast()
            }
            FacetContainer::Box => {
                let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
                if let Some(facet) = &facet {
                    row.append(facet);
                }
                row.upcast()
            }
        };
        (widget, facet.map(gtk4::glib::object::Cast::upcast))
    }

    fn all_slots() -> [FilterBarSlot; 8] {
        [
            FilterBarSlot::Place,
            FilterBarSlot::Search,
            FilterBarSlot::Facets,
            FilterBarSlot::AddFilter,
            FilterBarSlot::Spacer,
            FilterBarSlot::Count,
            FilterBarSlot::ClearAll,
            FilterBarSlot::TrailingAction,
        ]
    }

    fn assert_close(actual: f32, expected: f32, message: &str) {
        assert!(
            (actual - expected).abs() <= 1.0,
            "{message}: expected {expected:.1}, got {actual:.1}"
        );
    }
}
