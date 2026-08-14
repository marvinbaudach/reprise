//! Widget-only construction for the grouped Updates popover shell.

use gtk4::prelude::*;

use crate::ui::feed_footer::FeedFooter;
use crate::ui::strings;

use super::badge;
use super::concerts_section::ConcertsSection;

const POPOVER_WIDTH: i32 = 380;

pub(super) struct UpdatesShell {
    pub button: gtk4::MenuButton,
    pub badge: gtk4::Label,
    pub popover: gtk4::Popover,
    pub news_section: gtk4::Box,
    pub concerts_section: ConcertsSection,
    pub list: gtk4::ListBox,
    pub empty: gtk4::Label,
    pub releases_header: gtk4::Button,
    pub new_tag: gtk4::Label,
    pub footer: FeedFooter,
}

pub(super) fn build() -> UpdatesShell {
    let (button, badge) = badge::build_button();
    let popover = gtk4::Popover::new();
    popover.add_css_class("new-release-popover");

    let list = gtk4::ListBox::new();
    list.set_selection_mode(gtk4::SelectionMode::None);
    let empty = gtk4::Label::new(None);
    empty.add_css_class("reprise-text-secondary");
    empty.set_wrap(true);
    empty.set_justify(gtk4::Justification::Center);
    empty.set_margin_top(12);
    empty.set_margin_bottom(12);

    let header_label = gtk4::Label::new(Some(&strings::text(strings::RELEASES)));
    header_label.add_css_class("new-release-header");
    header_label.set_xalign(0.0);
    header_label.set_hexpand(true);
    let new_tag = gtk4::Label::new(None);
    new_tag.add_css_class("new-release-tag");
    new_tag.set_visible(false);
    let header_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    header_row.append(&header_label);
    header_row.append(&new_tag);
    let releases_header = gtk4::Button::builder()
        .child(&header_row)
        .css_classes(["flat", "updates-section-header"])
        .build();
    let news_section = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    news_section.append(&releases_header);
    news_section.append(&list);

    let concerts_section = ConcertsSection::new();

    let updates_header = build_header();
    let list_page = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    list_page.append(&updates_header);
    list_page.append(&news_section);
    list_page.append(concerts_section.root());
    let footer = FeedFooter::new();

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.set_size_request(POPOVER_WIDTH, -1);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    content.set_margin_start(10);
    content.set_margin_end(10);
    content.append(&list_page);
    content.append(footer.widget());
    popover.set_child(Some(&content));
    button.set_popover(Some(&popover));

    UpdatesShell {
        button,
        badge,
        popover,
        news_section,
        concerts_section,
        list,
        empty,
        releases_header,
        new_tag,
        footer,
    }
}

fn build_header() -> gtk4::Box {
    let title = gtk4::Label::new(Some(&strings::text(strings::UPDATES_HEADER)));
    title.add_css_class("new-release-header");
    title.set_xalign(0.0);
    title.set_hexpand(true);
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    header.append(&title);
    header
}
