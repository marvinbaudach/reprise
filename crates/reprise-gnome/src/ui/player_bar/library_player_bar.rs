//! Full-window Library content with a movable player bar as a structural
//! edge: content and bar are siblings in a vertical `GtkBox`, so the bar
//! reserves its own height and nothing (track list, sidebar, info panel)
//! ever extends behind it — the bar is a hard boundary, not an overlay.

use gtk4::prelude::*;
use reprise_core::library::settings::PlayerBarPosition;

#[derive(Clone)]
pub(in crate::ui) struct LibraryPlayerBarShell {
    root: gtk4::Box,
    bar_box: gtk4::Box,
}

impl LibraryPlayerBarShell {
    pub(in crate::ui) fn new(
        content: &impl IsA<gtk4::Widget>,
        player_bar: Option<&gtk4::Widget>,
        position: PlayerBarPosition,
    ) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.set_hexpand(true);
        content.set_vexpand(true);

        let bar_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        bar_box.set_hexpand(true);
        if let Some(player_bar) = player_bar {
            bar_box.append(player_bar);
        }

        root.append(content);
        root.append(&bar_box);

        let shell = Self { root, bar_box };
        shell.set_position(position);
        shell
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(in crate::ui) fn set_position(&self, position: PlayerBarPosition) {
        match position {
            PlayerBarPosition::Top => {
                self.root
                    .reorder_child_after(&self.bar_box, gtk4::Widget::NONE);
            }
            PlayerBarPosition::Bottom => {
                let last = self.root.last_child();
                if last.as_ref() != Some(self.bar_box.upcast_ref()) {
                    self.root.reorder_child_after(&self.bar_box, last.as_ref());
                }
            }
        }
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
    fn bar_is_a_structural_edge_at_bottom_and_top() {
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

        // Content and bar are siblings in a vertical box: the bar sits below
        // the content and claims its own height, so the content region ends
        // where the bar begins — nothing can slide underneath it.
        assert_eq!(
            shell.widget().first_child().as_ref(),
            Some(content.upcast_ref::<gtk4::Widget>())
        );
        assert_eq!(
            shell.widget().last_child().as_ref(),
            Some(shell.bar_box.upcast_ref::<gtk4::Widget>())
        );
        assert!(shell.bar_box.height() > 0);
        assert_eq!(
            content.height() + shell.bar_box.height(),
            shell.widget().height()
        );
        assert_eq!(shell.bar_box.width(), shell.widget().width());

        shell.set_position(PlayerBarPosition::Top);
        wait_for_layout();
        assert_eq!(
            shell.widget().first_child().as_ref(),
            Some(shell.bar_box.upcast_ref::<gtk4::Widget>())
        );

        shell.set_position(PlayerBarPosition::Bottom);
        assert_eq!(
            shell.widget().last_child().as_ref(),
            Some(shell.bar_box.upcast_ref::<gtk4::Widget>())
        );
        // Calling set_position twice is a no-op: bar_box stays in the box.
        shell.set_position(PlayerBarPosition::Bottom);
        assert_eq!(
            shell.bar_box.parent().as_ref(),
            Some(shell.widget().upcast_ref::<gtk4::Widget>())
        );

        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn persisted_top_position_is_applied_at_construction() {
        if gtk4::init().is_err() {
            return;
        }
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let player = gtk4::ActionBar::new();
        let shell =
            LibraryPlayerBarShell::new(&content, Some(player.upcast_ref()), PlayerBarPosition::Top);

        assert_eq!(
            shell.widget().first_child().as_ref(),
            Some(shell.bar_box.upcast_ref::<gtk4::Widget>())
        );
        assert_eq!(
            shell.widget().last_child().as_ref(),
            Some(content.upcast_ref::<gtk4::Widget>())
        );
    }
}
