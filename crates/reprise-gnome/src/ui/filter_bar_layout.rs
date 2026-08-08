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
        spacer.set_hexpand(true);
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
    slot
}

fn fill(slot: &gtk4::Box, widget: &impl IsA<gtk4::Widget>) {
    clear(slot);
    slot.append(widget);
}

fn clear(slot: &gtk4::Box) {
    while let Some(child) = slot.first_child() {
        slot.remove(&child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
