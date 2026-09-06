//! Shared refinement bar for every table-like source.

#![allow(dead_code)] // B1 lands the shared grammar before B2 moves callers onto it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_view::search_scope::SearchScope;

use crate::ui::filter_bar_layout::{self, FilterBarLayout};

pub(in crate::ui) const FACET_PAGE: &str = "facets";
pub(in crate::ui) const VALUE_PAGE: &str = "values";

type OnQueryChanged = Rc<dyn Fn(&str)>;
type OnChanged<F> = Rc<dyn Fn(F)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) struct FacetDescriptor {
    pub id: &'static str,
    pub label: String,
    pub multiple: bool,
    pub enabled: bool,
    pub tooltip: Option<String>,
}

impl FacetDescriptor {
    pub(in crate::ui) fn single(id: &'static str, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            multiple: false,
            enabled: true,
            tooltip: None,
        }
    }

    pub(in crate::ui) fn multiple(id: &'static str, label: impl Into<String>) -> Self {
        Self {
            multiple: true,
            ..Self::single(id, label)
        }
    }

    pub(in crate::ui) fn disabled(mut self, tooltip: impl Into<String>) -> Self {
        self.enabled = false;
        self.tooltip = Some(tooltip.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) struct ValueDescriptor {
    pub id: String,
    pub label: String,
}

impl ValueDescriptor {
    pub(in crate::ui) fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) struct SelectionDescriptor {
    pub facet_id: String,
    pub value_id: String,
    pub label: String,
    removable: bool,
    css_class: Option<&'static str>,
}

impl SelectionDescriptor {
    pub(in crate::ui) fn new(
        facet_id: impl Into<String>,
        value_id: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            facet_id: facet_id.into(),
            value_id: value_id.into(),
            label: label.into(),
            removable: true,
            css_class: None,
        }
    }

    pub(in crate::ui) fn action(
        facet_id: impl Into<String>,
        value_id: impl Into<String>,
        label: impl Into<String>,
        css_class: &'static str,
    ) -> Self {
        Self {
            facet_id: facet_id.into(),
            value_id: value_id.into(),
            label: label.into(),
            removable: false,
            css_class: Some(css_class),
        }
    }

    pub(in crate::ui) fn picker(
        facet_id: impl Into<String>,
        value_id: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            facet_id: facet_id.into(),
            value_id: value_id.into(),
            label: label.into(),
            removable: false,
            css_class: None,
        }
    }

    fn pair(&self) -> (String, String) {
        (self.facet_id.clone(), self.value_id.clone())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) struct CountText {
    text: String,
    markup: bool,
}

impl CountText {
    pub(in crate::ui) fn plain(text: String) -> Self {
        Self {
            text,
            markup: false,
        }
    }

    pub(in crate::ui) fn markup(text: String) -> Self {
        Self { text, markup: true }
    }
}

/// The source-specific projection behind the common filter grammar.
///
/// Facets and values deliberately use string identities. Only the resulting
/// domain filter is typed; this avoids multiplying GTK object wrappers for
/// identifiers that never leave the chooser.
pub(in crate::ui) trait FilterModel: 'static {
    type Filter: Clone + PartialEq + 'static;

    fn initial_filter(&self) -> Self::Filter;
    fn facets(&self) -> Vec<FacetDescriptor>;
    fn values(&self, facet_id: &str) -> Vec<ValueDescriptor>;
    fn apply(&self, query: &str, selections: &[(String, String)]) -> Self::Filter;
    fn persistence_key(&self) -> &'static str;
    fn query<'a>(&self, filter: &'a Self::Filter) -> &'a str;
    fn selections(&self, filter: &Self::Filter) -> Vec<SelectionDescriptor>;
    fn search_scope(&self) -> SearchScope;
    fn add_filter_label(&self) -> String;
    fn clear_all_label(&self) -> String;
    fn count_text(&self, shown: usize, total: usize, active: bool) -> CountText;

    fn clear_filter(&self) -> Self::Filter {
        self.apply("", &[])
    }

    fn is_active(&self, filter: &Self::Filter) -> bool {
        !self.query(filter).trim().is_empty() || !self.selections(filter).is_empty()
    }

    fn persist(&self, _previous: &Self::Filter, _filter: &Self::Filter) -> Result<(), String> {
        Ok(())
    }

    fn activate_selection(&self, _selection: &SelectionDescriptor) -> bool {
        false
    }
}

pub(in crate::ui) struct FilterBar<M: FilterModel> {
    pub(in crate::ui) root: gtk4::Box,
    pub(in crate::ui) layout: FilterBarLayout,
    pub(in crate::ui) chips: gtk4::Box,
    pub(in crate::ui) add_filter: gtk4::MenuButton,
    pub(in crate::ui) result_label: gtk4::Label,
    pub(in crate::ui) count: gtk4::Label,
    pub(in crate::ui) result: gtk4::Label,
    pub(in crate::ui) clear_all: gtk4::Button,
    pub(in crate::ui) facet_list: gtk4::ListBox,
    pub(in crate::ui) value_list: gtk4::ListBox,
    pub(in crate::ui) chooser_back: gtk4::Button,
    chooser_stack: gtk4::Stack,
    popover: gtk4::Popover,
    chooser_facets: RefCell<Vec<FacetDescriptor>>,
    chooser_values: RefCell<Vec<ValueDescriptor>>,
    selected_facet: RefCell<Option<FacetDescriptor>>,
    model: M,
    filter: RefCell<M::Filter>,
    committed_query: RefCell<String>,
    counts: Cell<(usize, usize)>,
    on_changed: RefCell<Option<OnChanged<M::Filter>>>,
    on_query_changed: RefCell<Option<OnQueryChanged>>,
}

impl<M: FilterModel> FilterBar<M> {
    pub(in crate::ui) fn new(model: M) -> Rc<Self> {
        let layout = FilterBarLayout::new();
        let root = layout.root().clone();
        let chips = filter_bar_layout::facet_row();
        layout.fill_facets(&chips);

        let chooser_stack = gtk4::Stack::new();
        chooser_stack.set_transition_type(gtk4::StackTransitionType::SlideLeftRight);
        chooser_stack.set_transition_duration(crate::ui::motion::STANDARD_MS);
        let facet_list = gtk4::ListBox::new();
        facet_list.add_css_class("boxed-list");
        chooser_stack.add_named(&page(&facet_list), Some(FACET_PAGE));
        let value_list = gtk4::ListBox::new();
        value_list.add_css_class("boxed-list");
        let chooser_back = gtk4::Button::from_icon_name("go-previous-symbolic");
        chooser_back.add_css_class("flat");
        chooser_back.set_tooltip_text(Some(&crate::ui::filter_bar_strings::text(
            crate::ui::filter_bar_strings::BACK,
        )));
        let value_page = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        value_page.set_margin_top(8);
        value_page.set_margin_bottom(8);
        value_page.set_margin_start(8);
        value_page.set_margin_end(8);
        value_page.append(&chooser_back);
        value_page.append(&value_list);
        chooser_stack.add_named(&value_page, Some(VALUE_PAGE));
        let popover = gtk4::Popover::builder().child(&chooser_stack).build();
        let add_filter = gtk4::MenuButton::builder()
            .label(model.add_filter_label())
            .popover(&popover)
            .build();
        add_filter.add_css_class("pill");
        filter_bar_layout::style_add_filter(&add_filter);
        layout.fill_add_filter(&add_filter);

        let result_label = filter_bar_layout::count_label();
        layout.fill_count(&result_label);
        let clear_all = filter_bar_layout::clear_all_button(&model.clear_all_label());
        clear_all.set_visible(false);
        layout.fill_clear_all(&clear_all);
        let filter = model.initial_filter();
        let bar = Rc::new(Self {
            root,
            layout,
            chips,
            add_filter,
            result_label: result_label.clone(),
            count: result_label.clone(),
            result: result_label,
            clear_all,
            facet_list,
            value_list,
            chooser_back,
            chooser_stack,
            popover,
            chooser_facets: RefCell::new(Vec::new()),
            chooser_values: RefCell::new(Vec::new()),
            selected_facet: RefCell::new(None),
            model,
            filter: RefCell::new(filter),
            committed_query: RefCell::new(String::new()),
            counts: Cell::new((0, 0)),
            on_changed: RefCell::new(None),
            on_query_changed: RefCell::new(None),
        });
        wire(&bar);
        bar.rebuild();
        bar
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    pub(in crate::ui) fn filter(&self) -> M::Filter {
        self.filter.borrow().clone()
    }

    pub(in crate::ui) fn set_on_changed(&self, callback: impl Fn(M::Filter) + 'static) {
        self.on_changed.replace(Some(Rc::new(callback)));
    }

    pub(in crate::ui) fn set_on_query_changed(&self, callback: impl Fn(&str) + 'static) {
        self.on_query_changed.replace(Some(Rc::new(callback)));
    }

    pub(in crate::ui) fn set_query(self: &Rc<Self>, query: &str) {
        let current = self.filter();
        let query = query.trim();
        if self.model.query(&current) == query {
            return;
        }
        let selections = selection_pairs(&self.model.selections(&current));
        self.apply_filter(self.model.apply(query, &selections), false);
    }

    pub(in crate::ui) fn set_committed_query(self: &Rc<Self>, query: &str) {
        if *self.committed_query.borrow() == query {
            return;
        }
        self.committed_query.replace(query.to_owned());
        self.rebuild();
    }

    pub(in crate::ui) fn set_counts(&self, shown: usize, total: usize) {
        self.counts.set((shown, total));
        self.rebuild_count();
    }

    pub(in crate::ui) fn clear_all(self: &Rc<Self>) {
        self.apply_filter(self.model.clear_filter(), true);
    }

    pub(in crate::ui) fn replace_filter(self: &Rc<Self>, filter: M::Filter) {
        self.apply_filter(filter, true);
    }

    pub(in crate::ui) fn refresh(self: &Rc<Self>) {
        self.rebuild();
    }

    pub(in crate::ui) fn model(&self) -> &M {
        &self.model
    }

    pub(in crate::ui) fn request_search_clear(&self) {
        let callback = self.on_query_changed.borrow().clone();
        if let Some(callback) = callback {
            callback("");
        }
    }

    pub(in crate::ui) fn select(self: &Rc<Self>, facet_id: &str, value_id: &str) {
        let filter = self.filter();
        let mut selections = selection_pairs(&self.model.selections(&filter));
        let multiple = self
            .model
            .facets()
            .into_iter()
            .find(|facet| facet.id == facet_id)
            .is_some_and(|facet| facet.multiple);
        if !multiple {
            selections.retain(|(facet, _)| facet != facet_id);
        }
        let pair = (facet_id.to_owned(), value_id.to_owned());
        if !selections.contains(&pair) {
            selections.push(pair);
        }
        let query = self.model.query(&filter).to_owned();
        self.apply_filter(self.model.apply(&query, &selections), true);
    }

    pub(in crate::ui) fn remove(self: &Rc<Self>, facet_id: &str, value_id: &str) {
        let filter = self.filter();
        let mut selections = selection_pairs(&self.model.selections(&filter));
        selections.retain(|pair| pair != &(facet_id.to_owned(), value_id.to_owned()));
        let query = self.model.query(&filter).to_owned();
        self.apply_filter(self.model.apply(&query, &selections), true);
    }

    fn apply_filter(self: &Rc<Self>, filter: M::Filter, announce_query: bool) {
        let previous = self.filter();
        if previous == filter {
            return;
        }
        if let Err(error) = self.model.persist(&previous, &filter) {
            tracing::warn!(
                %error,
                key = self.model.persistence_key(),
                "could not persist filter"
            );
        }
        let previous_query = self.model.query(&previous).to_owned();
        let next_query = self.model.query(&filter).to_owned();
        self.filter.replace(filter.clone());
        if next_query.is_empty() {
            self.committed_query.replace(String::new());
        }
        self.popover.popdown();
        self.rebuild();
        if announce_query && previous_query != next_query {
            let callback = self.on_query_changed.borrow().clone();
            if let Some(callback) = callback {
                callback(&next_query);
            }
        }
        let callback = self.on_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(filter);
        }
    }

    fn rebuild(self: &Rc<Self>) {
        self.rebuild_search();
        self.rebuild_chips();
        self.rebuild_facets();
        self.rebuild_count();
        let filter = self.filter();
        self.clear_all.set_visible(self.model.is_active(&filter));
    }

    fn rebuild_search(self: &Rc<Self>) {
        let query = self.committed_query.borrow().clone();
        let weak = Rc::downgrade(self);
        self.layout
            .replace_scoped_search(self.model.search_scope(), &query, move || {
                if let Some(bar) = weak.upgrade() {
                    bar.request_search_clear();
                }
            });
    }

    fn rebuild_chips(self: &Rc<Self>) {
        while let Some(child) = self.chips.first_child() {
            self.chips.remove(&child);
        }
        for selection in self.model.selections(&self.filter()) {
            let label = if selection.removable {
                format!("{}  ×", selection.label)
            } else {
                selection.label.clone()
            };
            let button = gtk4::Button::with_label(&label);
            button.add_css_class("flat");
            button.add_css_class(filter_bar_layout::CHIP_CSS_CLASS);
            if let Some(css_class) = selection.css_class {
                button.add_css_class(css_class);
            }
            button.set_size_request(-1, 20);
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(bar) = weak.upgrade() {
                    if bar.model.activate_selection(&selection) {
                        return;
                    }
                    if selection.removable {
                        bar.remove(&selection.facet_id, &selection.value_id);
                    } else if let Some(facet) = bar
                        .model
                        .facets()
                        .into_iter()
                        .find(|facet| facet.id == selection.facet_id)
                    {
                        bar.show_values(facet, true);
                        bar.popover.popup();
                    }
                }
            });
            self.chips.append(&button);
        }
        self.chips.set_visible(self.chips.first_child().is_some());
    }

    fn rebuild_facets(&self) {
        self.facet_list.remove_all();
        let filter = self.filter();
        let selections = selection_pairs(&self.model.selections(&filter));
        let facets = self
            .model
            .facets()
            .into_iter()
            .filter(|facet| {
                let selected = selections
                    .iter()
                    .any(|(selected_facet, _)| selected_facet.as_str() == facet.id);
                (!selected || facet.multiple)
                    && self
                        .model
                        .values(facet.id)
                        .into_iter()
                        .any(|value| !selections.contains(&(facet.id.to_owned(), value.id)))
            })
            .collect::<Vec<_>>();
        for facet in &facets {
            let row = filter_bar_layout::chooser_row(&facet.label);
            row.set_sensitive(facet.enabled);
            row.set_tooltip_text(facet.tooltip.as_deref());
            self.facet_list.append(&row);
        }
        self.add_filter.set_sensitive(!facets.is_empty());
        self.chooser_facets.replace(facets);
        self.chooser_stack.set_visible_child_name(FACET_PAGE);
    }

    fn show_values(&self, facet: FacetDescriptor, include_selected: bool) {
        self.value_list.remove_all();
        let filter = self.filter();
        let selections = selection_pairs(&self.model.selections(&filter));
        let values = self
            .model
            .values(facet.id)
            .into_iter()
            .filter(|value| {
                include_selected || !selections.contains(&(facet.id.to_owned(), value.id.clone()))
            })
            .collect::<Vec<_>>();
        for value in &values {
            self.value_list
                .append(&filter_bar_layout::chooser_row(&value.label));
        }
        self.selected_facet.replace(Some(facet));
        self.chooser_values.replace(values);
        self.chooser_stack.set_visible_child_name(VALUE_PAGE);
    }

    fn rebuild_count(&self) {
        let filter = self.filter();
        let (shown, total) = self.counts.get();
        let count = self
            .model
            .count_text(shown, total, self.model.is_active(&filter));
        let presentation = if count.markup {
            filter_bar_layout::CountPresentation::RestrictedMarkup(&count.text)
        } else {
            filter_bar_layout::CountPresentation::Plain(&count.text)
        };
        filter_bar_layout::present_count(&self.result_label, presentation);
    }

    #[cfg(test)]
    fn committed_query(&self) -> String {
        self.committed_query.borrow().clone()
    }

    #[cfg(test)]
    fn chip_labels(&self) -> Vec<String> {
        std::iter::successors(self.chips.first_child(), WidgetExt::next_sibling)
            .filter_map(|widget| widget.downcast::<gtk4::Button>().ok())
            .filter_map(|button| button.label())
            .map(|label| label.trim_end_matches("  ×").to_owned())
            .collect()
    }

    pub(in crate::ui) fn count_text(&self) -> String {
        self.result_label.text().to_string()
    }
}

fn selection_pairs(selections: &[SelectionDescriptor]) -> Vec<(String, String)> {
    selections.iter().map(SelectionDescriptor::pair).collect()
}

fn page(child: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    page.set_margin_top(8);
    page.set_margin_bottom(8);
    page.set_margin_start(8);
    page.set_margin_end(8);
    page.append(child);
    page
}

fn wire<M: FilterModel>(bar: &Rc<FilterBar<M>>) {
    let weak = Rc::downgrade(bar);
    bar.clear_all.connect_clicked(move |_| {
        if let Some(bar) = weak.upgrade() {
            bar.clear_all();
        }
    });
    let weak = Rc::downgrade(bar);
    bar.facet_list.connect_row_activated(move |_, row| {
        let Some(bar) = weak.upgrade() else {
            return;
        };
        let facet = bar
            .chooser_facets
            .borrow()
            .get(row.index() as usize)
            .cloned();
        if let Some(facet) = facet {
            bar.show_values(facet, false);
        }
    });
    let weak = Rc::downgrade(bar);
    bar.value_list.connect_row_activated(move |_, row| {
        let Some(bar) = weak.upgrade() else {
            return;
        };
        let facet = bar.selected_facet.borrow().clone();
        let value = bar
            .chooser_values
            .borrow()
            .get(row.index() as usize)
            .cloned();
        if let (Some(facet), Some(value)) = (facet, value) {
            bar.select(facet.id, &value.id);
        }
    });
    let weak = Rc::downgrade(bar);
    bar.chooser_back.connect_clicked(move |_| {
        if let Some(bar) = weak.upgrade() {
            bar.rebuild_facets();
        }
    });
}

#[cfg(test)]
#[path = "filter_bar_tests.rs"]
mod tests;
