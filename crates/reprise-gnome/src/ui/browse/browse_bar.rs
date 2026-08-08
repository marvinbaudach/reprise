//! Cascading Genre/Artist/Album controls for the Library source.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::db::Db;
use reprise_core::queries::{self, BrowseFacet, BrowseFilter, BrowseValue};
use reprise_core::view_source::ViewSource;

use super::browse_bar_chips::{
    append_chip, apply_selection, available_facets, displayed_value, facet_label, filter_chips,
    remove_filter, restored_filter, value_matches_search,
};
use super::browse_chooser::{
    browse_popup_min_height, build_chooser, chooser_row, load_values, wire_chooser, FACET_PAGE,
    VALUE_PAGE,
};
use crate::ui::browse_filter_strings as filter_strings;
use crate::ui::track_list::Shared;

const SMOKE_ENV: &str = "REPRISE_SMOKE_BROWSE";
/// FIL-7: the sticky settings key for the AI-exclude filter.
const EXCLUDE_AI_KEY: &str = "filter.exclude_ai";
pub(in crate::ui) const CHIP_CSS_CLASS: &str = "reprise-filter-chip";
/// FIL-1c: the place pill's own class — outlined, so a location never reads as
/// one of the filled filter chips beside it.
pub(in crate::ui) const PLACE_PILL_CSS_CLASS: &str = "reprise-place-pill";
const POPOVER_CSS_CLASS: &str = "reprise-filter-popover";
type OnChanged = Rc<dyn Fn(BrowseFilter)>;
type OnVoid = Rc<dyn Fn()>;
/// Minimum content height (px) of the filter bar. Both the empty state (the
/// tall "+ Add filter" pill) and the active state (compact chips) are pinned
/// to this so toggling a filter never changes the bar's height and shifts the
/// track table (QA #8). Sized to the taller of the two — the pill.
const FILTER_BAR_MIN_HEIGHT: i32 = 34;

/// Chip and value-popover rules; installed app-wide by [`super::style`].
pub(in crate::ui) fn css() -> String {
    use super::style::tokens::{CHIP_BG_ALPHA, CHIP_BG_HOVER_ALPHA};
    format!(
        ".{CHIP_CSS_CLASS} {{ border-radius: 9999px; padding: 2px 8px; \
         background-color: alpha(@accent_bg_color, {CHIP_BG_ALPHA}); color: @reprise_accent_text_color; }} \
         .{CHIP_CSS_CLASS}:hover {{ background-color: alpha(@accent_bg_color, {CHIP_BG_HOVER_ALPHA}); }} \
         .{PLACE_PILL_CSS_CLASS} {{ border-radius: 9999px; padding: 2px 10px; \
         border: 1px solid alpha(currentColor, 0.30); background-color: transparent; }} \
         .{PLACE_PILL_CSS_CLASS}:hover {{ background-color: alpha(currentColor, 0.08); }} \
         .{POPOVER_CSS_CLASS} contents {{ min-width: 300px; min-height: {}px; }}",
        browse_popup_min_height(0)
    )
}

pub struct BrowseBar {
    root: gtk4::Box,
    search: RefCell<String>,
    source: RefCell<ViewSource>,
    track_source: Cell<bool>,
    is_library: Cell<bool>,
    preference_visible: Cell<bool>,
    section_label: gtk4::Label,
    chips: gtk4::FlowBox,
    pub(super) add_filter: gtk4::MenuButton,
    chooser_stack: gtk4::Stack,
    pub(super) facet_list: gtk4::ListBox,
    pub(super) chooser_back: gtk4::Button,
    pub(super) value_search: gtk4::SearchEntry,
    pub(super) value_list: gtk4::ListBox,
    result_label: gtk4::Label,
    clear_all: gtk4::Button,
    /// FIL-1c: the left zone holding the place pill; empty at sidebar places.
    place_zone: gtk4::Box,
    /// Divides place zone from filter zone, shown only when both are populated.
    zone_separator: gtk4::Separator,
    #[cfg_attr(not(test), allow(dead_code))]
    scope_button: RefCell<Option<gtk4::Button>>,
    pub(super) chooser_facets: RefCell<Vec<BrowseFacet>>,
    pub(super) chooser_facet: Cell<Option<BrowseFacet>>,
    chooser_values: RefCell<Vec<BrowseValue>>,
    pub(super) visible_values: RefCell<Vec<String>>,
    filter: RefCell<BrowseFilter>,
    /// FIL-7: sticky opt-in "Hide AI music" state (Library-only).
    exclude_ai: Cell<bool>,
    /// The facet-chooser row index of the "Hide AI music" entry, when present —
    /// so its activation is distinguished from a real facet.
    chooser_ai_row_index: Cell<Option<usize>>,
    result_count: Cell<Option<(usize, usize)>>,
    conn: Rc<Db>,
    on_changed: RefCell<Option<OnChanged>>,
    on_search_cleared: RefCell<Option<OnVoid>>,
    on_clear_all: RefCell<Option<OnVoid>>,
    on_scope_cleared: RefCell<Option<OnVoid>>,
}

impl BrowseBar {
    pub fn new(conn: Rc<Db>) -> Rc<Self> {
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        root.set_margin_top(6);
        root.set_margin_bottom(6);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.add_css_class("toolbar");
        // Pin the bar to a constant height so switching between the empty
        // "+ Add filter" pill and the active chip row never shifts the track
        // table below it (QA #8). Sized to the taller state (the pill).
        root.set_size_request(-1, FILTER_BAR_MIN_HEIGHT);

        let section_label = gtk4::Label::new(Some(&filter_strings::text(filter_strings::FILTERS)));
        section_label.add_css_class("dim-label");
        section_label.add_css_class("caption-heading");

        let chips = gtk4::FlowBox::builder()
            .selection_mode(gtk4::SelectionMode::None)
            .column_spacing(6)
            .row_spacing(4)
            .max_children_per_line(20)
            .hexpand(true)
            .halign(gtk4::Align::Fill)
            .build();

        let popover = gtk4::Popover::new();
        popover.add_css_class(POPOVER_CSS_CLASS);
        let (chooser_stack, facet_list, chooser_back, value_search, value_list) = build_chooser();
        popover.set_child(Some(&chooser_stack));

        let add_label = gtk4::Label::new(Some(&format!(
            "+ {}",
            filter_strings::text(filter_strings::ADD_FILTER)
        )));
        let add_filter = gtk4::MenuButton::new();
        add_filter.set_child(Some(&add_label));
        add_filter.set_popover(Some(&popover));
        add_filter.add_css_class("pill");
        add_filter.update_property(&[gtk4::accessible::Property::Label(&filter_strings::text(
            filter_strings::ADD_FILTER,
        ))]);
        append_chip(&chips, &add_filter);

        let result_label = gtk4::Label::new(None);
        result_label.add_css_class("dim-label");
        result_label.add_css_class("caption");
        result_label.set_visible(false);

        let clear_all = gtk4::Button::with_label(&format!(
            "{} ×",
            filter_strings::text(filter_strings::CLEAR_ALL)
        ));
        clear_all.add_css_class("flat");
        clear_all.add_css_class(CHIP_CSS_CLASS);
        clear_all.set_visible(false);

        // FIL-1c: two zones. The place zone answers "where am I", the filter
        // zone "what is withheld here" — they never share a shape or a label.
        let place_zone = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        place_zone.set_visible(false);
        let zone_separator = gtk4::Separator::new(gtk4::Orientation::Vertical);
        zone_separator.set_visible(false);

        root.append(&place_zone);
        root.append(&zone_separator);
        root.append(&section_label);
        root.append(&chips);
        root.append(&result_label);
        root.append(&clear_all);

        let initial_exclude_ai =
            reprise_core::library::settings::get_bool(&conn, EXCLUDE_AI_KEY, false)
                .unwrap_or(false);
        let bar = Rc::new(Self {
            root,
            search: RefCell::new(String::new()),
            source: RefCell::new(ViewSource::Library),
            track_source: Cell::new(true),
            is_library: Cell::new(true),
            preference_visible: Cell::new(true),
            section_label,
            chips,
            add_filter,
            chooser_stack,
            facet_list,
            chooser_back,
            value_search,
            value_list,
            result_label,
            clear_all,
            place_zone,
            zone_separator,
            scope_button: RefCell::new(None),
            chooser_facets: RefCell::new(Vec::new()),
            chooser_facet: Cell::new(None),
            chooser_values: RefCell::new(Vec::new()),
            visible_values: RefCell::new(Vec::new()),
            filter: RefCell::new(BrowseFilter::default()),
            exclude_ai: Cell::new(initial_exclude_ai),
            chooser_ai_row_index: Cell::new(None),
            result_count: Cell::new(None),
            conn,
            on_changed: RefCell::new(None),
            on_search_cleared: RefCell::new(None),
            on_clear_all: RefCell::new(None),
            on_scope_cleared: RefCell::new(None),
        });
        {
            let weak = Rc::downgrade(&bar);
            bar.clear_all.connect_clicked(move |_| {
                let Some(bar) = weak.upgrade() else {
                    return;
                };
                let callback = bar.on_clear_all.borrow().clone();
                if let Some(callback) = callback {
                    callback();
                }
            });
        }
        wire_chooser(&bar);
        bar.sync_visibility();
        bar.refresh();
        bar
    }

    pub fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub fn filter(&self) -> BrowseFilter {
        self.filter.borrow().clone()
    }

    /// FIL-7: the sticky "Hide AI music" state. `run_query` re-reads this each
    /// reload (and gates it on Library).
    pub(in crate::ui) fn exclude_ai(&self) -> bool {
        self.exclude_ai.get()
    }

    /// Clears the AI-exclude state and persists it **without** triggering its
    /// own reload — for "Clear all" (FIL-2), whose caller reloads once after
    /// clearing search + facets + this together.
    pub(in crate::ui) fn clear_exclude_ai(&self) {
        self.exclude_ai.set(false);
        if let Err(error) =
            reprise_core::library::settings::set_bool(&self.conn, EXCLUDE_AI_KEY, false)
        {
            tracing::warn!(%error, "could not clear the AI-exclude filter state");
        }
    }

    /// Whether the "Hide AI music" filter is offered here: Library-only.
    fn ai_filter_available(&self) -> bool {
        self.is_library.get()
    }

    /// Toggles the sticky AI-exclude filter and reloads (via the browse-changed
    /// callback, which `run_query` follows to re-read `exclude_ai`). No-op when
    /// unchanged.
    pub(in crate::ui) fn set_exclude_ai(self: &Rc<Self>, value: bool) {
        if self.exclude_ai.get() == value {
            return;
        }
        self.exclude_ai.set(value);
        if let Err(error) =
            reprise_core::library::settings::set_bool(&self.conn, EXCLUDE_AI_KEY, value)
        {
            tracing::warn!(%error, "could not persist the AI-exclude filter state");
        }
        self.add_filter.popdown();
        self.refresh();
        self.sync_visibility();
        let callback = self.on_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(self.filter());
        }
    }

    /// The browse filter as the reload path applies it: facets only act in
    /// the Library source (`track_list_reload::reload` uses default elsewhere).
    fn effective_filter(&self) -> BrowseFilter {
        if self.is_library.get() {
            self.filter.borrow().clone()
        } else {
            BrowseFilter::default()
        }
    }

    pub(in crate::ui) fn restore_filter(self: &Rc<Self>, filter: &BrowseFilter) {
        let filter = restored_filter(filter);
        *self.filter.borrow_mut() = filter;
        self.refresh();
    }

    pub fn set_on_changed(&self, callback: impl Fn(BrowseFilter) + 'static) {
        *self.on_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub fn set_on_search_cleared(&self, callback: impl Fn() + 'static) {
        *self.on_search_cleared.borrow_mut() = Some(Rc::new(callback));
    }

    pub fn set_on_clear_all(&self, callback: impl Fn() + 'static) {
        *self.on_clear_all.borrow_mut() = Some(Rc::new(callback));
    }

    pub fn set_on_scope_cleared(&self, callback: impl Fn() + 'static) {
        *self.on_scope_cleared.borrow_mut() = Some(Rc::new(callback));
    }

    pub fn set_source_context(self: &Rc<Self>, source: &ViewSource) {
        *self.source.borrow_mut() = source.clone();
        self.track_source
            .set(super::filter_restriction::is_track_source(source));
        self.is_library.set(matches!(source, ViewSource::Library));
        self.refresh();
        self.sync_visibility();
    }

    /// FIL-1d: the scope this bar's search chip speaks for — derived from the
    /// source it is currently describing, never guessed.
    pub(in crate::ui) fn search_scope(&self) -> reprise_view::search_scope::SearchScope {
        reprise_view::search_scope::scope_for(&self.source.borrow())
    }

    pub fn set_search(self: &Rc<Self>, text: &str) {
        *self.search.borrow_mut() = text.to_string();
        self.refresh();
        self.sync_visibility();
    }

    pub fn set_preference_visible(&self, visible: bool) {
        self.preference_visible.set(visible);
        self.sync_visibility();
    }

    fn sync_visibility(&self) {
        let search = self.search.borrow().clone();
        let filter = self.effective_filter();
        let exclude_ai = self.exclude_ai.get() && self.ai_filter_available();
        let source = self.source.borrow().clone();
        let filters_restrict =
            super::filter_restriction::filters_restrict(&search, &filter, exclude_ai);
        let restricted = super::filter_restriction::is_restricted(&search, &filter, exclude_ai);
        let has_place_pill = super::filter_restriction::has_place_pill(&source);
        let visible = super::filter_restriction::row_visible(
            self.track_source.get(),
            restricted,
            has_place_pill,
            self.preference_visible.get(),
        );
        self.root.set_visible(visible);
        // FIL-1c: the FILTER heading describes the filter zone only — a place
        // is not a filter and must never be labelled as one.
        self.section_label.set_visible(filters_restrict);
        self.zone_separator
            .set_visible(has_place_pill && filters_restrict);
        self.clear_all.set_visible(filters_restrict);
        tracing::info!(
            visible,
            restricted,
            has_place_pill,
            "filter row visibility updated"
        );
    }

    pub fn set_result_count(&self, filtered: usize, total: usize) {
        self.result_count.set(Some((filtered, total)));
        let (markup, accented) = filter_strings::result_count_markup(filtered, total);
        self.result_label.set_markup(&markup);
        if accented {
            self.result_label.add_css_class("accent");
        } else {
            self.result_label.remove_css_class("accent");
        }
        self.result_label.set_visible(true);
    }

    pub(in crate::ui) fn result_count(&self) -> Option<(usize, usize)> {
        self.result_count.get()
    }

    #[cfg(test)]
    pub(in crate::ui) fn place_button(&self) -> Option<gtk4::Button> {
        self.scope_button.borrow().clone()
    }

    #[cfg(test)]
    pub(in crate::ui) fn section_label_visible(&self) -> bool {
        self.section_label.is_visible()
    }

    #[cfg(test)]
    pub(in crate::ui) fn zone_separator_visible(&self) -> bool {
        self.zone_separator.is_visible()
    }

    pub fn hide_result_count(&self) {
        self.result_count.set(None);
        self.result_label.set_visible(false);
    }

    pub fn refresh(self: &Rc<Self>) {
        let stored_filter = self.filter();
        let effective_filter = self.effective_filter();
        self.rebuild_place_zone();
        self.rebuild_chips(&effective_filter);
        self.rebuild_facet_page(&stored_filter);
    }

    /// FIL-1c: the place zone carries at most one pill, and only where no
    /// sidebar row already names the location.
    fn rebuild_place_zone(self: &Rc<Self>) {
        while let Some(child) = self.place_zone.first_child() {
            self.place_zone.remove(&child);
        }
        self.scope_button.borrow_mut().take();
        let source = self.source.borrow().clone();
        let Some(place) = super::filter_restriction::place_pill_label(&source) else {
            self.place_zone.set_visible(false);
            return;
        };
        let button = super::browse_bar_chips::build_place_pill(&place);
        let weak = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            let Some(bar) = weak.upgrade() else {
                return;
            };
            let callback = bar.on_scope_cleared.borrow().clone();
            if let Some(callback) = callback {
                callback();
            }
        });
        self.place_zone.append(&button);
        self.place_zone.set_visible(true);
        *self.scope_button.borrow_mut() = Some(button);
    }

    pub(super) fn apply_filter(self: &Rc<Self>, next: BrowseFilter) {
        let current = self.filter();
        if next == current {
            return;
        }
        *self.filter.borrow_mut() = next.clone();
        self.add_filter.popdown();
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            let Some(bar) = weak.upgrade() else {
                return;
            };
            bar.refresh();
            bar.sync_visibility();
            let callback = bar.on_changed.borrow().clone();
            if let Some(callback) = callback {
                callback(next);
            }
        });
    }

    fn rebuild_chips(self: &Rc<Self>, filter: &BrowseFilter) {
        // FlowBox wraps appended widgets in an implicit FlowBoxChild. Removing
        // that wrapper does not unparent our persistent MenuButton from it, so
        // detach the button before clearing the wrappers and appending it again.
        if let Some(wrapper) = self
            .add_filter
            .parent()
            .and_downcast::<gtk4::FlowBoxChild>()
        {
            wrapper.set_child(gtk4::Widget::NONE);
        }
        self.chips.remove_all();
        let query = self.search.borrow().trim().to_string();
        if !query.is_empty() {
            // FIL-1d: Music and its sibling track sources keep "in any field";
            // Missing files says "in file paths", because that is what its
            // list is made of.
            let weak = Rc::downgrade(self);
            let button = super::search_chip::build(self.search_scope(), &query, move || {
                let Some(bar) = weak.upgrade() else {
                    return;
                };
                let callback = bar.on_search_cleared.borrow().clone();
                if let Some(callback) = callback {
                    callback();
                }
            });
            append_chip(&self.chips, &button);
        }
        for chip in filter_chips(filter) {
            let button = gtk4::Button::with_label(&format!("{}  ×", chip.label));
            button.add_css_class("flat");
            button.add_css_class(CHIP_CSS_CLASS);
            button.update_property(&[gtk4::accessible::Property::Label(
                &chip.accessible_remove_label,
            )]);
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                let Some(bar) = weak.upgrade() else {
                    return;
                };
                let next = remove_filter(&bar.filter(), chip.facet);
                bar.apply_filter(next);
            });
            append_chip(&self.chips, &button);
        }
        // FIL-7: the active "Hide AI music" filter shows as its own chip whose ×
        // turns it off (FIL-1a).
        if self.exclude_ai.get() && self.ai_filter_available() {
            let button = gtk4::Button::with_label(&format!(
                "{}  ×",
                crate::ui::strings::text(crate::ui::strings::FILTER_HIDE_AI)
            ));
            button.add_css_class("flat");
            button.add_css_class(CHIP_CSS_CLASS);
            button.update_property(&[gtk4::accessible::Property::Label(
                &crate::ui::strings::remove_hide_ai_filter(),
            )]);
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(bar) = weak.upgrade() {
                    bar.set_exclude_ai(false);
                }
            });
            append_chip(&self.chips, &button);
        }
        if self.is_library.get() {
            append_chip(&self.chips, &self.add_filter);
        }
        let ai_addable = self.ai_filter_available() && !self.exclude_ai.get();
        self.add_filter
            .set_sensitive(!available_facets(filter).is_empty() || ai_addable);
    }

    pub(super) fn rebuild_facet_page(&self, filter: &BrowseFilter) {
        self.facet_list.remove_all();
        let facets = available_facets(filter);
        // FIL-7: "Hide AI music" is an addable boolean filter when the switch is
        // on and it is not already active.
        let ai_addable = self.ai_filter_available() && !self.exclude_ai.get();
        if facets.is_empty() && !ai_addable {
            self.facet_list.append(&chooser_row(
                &filter_strings::text(filter_strings::NO_FILTERS_AVAILABLE),
                None,
            ));
        } else {
            for facet in &facets {
                self.facet_list
                    .append(&chooser_row(&facet_label(*facet), None));
            }
        }
        let facet_count = facets.len();
        *self.chooser_facets.borrow_mut() = facets;
        if ai_addable {
            self.facet_list.append(&chooser_row(
                &crate::ui::strings::text(crate::ui::strings::FILTER_HIDE_AI),
                None,
            ));
            self.chooser_ai_row_index.set(Some(facet_count));
        } else {
            self.chooser_ai_row_index.set(None);
        }
        self.chooser_stack.set_visible_child_name(FACET_PAGE);
    }

    /// The facet-chooser row index of the "Hide AI music" entry, if present.
    pub(super) fn chooser_ai_row_index(&self) -> Option<usize> {
        self.chooser_ai_row_index.get()
    }

    pub(super) fn show_values(&self, facet: BrowseFacet) {
        let filter = self.filter();
        let values = {
            let conn = &self.conn;
            load_values(conn, facet, &filter)
        };
        self.chooser_facet.set(Some(facet));
        *self.chooser_values.borrow_mut() = values;
        self.value_search.set_text("");
        self.rebuild_value_rows();
        self.chooser_stack.set_visible_child_name(VALUE_PAGE);
        self.value_search.grab_focus();
    }

    pub(super) fn rebuild_value_rows(&self) {
        self.value_list.remove_all();
        let Some(facet) = self.chooser_facet.get() else {
            return;
        };
        let search = self.value_search.text();
        let mut visible = Vec::new();
        let values = self.chooser_values.borrow().clone();
        for value in values {
            let display = displayed_value(facet, &value.value);
            if !value_matches_search(&display, &search) {
                continue;
            }
            self.value_list.append(&chooser_row(
                &display,
                Some(&reprise_core::format::format_thousands(value.count)),
            ));
            visible.push(value.value);
        }
        *self.visible_values.borrow_mut() = visible;
    }

    fn select_raw(self: &Rc<Self>, facet: BrowseFacet, value: &str) -> bool {
        let filter = self.filter();
        let found = {
            let conn = &self.conn;
            load_values(conn, facet, &filter)
                .iter()
                .any(|candidate| candidate.value == value)
        };
        if !found {
            return false;
        }
        self.apply_filter(apply_selection(&filter, facet, Some(value.to_string())));
        true
    }
}

pub(in crate::ui) fn arm_smoke(shared: &Rc<Shared>) {
    let Ok(value) = std::env::var(SMOKE_ENV) else {
        return;
    };
    let selections: VecDeque<_> = value
        .split('|')
        .filter_map(|part| {
            let (name, value) = part.split_once(':')?;
            let facet = match name {
                "genre" => BrowseFacet::Genre,
                "artist" => BrowseFacet::Artist,
                "album" => BrowseFacet::Album,
                "year" => BrowseFacet::Year,
                "rating" => BrowseFacet::Rating,
                _ => return None,
            };
            Some((facet, value.to_string()))
        })
        .collect();
    let shared_weak = Rc::downgrade(shared);
    glib::idle_add_local_once(move || {
        schedule_smoke_step(shared_weak, Rc::new(RefCell::new(selections)));
    });
}

fn schedule_smoke_step(
    shared_weak: std::rc::Weak<Shared>,
    selections: Rc<RefCell<VecDeque<(BrowseFacet, String)>>>,
) {
    glib::timeout_add_local_once(Duration::from_millis(25), move || {
        let Some(shared) = shared_weak.upgrade() else {
            return;
        };
        let selection = selections.borrow_mut().pop_front();
        if let Some((facet, value)) = selection {
            if !shared.browse_bar.select_raw(facet, &value) {
                tracing::warn!(?facet, %value, "browse smoke value not found");
            }
            schedule_smoke_step(Rc::downgrade(&shared), selections);
            return;
        }
        let browse = shared.browse_filter.borrow().clone();
        let sort = shared.sort.borrow().clone();
        let filter = shared.filter.borrow().clone();
        let ids = {
            let conn = &shared.conn;
            queries::query_track_ids_browsed(
                conn,
                &reprise_core::view_source::ViewSource::Library,
                &sort.field,
                &sort.dir,
                &filter,
                &browse,
                &[],
            )
        };
        let chips: Vec<_> = filter_chips(&browse)
            .into_iter()
            .map(|chip| chip.label)
            .collect();
        let result_count = shared.browse_bar.result_count();
        tracing::info!(
            ?browse,
            ?chips,
            ?result_count,
            ?ids,
            "browse smoke completed"
        );
    });
}

#[cfg(test)]
#[path = "browse_bar_tests.rs"]
mod tests;
