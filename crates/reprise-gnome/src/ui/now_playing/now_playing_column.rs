use gtk4::prelude::*;
use libadwaita as adw;

pub(in crate::ui) const PANEL_WIDTH: i32 = 300;

#[derive(Clone)]
pub(in crate::ui) struct NowPlayingColumn {
    split: adw::OverlaySplitView,
}

impl NowPlayingColumn {
    #[allow(clippy::needless_pass_by_value)]
    pub(in crate::ui) fn new(
        content: &impl IsA<gtk4::Widget>,
        sidebar: &adw::ToolbarView,
        visible: bool,
    ) -> Self {
        sidebar.set_width_request(PANEL_WIDTH);

        // Version-robust clip: `AdwOverlaySplitView` positions its content pane
        // with GPU transforms and does not clip it, so a content whose minimum
        // width exceeds the (window − sidebar) slot paints under the sidebar
        // (the track table's right columns "run over" — QA #3). The app CSS
        // clips libadwaita's internal wrapper node, but that selector is tied
        // to adwaita-internal node names; clipping our own content widget here
        // does not depend on them and survives a libadwaita restructure.
        content.set_overflow(gtk4::Overflow::Hidden);

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
