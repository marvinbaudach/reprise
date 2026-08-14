//! Shared geometry for release and concert rows in the Updates popover.

use gtk4::prelude::*;

pub(super) fn content(
    cover: &impl IsA<gtk4::Widget>,
    title_text: &str,
    meta_text: &str,
    trailing: &impl IsA<gtk4::Widget>,
) -> gtk4::Box {
    let title = gtk4::Label::new(Some(title_text));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    title.add_css_class("new-release-title");

    let meta = gtk4::Label::new(Some(meta_text));
    meta.set_xalign(0.0);
    meta.set_ellipsize(gtk4::pango::EllipsizeMode::None);
    meta.add_css_class("new-release-meta");

    let text = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.append(&title);
    text.append(&meta);

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    row.add_css_class("new-release-row");
    row.append(cover);
    row.append(&text);
    row.append(trailing);
    row
}
