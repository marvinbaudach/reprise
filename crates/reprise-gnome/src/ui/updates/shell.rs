//! Widget-only construction for the grouped Updates popover shell.

use gtk4::prelude::*;

use crate::ui::strings;

use super::badge;
use super::popover::{HISTORY_PAGE, LIST_PAGE, SCROLLER_MAX_HEIGHT};

const POPOVER_WIDTH: i32 = 336;

pub(super) struct UpdatesShell {
    pub button: gtk4::MenuButton,
    pub badge: gtk4::Label,
    pub popover: gtk4::Popover,
    pub stack: gtk4::Stack,
    pub news_section: gtk4::Box,
    pub list: gtk4::ListBox,
    pub empty: gtk4::Label,
    pub new_tag: gtk4::Label,
    pub history_row: gtk4::Button,
    pub history_row_count: gtk4::Label,
    pub history_page: gtk4::Box,
    pub fetch_button: gtk4::Button,
    pub fetch_stack: gtk4::Stack,
    pub spinner: gtk4::Spinner,
    pub updated: gtk4::Label,
    pub failure: gtk4::Label,
}

pub(super) fn build() -> UpdatesShell {
    let (button, badge) = badge::build_button();
    let popover = gtk4::Popover::new();
    popover.add_css_class("new-release-popover");

    let list = gtk4::ListBox::new();
    list.set_selection_mode(gtk4::SelectionMode::None);
    let empty = gtk4::Label::new(None);
    empty.set_wrap(true);
    empty.set_justify(gtk4::Justification::Center);
    empty.set_margin_top(12);
    empty.set_margin_bottom(12);
    let scroller = gtk4::ScrolledWindow::builder()
        .child(&list)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .propagate_natural_height(true)
        .max_content_height(SCROLLER_MAX_HEIGHT)
        .build();

    let updates_header = gtk4::Label::new(Some(&strings::text(strings::UPDATES_HEADER)));
    updates_header.add_css_class("new-release-header");
    updates_header.set_xalign(0.0);
    let header_label = gtk4::Label::new(Some(&strings::text(strings::UPDATES_NEW_RELEASES_HEADER)));
    header_label.add_css_class("new-release-header");
    header_label.set_xalign(0.0);
    header_label.set_hexpand(true);
    let new_tag = gtk4::Label::new(None);
    new_tag.add_css_class("new-release-tag");
    new_tag.set_visible(false);
    let header_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    header_row.append(&header_label);
    header_row.append(&new_tag);
    let news_section = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    news_section.append(&header_row);
    news_section.append(&scroller);

    let separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
    separator.add_css_class("new-release-separator");
    let history_row_label = gtk4::Label::new(Some(&strings::text(strings::SHOW_HISTORY)));
    history_row_label.add_css_class("new-release-history-label");
    let history_row_count = gtk4::Label::new(None);
    history_row_count.add_css_class("new-release-history-count");
    let history_text = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    history_text.set_hexpand(true);
    history_text.append(&history_row_label);
    history_text.append(&history_row_count);
    let history_content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    history_content.append(&gtk4::Image::from_icon_name(
        "document-open-recent-symbolic",
    ));
    history_content.append(&history_text);
    history_content.append(&gtk4::Image::from_icon_name("go-next-symbolic"));
    let history_row = gtk4::Button::builder()
        .child(&history_content)
        .css_classes(["flat", "new-release-history-row"])
        .build();

    let (footer, fetch_button, fetch_stack, spinner, updated, failure) = build_footer();
    let list_page = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    list_page.append(&updates_header);
    list_page.append(&news_section);
    list_page.append(&separator);
    list_page.append(&history_row);
    list_page.append(&footer);
    let history_page = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    let stack = gtk4::Stack::new();
    stack.set_transition_type(gtk4::StackTransitionType::SlideLeftRight);
    stack.set_transition_duration(crate::ui::motion::STANDARD_MS);
    stack.add_named(&list_page, Some(LIST_PAGE));
    stack.add_named(&history_page, Some(HISTORY_PAGE));
    stack.set_visible_child_name(LIST_PAGE);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.set_size_request(POPOVER_WIDTH, -1);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    content.set_margin_start(10);
    content.set_margin_end(10);
    content.append(&stack);
    popover.set_child(Some(&content));
    button.set_popover(Some(&popover));

    UpdatesShell {
        button,
        badge,
        popover,
        stack,
        news_section,
        list,
        empty,
        new_tag,
        history_row,
        history_row_count,
        history_page,
        fetch_button,
        fetch_stack,
        spinner,
        updated,
        failure,
    }
}

fn build_footer() -> (
    gtk4::Box,
    gtk4::Button,
    gtk4::Stack,
    gtk4::Spinner,
    gtk4::Label,
    gtk4::Label,
) {
    let icon = gtk4::Image::from_icon_name("view-refresh-symbolic");
    let spinner = gtk4::Spinner::new();
    let stack = gtk4::Stack::new();
    stack.add_named(&icon, Some("icon"));
    stack.add_named(&spinner, Some("spinner"));
    stack.set_visible_child_name("icon");
    let fetch_label = gtk4::Label::new(Some(&strings::text(strings::FETCH_NOW)));
    let fetch_content = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    fetch_content.append(&stack);
    fetch_content.append(&fetch_label);
    let fetch_button = gtk4::Button::builder()
        .child(&fetch_content)
        .css_classes(["flat", "new-release-ghost"])
        .build();
    let updated = gtk4::Label::new(None);
    updated.add_css_class("dim-label");
    let failure = gtk4::Label::new(None);
    failure.add_css_class("dim-label");
    failure.set_visible(false);
    let status = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    status.set_hexpand(true);
    status.set_halign(gtk4::Align::End);
    status.append(&updated);
    status.append(&failure);
    let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    footer.append(&fetch_button);
    footer.append(&status);
    (footer, fetch_button, stack, spinner, updated, failure)
}
