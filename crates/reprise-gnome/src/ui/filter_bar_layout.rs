//! Shared filter-bar geometry and slot ordering.

use gtk4::prelude::*;
use reprise_view::search_scope::SearchScope;

pub(in crate::ui) const FILTER_BAR_MIN_HEIGHT: i32 = 34;
pub(in crate::ui) const CHIP_CSS_CLASS: &str = "reprise-filter-chip";
pub(in crate::ui) const ADD_FILTER_CSS_CLASS: &str = "reprise-filter-add";
pub(in crate::ui) const CLEAR_ALL_CSS_CLASS: &str = "reprise-filter-clear";

/// The minimum ×-click target FIL-1a requires of a removable search chip.
const CHIP_MIN_HIT_PX: i32 = 20;

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
    #[cfg(test)]
    // Keep every new variant here; Rust cannot prove enum-array completeness.
    const ALL: [Self; 8] = [
        Self::Place,
        Self::Search,
        Self::Facets,
        Self::AddFilter,
        Self::Spacer,
        Self::Count,
        Self::ClearAll,
        Self::TrailingAction,
    ];

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

    /// Replaces the search slot with the canonical chip for this view. The
    /// query is the committed query; blank means there is no chip.
    pub(in crate::ui) fn replace_scoped_search(
        &self,
        scope: SearchScope,
        query: &str,
        on_clear: impl Fn() + 'static,
    ) {
        let query = query.trim();
        if query.is_empty() {
            self.clear_search();
            return;
        }
        self.replace_search(
            &format!(
                "{}  ×",
                crate::ui::filter_bar_strings::scoped_search_chip_label(scope, query)
            ),
            &crate::ui::filter_bar_strings::remove_search_label(query),
            on_clear,
        );
    }

    /// Replaces the search slot with a canonical removable chip whose complete
    /// visible label has already been rendered for a surface outside
    /// [`SearchScope`].
    pub(in crate::ui) fn replace_search(
        &self,
        label: &str,
        accessible_remove_label: &str,
        on_clear: impl Fn() + 'static,
    ) {
        let button = gtk4::Button::with_label(label);
        button.add_css_class("flat");
        button.add_css_class(CHIP_CSS_CLASS);
        button.set_size_request(-1, CHIP_MIN_HIT_PX);
        button.update_property(&[gtk4::accessible::Property::Label(accessible_remove_label)]);
        button.connect_clicked(move |_| on_clear());
        self.fill_search(&button);
    }

    pub(in crate::ui) fn fill_facets(&self, widget: &impl IsA<gtk4::Widget>) {
        widget.set_halign(gtk4::Align::Start);
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

    /// The slots that actually hold something, in visual order. `slot_order`
    /// below reports the fixed construction order and so can never disagree
    /// with itself; this one changes when the bar's content changes, which is
    /// what an ordering assertion needs to be worth making.
    #[cfg(test)]
    pub(in crate::ui) fn populated_slot_order(&self) -> Vec<FilterBarSlot> {
        self.slot_order()
            .into_iter()
            .filter(|slot| {
                self.slot_box(*slot)
                    .is_some_and(|slot| slot.first_child().is_some())
            })
            .collect()
    }

    /// `None` for the spacer: it is structural, so "populated" means nothing
    /// there and counting it would put it in every ordering assertion.
    #[cfg(test)]
    fn slot_box(&self, slot: FilterBarSlot) -> Option<&gtk4::Box> {
        match slot {
            FilterBarSlot::Place => Some(&self.place),
            FilterBarSlot::Search => Some(&self.search),
            FilterBarSlot::Facets => Some(&self.facets),
            FilterBarSlot::AddFilter => Some(&self.add_filter),
            FilterBarSlot::Spacer => None,
            FilterBarSlot::Count => Some(&self.count),
            FilterBarSlot::ClearAll => Some(&self.clear_all),
            FilterBarSlot::TrailingAction => Some(&self.trailing_action),
        }
    }

    #[cfg(test)]
    pub(in crate::ui) fn slot_order(&self) -> Vec<FilterBarSlot> {
        let mut order = Vec::new();
        let mut child = self.root.first_child();
        while let Some(widget) = child {
            let name = widget.widget_name();
            let slot = FilterBarSlot::ALL
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
        let mut ancestor = widget.as_ref().parent();
        while let Some(parent) = ancestor {
            let name = parent.widget_name();
            if FilterBarSlot::ALL
                .into_iter()
                .any(|candidate| name == candidate.name())
            {
                return name == slot.name();
            }
            ancestor = parent.parent();
        }
        false
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

    /// Shared SEARCH-4a assertion used by every section's display contract:
    /// its existing query-clear path must remove both filter state and chip.
    #[cfg(test)]
    pub(in crate::ui) fn assert_search_cleared(&self, query: &str) {
        assert!(query.is_empty(), "the section remains search-filtered");
        assert!(
            self.slot_child(FilterBarSlot::Search).is_none(),
            "the section keeps a search chip after clearing"
        );
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

pub(in crate::ui) fn chooser_row(label: &str) -> gtk4::ListBoxRow {
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

pub(in crate::ui) fn count_label() -> gtk4::Label {
    let label = gtk4::Label::new(None);
    label.add_css_class("dim-label");
    label.add_css_class("caption");
    label
}

pub(in crate::ui) fn clear_all_button(label: &str) -> gtk4::Button {
    let button = gtk4::Button::with_label(label);
    button.add_css_class("flat");
    style_clear_all(&button);
    button
}

#[derive(Clone, Copy)]
pub(in crate::ui) enum CountPresentation<'a> {
    Plain(&'a str),
    RestrictedMarkup(&'a str),
}

pub(in crate::ui) fn present_count(label: &gtk4::Label, presentation: CountPresentation<'_>) {
    match presentation {
        CountPresentation::Plain(text) => {
            label.remove_css_class("accent");
            label.set_text(text);
        }
        CountPresentation::RestrictedMarkup(markup) => {
            label.set_markup(markup);
            label.add_css_class("accent");
        }
    }
}

pub(in crate::ui) fn facet_row() -> gtk4::Box {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    row.set_visible(false);
    row
}

pub(in crate::ui) fn css() -> String {
    use crate::ui::style::tokens::{CHIP_BG_ALPHA, CHIP_BG_HOVER_ALPHA};

    format!(
        ".{CHIP_CSS_CLASS} {{ border-radius: 9999px; padding: 2px 8px; \
         background-color: alpha(@accent_bg_color, {CHIP_BG_ALPHA}); color: @reprise_accent_text_color; }} \
         .{CHIP_CSS_CLASS}:hover {{ background-color: alpha(@accent_bg_color, {CHIP_BG_HOVER_ALPHA}); }} \
         .{ADD_FILTER_CSS_CLASS} {{ border: 1px dashed alpha(currentColor, 0.18); \
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
    slot.set_visible(kind == FilterBarSlot::Spacer);
    slot
}

fn fill(slot: &gtk4::Box, widget: &impl IsA<gtk4::Widget>) {
    clear(slot);
    widget.set_hexpand(false);
    slot.append(widget);
    // An empty visible child still makes GtkBox apply spacing around its slot.
    // Mirror the child's explicit visibility so absent slots consume nothing.
    slot.set_visible(widget.property("visible"));
    let weak_slot = slot.downgrade();
    widget.connect_visible_notify(move |widget| {
        let Some(slot) = weak_slot.upgrade() else {
            return;
        };
        if widget
            .parent()
            .is_some_and(|parent| parent == slot.clone().upcast::<gtk4::Widget>())
        {
            slot.set_visible(widget.property("visible"));
        }
    });
}

fn clear(slot: &gtk4::Box) {
    while let Some(child) = slot.first_child() {
        slot.remove(&child);
    }
    slot.set_visible(false);
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    use super::*;

    const NARROW_WIDTH: i32 = 800;
    const WIDE_WIDTH: i32 = 1_120;
    const NORMAL_SEARCH_CHIP_GAP: f32 = 14.0;
    const NORMAL_FACET_CHIP_GAP: f32 = 13.0;

    #[derive(Debug)]
    struct Geometry {
        root_width: f32,
        add_gap: Option<f32>,
        add_leading_x: f32,
        facet_width: f32,
        facet_trailing_gap: Option<f32>,
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
    fn fil_1d_search_slot_uses_the_real_scoped_removable_chip() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let layout = FilterBarLayout::new();
        let cleared = Rc::new(Cell::new(false));
        let flag = cleared.clone();

        layout.replace_scoped_search(SearchScope::Podcasts, "  wer  ", move || flag.set(true));

        let chip = layout
            .slot_child(FilterBarSlot::Search)
            .and_downcast::<gtk4::Button>()
            .expect("the search slot contains the canonical chip");
        assert_eq!(
            chip.label().as_deref(),
            Some("⌕ “wer” in episode titles  ×")
        );
        assert!(chip.has_css_class(CHIP_CSS_CLASS));
        assert_eq!(chip.height_request(), CHIP_MIN_HIT_PX);
        chip.emit_clicked();
        assert!(cleared.get(), "the × must clear the query");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_2a_slots_are_content_sized_and_only_the_spacer_expands_across_all_six_bars() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();

        for bar in [
            "Music", "Releases", "Concerts", "Podcasts", "YouTube", "Radio",
        ] {
            for (state, search_present, facets_full) in [
                ("empty", false, false),
                ("one search chip", true, false),
                ("one facet chip", false, true),
            ] {
                let narrow = measure_geometry(search_present, facets_full, NARROW_WIDTH);
                let wide = measure_geometry(search_present, facets_full, WIDE_WIDTH);
                assert_close(
                    wide.add_gap.unwrap_or(wide.add_leading_x),
                    narrow.add_gap.unwrap_or(narrow.add_leading_x),
                    &format!(
                        "{bar} moved Add filter away from the preceding {state}; narrow={narrow:?}; wide={wide:?}"
                    ),
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

                if facets_full {
                    assert_close(
                        narrow
                            .facet_trailing_gap
                            .expect("the full Facets slot has one chip"),
                        0.0,
                        &format!("{bar}'s Facets slot reserves width after its only chip"),
                    );
                    assert_close(
                        narrow.add_gap.expect("the facet chip precedes Add filter"),
                        NORMAL_FACET_CHIP_GAP,
                        &format!("{bar} left more than normal spacing after its facet chip"),
                    );
                } else {
                    assert_close(
                        narrow.facet_width,
                        0.0,
                        &format!("{bar}'s empty Facets slot still reserves width"),
                    );
                }

                if search_present {
                    assert_close(
                        narrow.add_gap.expect("the search chip precedes Add filter"),
                        NORMAL_SEARCH_CHIP_GAP,
                        &format!("{bar} left more than normal spacing after its search chip"),
                    );
                } else if !facets_full {
                    assert_close(
                        narrow.add_leading_x,
                        layout_content_start(),
                        &format!("{bar} left empty slots before Add filter"),
                    );
                }
            }
        }
    }

    fn measure_geometry(search_present: bool, facets_full: bool, width: i32) -> Geometry {
        let layout = FilterBarLayout::new();
        let search = search_present.then(|| gtk4::Button::with_label("⌕ falling"));
        if let Some(search) = &search {
            layout.fill_search(search);
        }

        let (facets, last_facet) = facet_container(facets_full);
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

        let preceding = last_facet
            .clone()
            .or_else(|| search.map(gtk4::glib::object::Cast::upcast));
        let add_bounds = add_filter
            .compute_bounds(layout.root())
            .expect("Add filter has filter-bar bounds");
        let facet_slot = layout.slot_widget(FilterBarSlot::Facets);
        let add_slot_bounds = layout
            .slot_widget(FilterBarSlot::AddFilter)
            .compute_bounds(layout.root())
            .expect("Add filter slot has filter-bar bounds");
        let slot_widths = all_slots().map(|slot| layout.slot_widget(slot).width() as f32);
        let geometry = Geometry {
            root_width: layout.root().width() as f32,
            add_gap: preceding.map(|preceding| {
                let preceding_bounds = preceding
                    .compute_bounds(layout.root())
                    .expect("preceding chip has filter-bar bounds");
                add_bounds.x() - preceding_bounds.x() - preceding_bounds.width()
            }),
            add_leading_x: add_slot_bounds.x(),
            facet_width: facet_slot.width() as f32,
            facet_trailing_gap: last_facet.as_ref().map(|facet| {
                let slot_bounds = facet_slot
                    .compute_bounds(layout.root())
                    .expect("the populated Facets slot has filter-bar bounds");
                let chip_bounds = facet
                    .compute_bounds(layout.root())
                    .expect("the facet chip has filter-bar bounds");
                slot_bounds.x() + slot_bounds.width() - chip_bounds.x() - chip_bounds.width()
            }),
            slot_widths,
        };
        window.close();
        geometry
    }

    fn layout_content_start() -> f32 {
        0.0
    }

    fn facet_container(full: bool) -> (gtk4::Widget, Option<gtk4::Widget>) {
        let facet = full.then(|| gtk4::Button::with_label("Genre: Metal"));
        let row = facet_row();
        if let Some(facet) = &facet {
            row.append(facet);
        }
        row.set_visible(full);
        let widget = row.upcast();
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
