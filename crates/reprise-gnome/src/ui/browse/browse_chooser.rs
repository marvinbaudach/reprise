//! Facet and value chooser for [`BrowseBar`].

use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::queries::{self, BrowseFacet, BrowseFilter, BrowseValue};
use rusqlite::Connection;

use super::browse_bar::{apply_selection, BrowseBar};
use crate::ui::browse_filter_strings as filter_strings;

pub(super) const POPUP_MIN_HEIGHT: i32 = 200;
pub(super) const FACET_PAGE: &str = "facets";
pub(super) const VALUE_PAGE: &str = "values";

pub(super) fn browse_popup_min_height(_option_count: usize) -> i32 {
    POPUP_MIN_HEIGHT
}

pub(super) fn build_chooser() -> (
    gtk4::Stack,
    gtk4::ListBox,
    gtk4::Button,
    gtk4::SearchEntry,
    gtk4::ListBox,
) {
    let stack = gtk4::Stack::new();
    stack.set_transition_type(gtk4::StackTransitionType::SlideLeftRight);

    let facet_page = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    facet_page.set_margin_top(8);
    facet_page.set_margin_bottom(8);
    facet_page.set_margin_start(8);
    facet_page.set_margin_end(8);
    let heading = gtk4::Label::new(Some(&filter_strings::text(filter_strings::ADD_FILTER)));
    heading.add_css_class("heading");
    heading.set_halign(gtk4::Align::Start);
    let facet_list = gtk4::ListBox::new();
    facet_list.add_css_class("boxed-list");
    facet_page.append(&heading);
    facet_page.append(&facet_list);
    stack.add_named(&facet_page, Some(FACET_PAGE));

    let value_page = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    value_page.set_margin_top(8);
    value_page.set_margin_bottom(8);
    value_page.set_margin_start(8);
    value_page.set_margin_end(8);
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let back = gtk4::Button::from_icon_name("go-previous-symbolic");
    back.add_css_class("flat");
    back.set_tooltip_text(Some(&filter_strings::text(filter_strings::BACK)));
    let search = gtk4::SearchEntry::builder()
        .placeholder_text(filter_strings::text(filter_strings::SEARCH_VALUES))
        .hexpand(true)
        .build();
    header.append(&back);
    header.append(&search);
    let value_list = gtk4::ListBox::new();
    value_list.add_css_class("boxed-list");
    let scrolled = gtk4::ScrolledWindow::builder()
        .child(&value_list)
        .min_content_height(POPUP_MIN_HEIGHT)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .build();
    value_page.append(&header);
    value_page.append(&scrolled);
    stack.add_named(&value_page, Some(VALUE_PAGE));
    stack.set_visible_child_name(FACET_PAGE);

    (stack, facet_list, back, search, value_list)
}

pub(super) fn chooser_row(title: &str, count: Option<&str>) -> gtk4::ListBoxRow {
    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    content.set_margin_top(7);
    content.set_margin_bottom(7);
    content.set_margin_start(10);
    content.set_margin_end(10);
    let title = gtk4::Label::new(Some(title));
    title.set_halign(gtk4::Align::Start);
    title.set_hexpand(true);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    content.append(&title);
    if let Some(count) = count {
        let count = gtk4::Label::new(Some(count));
        count.add_css_class("dim-label");
        count.add_css_class("caption");
        content.append(&count);
    }
    gtk4::ListBoxRow::builder().child(&content).build()
}

pub(super) fn wire_chooser(bar: &Rc<BrowseBar>) {
    {
        let weak = Rc::downgrade(bar);
        bar.add_filter.connect_active_notify(move |button| {
            if !button.is_active() {
                return;
            }
            if let Some(bar) = weak.upgrade() {
                let filter = bar.filter();
                bar.rebuild_facet_page(&filter);
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
        bar.value_search.connect_search_changed(move |_| {
            if let Some(bar) = weak.upgrade() {
                bar.rebuild_value_rows();
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
                .visible_values
                .borrow()
                .get(row.index() as usize)
                .cloned();
            let Some((facet, value)) = bar.chooser_facet.get().zip(value) else {
                return;
            };
            let current = bar.filter();
            bar.apply_filter(apply_selection(&current, facet, Some(value)));
        });
    }
    {
        let weak = Rc::downgrade(bar);
        bar.chooser_back.connect_clicked(move |_| {
            let Some(bar) = weak.upgrade() else {
                return;
            };
            let filter = bar.filter();
            bar.rebuild_facet_page(&filter);
        });
    }
}

pub(super) fn load_values(
    conn: &Connection,
    facet: BrowseFacet,
    filter: &BrowseFilter,
) -> Vec<BrowseValue> {
    queries::query_browse_values(conn, facet, filter).unwrap_or_else(|error| {
        tracing::warn!(%error, ?facet, "could not load browse facet values");
        Vec::new()
    })
}
