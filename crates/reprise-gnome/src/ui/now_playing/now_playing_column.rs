use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

pub(in crate::ui) const PANEL_WIDTH: i32 = 300;
pub(in crate::ui) const INFO_PANEL_COLLAPSE_WIDTH: i32 =
    crate::ui::window::library_shell::SIDEBAR_BREAKPOINT_WIDTH - 1;

#[derive(Clone)]
pub(in crate::ui) struct NowPlayingColumn {
    root: adw::BreakpointBin,
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
            .pin_sidebar(true)
            .min_sidebar_width(f64::from(PANEL_WIDTH))
            .max_sidebar_width(f64::from(PANEL_WIDTH))
            .sidebar_width_unit(adw::LengthUnit::Px)
            .build();

        // This bin is allocated inside the library split's content pane. It
        // deliberately uses the same 799 px bin as the library sidebar rather
        // than subtracting the pinned sidebar width: below 800 the library
        // sidebar overlays and the content receives the full window width, so
        // the mapping from window width to this allocation is ambiguous. The
        // derived mutual-exclusion constraint makes the threshold correct while
        // the info panel is open. When the hidden panel lands in the ambiguous
        // band (for example 784 px at a 1024 px window), pin-sidebar keeps the
        // breakpoint from changing its visibility behind the constraint owner.
        let condition = adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            f64::from(INFO_PANEL_COLLAPSE_WIDTH),
            adw::LengthUnit::Px,
        );
        let breakpoint = adw::Breakpoint::new(condition);
        breakpoint.add_setter(&split, "collapsed", Some(&true.to_value()));
        let root = adw::BreakpointBin::new();
        root.set_size_request(1, 1);
        root.set_child(Some(&split));
        root.add_breakpoint(breakpoint);

        Self { root, split }
    }

    pub(in crate::ui) fn root(&self) -> &adw::BreakpointBin {
        &self.root
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
            column.root(),
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
