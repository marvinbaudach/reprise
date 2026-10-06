//! Section headings of the navigation sidebar and the row hosting the
//! new-playlist action beside the PLAYLISTS heading.

use gtk4::prelude::*;

use super::sidebar_presentation::{ROW_HORIZONTAL_MARGIN, ROW_SPACING, SIDEBAR_TEXT_INSET};
use crate::ui::strings;

fn section_header_label(text: &str) -> gtk4::Label {
    let label = gtk4::Label::builder()
        .label(text)
        .xalign(0.0)
        .accessible_role(gtk4::AccessibleRole::Heading)
        .build();
    label.add_css_class("caption-heading");
    label.add_css_class("reprise-text-secondary");
    label.set_margin_top(14);
    label.set_margin_bottom(4);
    label
}

fn navigation_header_label(text: &str) -> gtk4::Label {
    let label = section_header_label(text);
    label.set_margin_start(ROW_HORIZONTAL_MARGIN);
    label.set_margin_end(ROW_HORIZONTAL_MARGIN);
    label
}

fn standalone_header_label(text: &str) -> gtk4::Label {
    let label = section_header_label(text);
    label.set_margin_start(SIDEBAR_TEXT_INSET);
    label.set_margin_end(SIDEBAR_TEXT_INSET);
    label
}

/// Header rows are `Generic`, never `Presentation`: GTK's AT-SPI backend drops
/// a presentational widget together with its subtree, which hid the section
/// heading and the new-playlist button from assistive technology (NAV-11).
pub(in crate::ui) fn append_header(listbox: &gtk4::ListBox, text: &str) -> gtk4::ListBoxRow {
    let label = navigation_header_label(text);
    let row = gtk4::ListBoxRow::builder()
        .child(&label)
        .selectable(false)
        .activatable(false)
        .focusable(false)
        .accessible_role(gtk4::AccessibleRole::Generic)
        .build();
    listbox.append(&row);
    row
}

pub(in crate::ui) fn append_header_with_action(
    listbox: &gtk4::ListBox,
    text: &str,
    action_name: &str,
    on_activate: impl Fn() + 'static,
) -> gtk4::Button {
    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, ROW_SPACING);
    let label = navigation_header_label(text);
    label.set_hexpand(true);
    hbox.append(&label);

    let button = gtk4::Button::from_icon_name("list-add-symbolic");
    button.add_css_class("flat");
    button.set_tooltip_text(Some(action_name));
    button.update_property(&[gtk4::accessible::Property::Label(action_name)]);
    // a11y-semantics: role=button name=new-playlist state=focusable action=activate
    button.set_focusable(true);
    button.connect_clicked(move |_| on_activate());
    hbox.append(&button);

    let row = gtk4::ListBoxRow::builder()
        .child(&hbox)
        .selectable(false)
        .activatable(false)
        .focusable(false)
        .accessible_role(gtk4::AccessibleRole::Generic)
        .build();
    listbox.append(&row);
    button
}

pub(in crate::ui) fn problem_header() -> gtk4::Label {
    standalone_header_label(&strings::text(strings::SIDEBAR_SECTION_ISSUES))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn problem_sources_use_a_labeled_section_header() {
        gtk4::init().unwrap();
        let label = problem_header();

        assert_eq!(label.text(), "ISSUES");
        assert!(label.has_css_class("caption-heading"));
        assert!(label.has_css_class("reprise-text-secondary"));
        assert_eq!(label.accessible_role(), gtk4::AccessibleRole::Heading);
    }
}
