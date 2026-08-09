use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gdk;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use super::preferences_search_index::{collect_rows, IndexedRow, PageHitCounts};
#[cfg(test)]
use super::preferences_search_results::TestOrigin;
use super::preferences_search_results::{MovedResult, PreparedResult};
use crate::ui::preferences_window::{PageId, PAGE_ORDER};

const SIDEBAR_DIM_OPACITY: f64 = 0.42;
const SEARCH_FIELD_WIDTH: i32 = 340;
const PAGES_CHILD: &str = "settings-pages";
const RESULTS_CHILD: &str = "settings-results";
const ALL_RESULTS_ROW_NAME: &str = "settings-search-all-results";

type PageMaterializer = Rc<dyn Fn(PageId)>;

#[derive(Clone)]
struct SidebarPageEntry {
    page: PageId,
    row: gtk4::ListBoxRow,
    count: gtk4::Label,
}

pub(in crate::ui) struct SettingsSearch {
    sidebar: gtk4::ListBox,
    page_stack: adw::ViewStack,
    content_stack: gtk4::Stack,
    content_title: adw::WindowTitle,
    entry: gtk4::SearchEntry,
    revealer: gtk4::Revealer,
    toggle: gtk4::ToggleButton,
    all_results_row: gtk4::ListBoxRow,
    all_results_count: gtk4::Label,
    page_entries: Vec<SidebarPageEntry>,
    results_box: gtk4::Box,
    filter_layout: crate::ui::filter_bar_layout::FilterBarLayout,
    count_label: gtk4::Label,
    end_of_results: Rc<crate::ui::end_of_results::EndOfResults>,
    materialize_page: PageMaterializer,
    index: RefCell<Option<Vec<IndexedRow>>>,
    moved: RefCell<Vec<MovedResult>>,
    current_query: RefCell<String>,
    active: Cell<bool>,
    previous_page: Cell<PageId>,
    weak_self: RefCell<std::rc::Weak<Self>>,
}

impl SettingsSearch {
    pub(super) fn install(
        sidebar: &gtk4::ListBox,
        page_stack: &adw::ViewStack,
        content_title: &adw::WindowTitle,
        content_header: &adw::HeaderBar,
        content_toolbar: &adw::ToolbarView,
        materialize_page: PageMaterializer,
    ) -> Rc<Self> {
        let page_entries = PAGE_ORDER
            .iter()
            .enumerate()
            .map(|(index, page)| {
                let row = sidebar
                    .row_at_index(index as i32)
                    .expect("every Preferences page has one sidebar row");
                SidebarPageEntry {
                    page: *page,
                    count: add_non_measuring_count(&row),
                    row,
                }
            })
            .collect();
        let (all_results_row, all_results_count) = result_sidebar_row();

        let toggle = gtk4::ToggleButton::builder()
            .icon_name("system-search-symbolic")
            .tooltip_text(crate::ui::strings::text(
                crate::ui::strings::SEARCH_SETTINGS,
            ))
            .build();
        toggle.add_css_class("flat");
        content_header.pack_end(&toggle);

        let entry = gtk4::SearchEntry::builder()
            .placeholder_text(crate::ui::strings::text(
                crate::ui::strings::SEARCH_SETTINGS,
            ))
            .max_width_chars(40)
            .build();
        entry.set_size_request(SEARCH_FIELD_WIDTH, -1);
        entry.set_hexpand(false);
        let search_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        search_row.set_halign(gtk4::Align::Center);
        search_row.set_margin_top(6);
        search_row.set_margin_bottom(6);
        search_row.append(&entry);
        let revealer = gtk4::Revealer::new();
        revealer.set_transition_type(gtk4::RevealerTransitionType::SlideDown);
        revealer.set_child(Some(&search_row));
        content_toolbar.add_top_bar(&revealer);

        let filter_layout = crate::ui::filter_bar_layout::FilterBarLayout::new();
        let count_label = crate::ui::filter_bar_layout::count_label();
        count_label.set_use_markup(true);
        count_label.set_halign(gtk4::Align::End);
        filter_layout.fill_count(&count_label);
        let results_box = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        results_box.set_margin_top(6);
        results_box.set_margin_bottom(64);
        results_box.set_margin_start(12);
        results_box.set_margin_end(12);
        let results_scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .child(&results_box)
            .build();
        let results_overlay = gtk4::Overlay::new();
        results_overlay.set_vexpand(true);
        results_overlay.set_child(Some(&results_scrolled));
        let end_of_results = crate::ui::end_of_results::EndOfResults::install(
            &results_overlay,
            &results_scrolled,
            &results_box,
            crate::ui::end_of_results::ResultsUnit::Settings,
        );
        let results_page = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        results_page.append(filter_layout.root());
        results_page.append(&results_overlay);

        let content_stack = gtk4::Stack::new();
        content_stack.set_vexpand(true);
        content_stack.add_named(page_stack, Some(PAGES_CHILD));
        content_stack.add_named(&results_page, Some(RESULTS_CHILD));
        content_stack.set_visible_child_name(PAGES_CHILD);
        content_toolbar.set_content(Some(&content_stack));

        let search = Rc::new(Self {
            sidebar: sidebar.clone(),
            page_stack: page_stack.clone(),
            content_stack,
            content_title: content_title.clone(),
            entry,
            revealer,
            toggle,
            all_results_row,
            all_results_count,
            page_entries,
            results_box,
            filter_layout,
            count_label,
            end_of_results,
            materialize_page,
            index: RefCell::new(None),
            moved: RefCell::new(Vec::new()),
            current_query: RefCell::new(String::new()),
            active: Cell::new(false),
            previous_page: Cell::new(PageId::Appearance),
            weak_self: RefCell::new(std::rc::Weak::new()),
        });
        search.weak_self.replace(Rc::downgrade(&search));
        search.connect_signals();
        {
            let search = Rc::downgrade(&search);
            search
                .upgrade()
                .expect("Settings search is alive during installation")
                .end_of_results
                .connect_recover(move || {
                    if let Some(search) = search.upgrade() {
                        search.entry.set_text("");
                    }
                });
        }
        search
    }

    fn connect_signals(self: &Rc<Self>) {
        let search = Rc::downgrade(self);
        self.toggle.connect_toggled(move |toggle| {
            let Some(search) = search.upgrade() else {
                return;
            };
            search.revealer.set_reveal_child(toggle.is_active());
            if toggle.is_active() {
                search.entry.grab_focus();
            } else {
                search.entry.set_text("");
                search.leave_search_mode();
            }
        });

        let search = Rc::downgrade(self);
        self.entry.connect_changed(move |entry| {
            let Some(search) = search.upgrade() else {
                return;
            };
            search.apply_query(entry.text().as_str());
        });

        let search = Rc::downgrade(self);
        self.entry.connect_stop_search(move |_| {
            let Some(search) = search.upgrade() else {
                return;
            };
            if search.entry.text().is_empty() {
                search.toggle.set_active(false);
            } else {
                search.entry.set_text("");
                search.entry.grab_focus();
            }
        });
    }

    pub(super) fn bind_shortcuts(self: &Rc<Self>, widget: &impl IsA<gtk4::Widget>) {
        let keys = gtk4::EventControllerKey::new();
        let search = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            if key == gdk::Key::f && modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
                if let Some(search) = search.upgrade() {
                    search.reveal();
                }
                return gtk4::glib::Propagation::Stop;
            }
            gtk4::glib::Propagation::Proceed
        });
        widget.add_controller(keys);
    }

    pub(super) fn pin_sidebar_width(&self, split: &adw::NavigationSplitView) {
        let sidebar = self.sidebar.clone();
        split.connect_map(move |split| {
            let width = sidebar.width();
            if width <= 0 {
                return;
            }
            split.set_sidebar_width_unit(adw::LengthUnit::Px);
            split.set_min_sidebar_width(f64::from(width));
            split.set_max_sidebar_width(f64::from(width));
        });
    }

    pub(super) fn reveal(&self) {
        self.toggle.set_active(true);
        self.revealer.set_reveal_child(true);
        self.entry.grab_focus();
    }

    pub(super) fn close(&self) {
        self.entry.set_text("");
        self.leave_search_mode();
    }

    fn apply_query(&self, query: &str) {
        let query = query.trim();
        if query.is_empty() {
            self.leave_search_mode();
            return;
        }
        if self.active.get() && self.current_query.borrow().as_str() == query {
            return;
        }
        self.current_query.replace(query.to_owned());
        self.ensure_index();
        if !self.active.replace(true) {
            self.previous_page.set(self.visible_page());
            self.sidebar.prepend(&self.all_results_row);
        }
        let counts = self.show_results(query);
        self.all_results_count.set_text(&counts.total().to_string());
        for entry in &self.page_entries {
            let count = counts.for_page(entry.page);
            entry.count.set_text(&count.to_string());
            entry.count.set_visible(true);
            entry
                .row
                .set_opacity(if count == 0 { SIDEBAR_DIM_OPACITY } else { 1.0 });
        }
        self.content_stack.set_visible_child_name(RESULTS_CHILD);
        self.content_title
            .set_title(&crate::ui::strings::text(crate::ui::strings::ALL_RESULTS));
        self.sidebar.select_row(Some(&self.all_results_row));
    }

    fn leave_search_mode(&self) {
        self.current_query.borrow_mut().clear();
        self.restore_results();
        if !self.active.replace(false) {
            return;
        }
        if self.all_results_row.parent().is_some() {
            self.sidebar.remove(&self.all_results_row);
        }
        for entry in &self.page_entries {
            entry.count.set_visible(false);
            entry.row.set_opacity(1.0);
        }
        self.content_stack.set_visible_child_name(PAGES_CHILD);
        let page = self.previous_page.get();
        let index = PAGE_ORDER
            .iter()
            .position(|candidate| *candidate == page)
            .unwrap_or(0) as i32;
        self.sidebar
            .select_row(self.sidebar.row_at_index(index).as_ref());
    }

    fn show_results(&self, query: &str) -> PageHitCounts {
        self.restore_results();
        let moves: Vec<_> = self
            .index
            .borrow()
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|entry| entry.matches(query))
            .cloned()
            .filter_map(|indexed| {
                let title = indexed.document.title.clone();
                let Some(prepared) = PreparedResult::capture(indexed) else {
                    tracing::warn!(%title, "settings search row has no list origin");
                    return None;
                };
                Some(prepared)
            })
            .collect();
        let counts = PageHitCounts::from_rows(moves.iter().map(PreparedResult::indexed));
        for prepared in moves {
            self.move_result(prepared, query);
        }
        let shown = self.moved.borrow().len();
        let total = self.index.borrow().as_ref().map_or(0, std::vec::Vec::len);
        self.update_filter_bar(query, shown, total);
        self.end_of_results
            .update(crate::ui::end_of_results::EndOfResultsInput {
                shown,
                total,
                query: query.to_owned(),
                facets_restrict: false,
            });
        counts
    }

    fn move_result(&self, prepared: PreparedResult, query: &str) {
        let weak = self.weak_self.borrow().clone();
        let moved = prepared.render(query, move |page, target| {
            if let Some(search) = weak.upgrade() {
                search.open_result(page, &target);
            }
        });
        self.results_box.append(moved.widget());
        self.moved.borrow_mut().push(moved);
    }

    fn restore_results(&self) {
        let moved = self.moved.take();
        for moved in moved {
            moved.restore();
        }
    }

    fn update_filter_bar(&self, query: &str, shown: usize, total: usize) {
        let weak = self.weak_self.borrow().clone();
        self.filter_layout.replace_search(
            &crate::ui::strings::settings_search_chip_label(query.trim()),
            &crate::ui::filter_bar_strings::remove_search_label(query.trim()),
            move || {
                if let Some(search) = weak.upgrade() {
                    search.entry.set_text("");
                }
            },
        );
        self.count_label
            .set_markup(&crate::ui::strings::settings_filtered_count_markup(
                shown, total,
            ));
        let clear = gtk4::Button::with_label(&crate::ui::strings::text(
            crate::ui::strings::SETTINGS_CLEAR_ALL,
        ));
        crate::ui::filter_bar_layout::style_clear_all(&clear);
        let weak = self.weak_self.borrow().clone();
        clear.connect_clicked(move |_| {
            if let Some(search) = weak.upgrade() {
                search.entry.set_text("");
            }
        });
        self.filter_layout.fill_clear_all(&clear);
    }

    fn open_result(&self, page: PageId, row: &adw::PreferencesRow) {
        let target = row.clone();
        let expanders = self
            .moved
            .borrow()
            .iter()
            .find(|moved| moved.matches(&target))
            .map(MovedResult::expanders)
            .unwrap_or_default();
        self.entry.set_text("");
        self.toggle.set_active(false);
        let index = PAGE_ORDER
            .iter()
            .position(|candidate| *candidate == page)
            .unwrap_or(0) as i32;
        self.sidebar
            .select_row(self.sidebar.row_at_index(index).as_ref());
        for expander in expanders {
            expander.set_expanded(true);
        }
        target.set_visible(true);
        target.grab_focus();
    }

    fn ensure_index(&self) {
        if self.index.borrow().is_some() {
            return;
        }
        for page in PAGE_ORDER {
            (self.materialize_page)(page);
        }
        let mut index = Vec::new();
        for page in PAGE_ORDER {
            let Some(holder) = self
                .page_stack
                .child_by_name(page.name())
                .and_then(|child| child.downcast::<adw::Bin>().ok())
            else {
                continue;
            };
            let Some(root) = adw::prelude::BinExt::child(&holder) else {
                continue;
            };
            collect_rows(&root, page, &mut index);
        }
        self.index.replace(Some(index));
    }

    fn visible_page(&self) -> PageId {
        let Some(name) = self.page_stack.visible_child_name() else {
            return PageId::Appearance;
        };
        PAGE_ORDER
            .iter()
            .find(|page| page.name() == name.as_str())
            .copied()
            .unwrap_or(PageId::Appearance)
    }

    #[cfg(test)]
    pub(super) fn entry(&self) -> gtk4::SearchEntry {
        self.entry.clone()
    }

    #[cfg(test)]
    pub(super) fn all_results_row(&self) -> gtk4::ListBoxRow {
        self.all_results_row.clone()
    }

    #[cfg(test)]
    pub(super) fn all_results_count(&self) -> gtk4::Label {
        self.all_results_count.clone()
    }

    #[cfg(test)]
    pub(super) fn page_row(&self, page: PageId) -> gtk4::ListBoxRow {
        self.page_entry(page).row.clone()
    }

    #[cfg(test)]
    pub(super) fn page_count(&self, page: PageId) -> gtk4::Label {
        self.page_entry(page).count.clone()
    }

    #[cfg(test)]
    fn page_entry(&self, page: PageId) -> &SidebarPageEntry {
        self.page_entries
            .iter()
            .find(|entry| entry.page == page)
            .expect("requested page has a sidebar entry")
    }

    #[cfg(test)]
    pub(super) fn origin_for(&self, row: &adw::PreferencesRow) -> TestOrigin {
        let moved = self.moved.borrow();
        let moved = moved
            .iter()
            .find(|moved| moved.matches(row))
            .expect("the requested row is a current result");
        moved.origin()
    }

    #[cfg(test)]
    pub(super) fn result_path_for(&self, row: &adw::PreferencesRow) -> String {
        self.moved
            .borrow()
            .iter()
            .find(|moved| moved.matches(row))
            .map(MovedResult::path)
            .expect("the requested row is a current result")
    }

    #[cfg(test)]
    pub(super) fn result_path_button_for(&self, row: &adw::PreferencesRow) -> gtk4::Button {
        self.moved
            .borrow()
            .iter()
            .find(|moved| moved.matches(row))
            .map(MovedResult::path_button)
            .expect("the requested row is a current result")
    }

    #[cfg(test)]
    pub(super) fn is_revealed(&self) -> bool {
        self.revealer.reveals_child()
    }
}

fn add_non_measuring_count(row: &gtk4::ListBoxRow) -> gtk4::Label {
    let child = row.child().expect("Preferences sidebar rows have content");
    row.set_child(None::<&gtk4::Widget>);
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&child));
    let count = gtk4::Label::new(None);
    count.add_css_class("caption");
    count.add_css_class("dim-label");
    count.set_halign(gtk4::Align::End);
    count.set_valign(gtk4::Align::Center);
    count.set_margin_end(6);
    count.set_visible(false);
    overlay.add_overlay(&count);
    overlay.set_measure_overlay(&count, false);
    row.set_child(Some(&overlay));
    count
}

fn result_sidebar_row() -> (gtk4::ListBoxRow, gtk4::Label) {
    let icon = gtk4::Image::from_icon_name("system-search-symbolic");
    let label = gtk4::Label::new(Some(&crate::ui::strings::text(
        crate::ui::strings::ALL_RESULTS,
    )));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    content.set_margin_top(8);
    content.set_margin_bottom(8);
    content.set_margin_start(6);
    content.set_margin_end(6);
    content.append(&icon);
    content.append(&label);
    let row = gtk4::ListBoxRow::new();
    row.set_widget_name(ALL_RESULTS_ROW_NAME);
    // a11y-semantics: role=list-item name=all-results state=focusable action=focus/navigate
    row.set_focusable(true);
    row.set_child(Some(&content));
    let count = add_non_measuring_count(&row);
    count.set_visible(true);
    (row, count)
}

pub(super) fn css() -> String {
    ".reprise-settings-result-path { font-size: 10.5px; letter-spacing: 0.05em; }".to_owned()
}

#[cfg(test)]
#[path = "preferences_search_tests.rs"]
mod display_tests;
