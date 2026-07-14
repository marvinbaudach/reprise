use gtk4::prelude::*;
use libadwaita as adw;

pub(super) const PANEL_WIDTH: i32 = 340;

#[derive(Clone)]
pub(super) struct InformationColumn {
    root: gtk4::Box,
    sidebar: adw::ToolbarView,
}

impl InformationColumn {
    pub(super) fn new(
        content: &impl IsA<gtk4::Widget>,
        sidebar: adw::ToolbarView,
        visible: bool,
    ) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        root.set_hexpand(true);
        root.set_vexpand(true);

        content.set_hexpand(true);
        content.set_vexpand(true);
        sidebar.set_width_request(PANEL_WIDTH);
        sidebar.set_hexpand(false);
        sidebar.set_vexpand(true);
        sidebar.set_visible(visible);

        root.append(content);
        root.append(&sidebar);

        Self { root, sidebar }
    }

    pub(super) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(super) fn sidebar_widget(&self) -> &adw::ToolbarView {
        &self.sidebar
    }

    #[cfg(test)]
    pub(super) fn is_visible(&self) -> bool {
        self.sidebar.is_visible()
    }

    pub(super) fn set_visible(&self, visible: bool) {
        self.sidebar.set_visible(visible);
    }
}
