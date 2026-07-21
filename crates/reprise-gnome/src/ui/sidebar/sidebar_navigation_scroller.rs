//! Sidebar navigation-list scroller: keeps the vertical scrollbar hidden and
//! non-interactive unless the navigation list actually overflows.

use gtk4::prelude::*;

pub(super) fn build_navigation_scroller(listbox: &gtk4::ListBox) -> gtk4::ScrolledWindow {
    let scrolled = gtk4::ScrolledWindow::builder()
        .child(listbox)
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .build();
    let adjustment = scrolled.vadjustment();
    adjustment.connect_changed({
        let scrolled = scrolled.downgrade();
        move |adjustment| {
            let Some(scrolled) = scrolled.upgrade() else {
                return;
            };
            update_navigation_scrollbar(&scrolled, adjustment);
        }
    });
    scrolled.connect_map({
        let adjustment = adjustment.clone();
        move |scrolled| update_navigation_scrollbar(scrolled, &adjustment)
    });
    scrolled.vscrollbar().connect_visible_notify({
        let scrolled = scrolled.downgrade();
        let adjustment = adjustment.clone();
        move |_| {
            let Some(scrolled) = scrolled.upgrade() else {
                return;
            };
            update_navigation_scrollbar(&scrolled, &adjustment);
        }
    });
    update_navigation_scrollbar(&scrolled, &adjustment);
    scrolled
}

fn update_navigation_scrollbar(scrolled: &gtk4::ScrolledWindow, adjustment: &gtk4::Adjustment) {
    const OVERFLOW_EPSILON_PX: f64 = 0.5;

    let overflow = adjustment.upper() > adjustment.page_size() + OVERFLOW_EPSILON_PX;
    let scrollbar = scrolled.vscrollbar();
    if scrollbar.is_visible() != overflow {
        scrollbar.set_visible(overflow);
    }
    scrollbar.set_can_target(overflow);
}
