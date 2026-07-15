//! Full-window Library content with a movable player bar overlaid via
//! `GtkOverlay` so the track list scrolls behind the translucent bar.

use gtk4::prelude::*;
use reprise_core::library::settings::PlayerBarPosition;

#[derive(Clone)]
pub(super) struct LibraryPlayerBarShell {
    overlay: gtk4::Overlay,
    bar_box: gtk4::Box,
}

impl LibraryPlayerBarShell {
    pub(super) fn new(
        content: &impl IsA<gtk4::Widget>,
        player_bar: Option<&gtk4::Widget>,
        _position: PlayerBarPosition,
    ) -> Self {
        let overlay = gtk4::Overlay::new();
        content.set_hexpand(true);
        content.set_vexpand(true);
        overlay.set_child(Some(content));

        let bar_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        bar_box.set_hexpand(true);
        bar_box.set_valign(gtk4::Align::End);
        if let Some(player_bar) = player_bar {
            bar_box.append(player_bar);
        }
        overlay.add_overlay(&bar_box);

        Self { overlay, bar_box }
    }

    pub(super) fn widget(&self) -> &gtk4::Overlay {
        &self.overlay
    }

    pub(super) fn set_position(&self, position: PlayerBarPosition) {
        self.bar_box.set_valign(match position {
            PlayerBarPosition::Top => gtk4::Align::Start,
            PlayerBarPosition::Bottom => gtk4::Align::End,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gtk4::prelude::*;
    use reprise_core::library::settings::PlayerBarPosition;

    use super::LibraryPlayerBarShell;

    fn wait_for_layout() {
        let main_loop = gtk4::glib::MainLoop::new(None, false);
        let quit = main_loop.clone();
        gtk4::glib::timeout_add_local_once(Duration::from_millis(50), move || quit.quit());
        main_loop.run();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn bar_overlay_aligns_to_bottom_and_top() {
        if gtk4::init().is_err() {
            return;
        }
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.set_hexpand(true);
        content.set_vexpand(true);
        let player = gtk4::ActionBar::new();
        player.set_center_widget(Some(&gtk4::Label::new(Some("Player"))));
        let shell = LibraryPlayerBarShell::new(
            &content,
            Some(player.upcast_ref()),
            PlayerBarPosition::Bottom,
        );
        let window = gtk4::Window::builder()
            .default_width(1_000)
            .default_height(600)
            .child(shell.widget())
            .build();
        window.present();
        wait_for_layout();

        // Content is the overlay's main child.
        assert_eq!(
            shell.widget().child().as_ref(),
            Some(content.upcast_ref::<gtk4::Widget>())
        );
        // bar_box is an overlay widget pinned to the bottom.
        assert_eq!(shell.bar_box.valign(), gtk4::Align::End);
        assert_eq!(shell.bar_box.width(), shell.widget().width());

        shell.set_position(PlayerBarPosition::Top);
        wait_for_layout();
        assert_eq!(shell.bar_box.valign(), gtk4::Align::Start);

        shell.set_position(PlayerBarPosition::Bottom);
        assert_eq!(shell.bar_box.valign(), gtk4::Align::End);
        // Calling set_position twice is a no-op: bar_box stays in the overlay.
        shell.set_position(PlayerBarPosition::Bottom);
        assert_eq!(shell.bar_box.parent().as_ref(), Some(shell.widget().upcast_ref::<gtk4::Widget>()));

        window.close();
    }
}
