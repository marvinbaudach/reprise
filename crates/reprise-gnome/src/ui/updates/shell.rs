//! Widget-only construction for the grouped Updates popover shell.

use gtk4::prelude::*;

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
    pub nothing_new: gtk4::Label,
    pub new_tag: gtk4::Label,
    pub releases_jump: gtk4::Button,
    pub releases_jump_label: gtk4::Label,
    pub concerts_jump: gtk4::Button,
    pub concerts_jump_label: gtk4::Label,
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
    news_section.append(&list);

    let concerts_section = ConcertsSection::new();
    let nothing_new = gtk4::Label::new(Some(&strings::text(strings::UPDATES_NOTHING_NEW)));
    nothing_new.add_css_class("dim-label");
    nothing_new.set_halign(gtk4::Align::Center);
    nothing_new.set_margin_top(12);
    nothing_new.set_margin_bottom(12);
    nothing_new.set_visible(false);
    let separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
    separator.add_css_class("new-release-separator");
    let (releases_jump, releases_jump_label) = build_jump_row();
    let (concerts_jump, concerts_jump_label) = build_jump_row();

    let (updates_header, fetch_button, fetch_stack, spinner, updated) = build_header();
    let failure = gtk4::Label::new(None);
    failure.add_css_class("dim-label");
    failure.set_xalign(0.0);
    failure.set_wrap(true);
    failure.set_visible(false);
    let list_page = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    list_page.append(&updates_header);
    list_page.append(&failure);
    list_page.append(&news_section);
    list_page.append(concerts_section.root());
    list_page.append(&nothing_new);
    list_page.append(&separator);
    list_page.append(&releases_jump);
    list_page.append(&concerts_jump);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.set_size_request(POPOVER_WIDTH, -1);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    content.set_margin_start(10);
    content.set_margin_end(10);
    content.append(&list_page);
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
        nothing_new,
        new_tag,
        releases_jump,
        releases_jump_label,
        concerts_jump,
        concerts_jump_label,
        fetch_button,
        fetch_stack,
        spinner,
        updated,
        failure,
    }
}

fn build_jump_row() -> (gtk4::Button, gtk4::Label) {
    let label = gtk4::Label::new(None);
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.add_css_class("new-release-history-label");
    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    content.append(&label);
    let button = gtk4::Button::builder()
        .child(&content)
        .css_classes(["flat", "new-release-history-row"])
        .build();
    (button, label)
}

fn build_header() -> (
    gtk4::Box,
    gtk4::Button,
    gtk4::Stack,
    gtk4::Spinner,
    gtk4::Label,
) {
    let title = gtk4::Label::new(Some(&strings::text(strings::UPDATES_HEADER)));
    title.add_css_class("new-release-header");
    title.set_xalign(0.0);
    title.set_hexpand(true);
    let icon = gtk4::Image::from_icon_name("view-refresh-symbolic");
    let spinner = gtk4::Spinner::new();
    let stack = gtk4::Stack::new();
    stack.add_named(&icon, Some("icon"));
    stack.add_named(&spinner, Some("spinner"));
    stack.set_visible_child_name("icon");
    let fetch_button = gtk4::Button::builder()
        .child(&stack)
        .tooltip_text(strings::text(strings::FETCH_NOW))
        .css_classes(["flat", "new-release-ghost"])
        .build();
    let updated = gtk4::Label::new(None);
    updated.add_css_class("dim-label");
    updated.set_halign(gtk4::Align::End);
    let fetch_status = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    fetch_status.append(&fetch_button);
    fetch_status.append(&updated);
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    header.append(&title);
    header.append(&fetch_status);
    (header, fetch_button, stack, spinner, updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nr_23_shell_is_a_fixed_delta_layout_with_fetch_state_in_its_header() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        if gtk4::init().is_err() {
            return;
        }
        let shell = build();
        let content = shell.popover.child().expect("popover content");
        let list_page = content.first_child().expect("list page");
        let header = list_page.first_child().expect("updates header");

        assert_eq!(content.width_request(), 380);
        assert!(shell
            .list
            .ancestor(gtk4::ScrolledWindow::static_type())
            .is_none());
        assert_eq!(
            shell.list.parent(),
            Some(shell.news_section.clone().upcast())
        );
        assert_eq!(shell.failure.parent(), Some(list_page.clone()));
        assert_eq!(shell.fetch_button.parent(), shell.updated.parent());
        assert_eq!(
            shell
                .fetch_button
                .parent()
                .and_then(|parent| parent.parent()),
            Some(header)
        );
    }
}
