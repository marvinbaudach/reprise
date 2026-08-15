use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

pub(super) fn no_match_page(on_clear: Rc<dyn Fn()>) -> adw::StatusPage {
    let page = adw::StatusPage::builder()
        .icon_name("system-search-symbolic")
        .vexpand(true)
        .build();
    let clear = gtk4::Button::builder()
        .label("")
        .css_classes(["pill"])
        .halign(gtk4::Align::Center)
        .build();
    clear.connect_clicked(move |_| on_clear());
    page.set_child(Some(&clear));
    page
}

impl super::ReviewState {
    pub(super) fn set_content_child(&self) {
        let name = if self.sorted.n_items() > 0 {
            "rows"
        } else if self.query.borrow().is_empty() {
            "empty"
        } else {
            "no-match"
        };
        self.content.set_visible_child_name(name);
    }
}
