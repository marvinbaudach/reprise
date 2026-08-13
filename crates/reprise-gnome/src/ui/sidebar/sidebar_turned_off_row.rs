use gtk4::prelude::*;

use super::sidebar_presentation::{self, NavIcon};

pub(super) fn build(title: &str) -> gtk4::ListBoxRow {
    let row = sidebar_presentation::build_nav_row(title, None, NavIcon::TurnedOff);
    row.add_css_class("dim-label");
    row.add_css_class("reprise-turned-off-modules");

    let content = row
        .child()
        .and_then(|child| child.downcast::<gtk4::Button>().ok())
        .and_then(|button| button.child())
        .and_then(|child| child.downcast::<gtk4::Box>().ok());
    if let Some(content) = content {
        let next = gtk4::Image::from_icon_name("go-next-symbolic");
        next.set_pixel_size(16);
        next.set_valign(gtk4::Align::Center);
        content.append(&next);
    }
    row
}
