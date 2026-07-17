//! Shared issue-card shell.

use gtk4::prelude::*;

/// A grouped issue surface with a fixed header and selectable row body.
pub(in crate::ui) struct IssueCard {
    root: gtk4::Box,
    body: gtk4::ListBox,
}

impl IssueCard {
    /// Builds a card header and an empty body ready for `IssueRow`s.
    ///
    /// `GtkListBox` owns Ctrl/Shift range selection and Ctrl+A in
    /// `SelectionMode::Multiple`; consumers only need to observe
    /// `selected-rows-changed` when they project selection into actions.
    pub(in crate::ui) fn new(
        icon: &str,
        title: &str,
        meta: &str,
        header_action: Option<gtk4::Widget>,
    ) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.add_css_class("issue-card");

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        header.add_css_class("issue-card-header");

        let icon = gtk4::Label::new(Some(icon));
        icon.add_css_class("issue-card-icon");
        header.append(&icon);

        let title = gtk4::Label::new(Some(title));
        title.set_xalign(0.0);
        title.add_css_class("issue-card-title");
        header.append(&title);

        let meta = gtk4::Label::new(Some(meta));
        meta.set_xalign(0.0);
        meta.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        meta.set_hexpand(true);
        meta.add_css_class("issue-card-meta");
        header.append(&meta);

        if let Some(action) = header_action {
            header.append(&action);
        }
        root.append(&header);

        let body = gtk4::ListBox::new();
        body.set_selection_mode(gtk4::SelectionMode::Multiple);
        body.set_activate_on_single_click(false);
        body.add_css_class("issue-card-list");
        root.append(&body);

        Self { root, body }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(in crate::ui) fn body_listbox(&self) -> &gtk4::ListBox {
        &self.body
    }
}
