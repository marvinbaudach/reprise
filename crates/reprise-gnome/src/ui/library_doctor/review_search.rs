use std::rc::Rc;
use std::time::Instant;

use gtk4::prelude::*;
use libadwaita as adw;

pub(super) fn no_match_page(on_clear: Rc<dyn Fn()>) -> adw::StatusPage {
    let page = adw::StatusPage::builder()
        .icon_name("system-search-symbolic")
        .vexpand(true)
        .build();
    let clear = gtk4::Button::builder()
        .label(crate::ui::strings::text(
            crate::ui::strings::DOCTOR_CLEAR_FILTERS,
        ))
        .action_name("win.clear-all-filters")
        .css_classes(["pill"])
        .halign(gtk4::Align::Center)
        .build();
    clear.connect_clicked(move |_| on_clear());
    page.set_child(Some(&clear));
    page
}

impl super::ReviewState {
    pub(super) fn set_query(self: &Rc<Self>, query: &str) {
        let started = Instant::now();
        let query = query.trim();
        if self.query.borrow().as_str() == query {
            return;
        }
        *self.query.borrow_mut() = query.to_owned();
        self.snapshot.borrow_mut().apply_query(query);
        self.filter.changed(gtk4::FilterChange::Different);
        self.push_query_scope();
        self.refresh_filter_summary();
        self.refresh_master_check();
        self.refresh_action_summary(self.ready_count.get());
        let albums = self.snapshot.borrow().albums.clone();
        self.album_headers.push_selection(&albums);
        self.set_content_child();
        let rows = self.snapshot.borrow().totals.changes;
        tracing::debug!(
            path = "search",
            rows,
            elapsed_us = started.elapsed().as_micros(),
            "DOCTOR_REVIEW_REFRESH path"
        );
    }

    pub(super) fn push_query_scope(&self) {
        let scope = {
            let query = self.query.borrow();
            if query.is_empty() {
                None
            } else {
                Some(self.snapshot.borrow().visible_selectable_row_ids())
            }
        };
        self.session.borrow_mut().set_query_scope(scope);
    }

    pub(super) fn set_content_child(&self) {
        let query = self.query.borrow().clone();
        let category = self.category.get();
        let name = if self.sorted.n_items() > 0 {
            "rows"
        } else if !query.is_empty() || category.is_some() {
            "no-match"
        } else {
            "empty"
        };
        if name == "no-match" {
            let hidden = self.snapshot.borrow().unfiltered_changes;
            if let Some(page) = self
                .content
                .child_by_name("no-match")
                .and_downcast::<adw::StatusPage>()
            {
                let category = category.map(|category| crate::ui::strings::text(category.label()));
                let (title, description) = match (query.is_empty(), category.as_deref()) {
                    (false, Some(category)) => (
                        crate::ui::strings::doctor_query_and_category_no_match_title(
                            &query, category,
                        ),
                        crate::ui::strings::doctor_query_and_category_no_match_description(
                            hidden, &query, category,
                        ),
                    ),
                    (false, None) => (
                        crate::ui::strings::doctor_no_match_title(&query),
                        crate::ui::strings::doctor_no_match_description(hidden),
                    ),
                    (true, Some(category)) => (
                        crate::ui::strings::doctor_category_no_match_title(category),
                        crate::ui::strings::doctor_category_no_match_description(hidden, category),
                    ),
                    (true, None) => unreachable!("the no-match page requires an active filter"),
                };
                page.set_title(&title);
                page.set_description(Some(&description));
            }
        }
        self.content.set_visible_child_name(name);
    }
}
