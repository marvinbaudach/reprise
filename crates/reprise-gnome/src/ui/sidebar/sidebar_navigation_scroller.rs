//! Sidebar navigation-list scroller: keeps the vertical scrollbar hidden and
//! non-interactive unless the navigation list actually overflows.

use gtk4::prelude::*;

// The production stylesheet adds six pixels around the heading-and-five-row
// block beyond the rows' authored heights. Measure the composed block rather
// than reconstructing a value that silently clips its last row.
const LIBRARY_BLOCK_MIN_HEIGHT: i32 = 232;

pub(super) fn build_navigation_scroller(
    listbox: &gtk4::ListBox,
    device_section: &impl IsA<gtk4::Widget>,
) -> gtk4::ScrolledWindow {
    let places = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    places.append(listbox);
    places.append(device_section);
    let scrolled = gtk4::ScrolledWindow::builder()
        .child(&places)
        .vexpand(true)
        .min_content_height(LIBRARY_BLOCK_MIN_HEIGHT)
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
    // input-parity: ACC-8 keyboard=scrolled-window-navigation
    let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
    scroll.connect_scroll({
        let adjustment = adjustment.clone();
        move |_, _, dy| {
            let step = if dy.abs() < f64::EPSILON {
                0.0
            } else {
                adjustment.step_increment().max(24.0) * dy.signum()
            };
            let maximum = (adjustment.upper() - adjustment.page_size()).max(adjustment.lower());
            adjustment.set_value((adjustment.value() + step).clamp(adjustment.lower(), maximum));
            gtk4::glib::Propagation::Stop
        }
    });
    scrolled.add_controller(scroll);
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
