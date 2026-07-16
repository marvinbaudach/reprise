use gtk4::prelude::*;
use libadwaita as adw;

pub(in crate::ui) const PANEL_WIDTH: i32 = 340;

#[derive(Clone)]
pub(in crate::ui) struct InformationColumn {
    split: adw::OverlaySplitView,
}

impl InformationColumn {
    #[allow(clippy::needless_pass_by_value)]
    pub(in crate::ui) fn new(
        content: &impl IsA<gtk4::Widget>,
        sidebar: &adw::ToolbarView,
        visible: bool,
    ) -> Self {
        sidebar.set_width_request(PANEL_WIDTH);

        let split = adw::OverlaySplitView::builder()
            .content(content)
            .sidebar(sidebar)
            .sidebar_position(gtk4::PackType::End)
            .show_sidebar(visible)
            .collapsed(false)
            .min_sidebar_width(f64::from(PANEL_WIDTH))
            .max_sidebar_width(f64::from(PANEL_WIDTH))
            .build();

        Self { split }
    }

    pub(in crate::ui) fn widget(&self) -> &adw::OverlaySplitView {
        &self.split
    }

    #[cfg(test)]
    pub(in crate::ui) fn sidebar_widget(&self) -> adw::ToolbarView {
        self.split
            .sidebar()
            .and_downcast::<adw::ToolbarView>()
            .expect("sidebar is a ToolbarView")
    }

    #[cfg(test)]
    pub(in crate::ui) fn is_visible(&self) -> bool {
        self.split.shows_sidebar()
    }

    pub(in crate::ui) fn set_visible(&self, visible: bool) {
        self.split.set_show_sidebar(visible);
    }
}
