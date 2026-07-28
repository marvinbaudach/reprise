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
        // The panel's cover, metadata, tabs, and footer have a useful tall
        // natural size. They must not become the whole split view's minimum
        // height, though: in a short main window that would force the
        // structural player bar below the client area. Scroll the panel as
        // one surface when vertical space is constrained.
        let sidebar_viewport = gtk4::ScrolledWindow::builder()
            .width_request(PANEL_WIDTH)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .child(sidebar)
            .build();

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
            .sidebar(&sidebar_viewport)
            .sidebar_position(gtk4::PackType::End)
            .show_sidebar(visible)
            .collapsed(false)
            .min_sidebar_width(f64::from(PANEL_WIDTH))
            .max_sidebar_width(f64::from(PANEL_WIDTH))
            .sidebar_width_unit(adw::LengthUnit::Px)
            .build();

        Self { split }
    }

    pub(in crate::ui) fn widget(&self) -> &adw::OverlaySplitView {
        &self.split
    }

    pub(in crate::ui) fn is_visible(&self) -> bool {
        self.split.shows_sidebar()
    }

    pub(in crate::ui) fn set_visible(&self, visible: bool) {
        self.split.set_show_sidebar(visible);
    }
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;
    use reprise_core::library::settings::PlayerBarPosition;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn style_5_info_panel_surrenders_height_before_the_player_bar() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();

        let content = gtk4::Label::new(Some("Library"));
        let tall_panel_content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        tall_panel_content.set_height_request(410);
        let panel = libadwaita::ToolbarView::new();
        panel.set_content(Some(&tall_panel_content));
        let column = super::NowPlayingColumn::new(&content, &panel, false);
        let player = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        player.set_height_request(86);
        let shell = crate::ui::library_player_bar::LibraryPlayerBarShell::new(
            column.widget(),
            Some(player.upcast_ref()),
            PlayerBarPosition::Bottom,
        );
        let window = gtk4::Window::builder()
            .default_width(1_200)
            .default_height(420)
            .child(shell.widget())
            .build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        let player_bounds = player
            .compute_bounds(&window)
            .expect("player must share the window coordinate space");
        assert!(
            window.height() <= 420
                && player_bounds.y() + player_bounds.height() <= window.height() as f32,
            "the info panel forced the short window to {} px or pushed the player below it: {player_bounds:?}",
            window.height()
        );
        window.close();
    }
}
