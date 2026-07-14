//! Full-window Library content with a movable player bar.

use gtk4::prelude::*;
use reprise_core::library::settings::PlayerBarPosition;

#[derive(Clone)]
pub(super) struct LibraryPlayerBarShell {
    root: gtk4::Box,
    bar_block: gtk4::Box,
}

impl LibraryPlayerBarShell {
    pub(super) fn new(
        content: &impl IsA<gtk4::Widget>,
        player_bar: Option<&gtk4::Widget>,
        position: PlayerBarPosition,
    ) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.set_hexpand(true);
        root.set_vexpand(true);
        content.set_hexpand(true);
        content.set_vexpand(true);
        root.append(content);

        let bar_block = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        bar_block.set_hexpand(true);
        if let Some(player_bar) = player_bar {
            bar_block.append(player_bar);
        }

        let shell = Self { root, bar_block };
        shell.set_position(position);
        shell
    }

    pub(super) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(super) fn set_position(&self, position: PlayerBarPosition) {
        if self.bar_block.parent().is_some() {
            self.root.remove(&self.bar_block);
        }
        match position {
            PlayerBarPosition::Top => self.root.prepend(&self.bar_block),
            PlayerBarPosition::Bottom => self.root.append(&self.bar_block),
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
    fn bar_spans_the_full_root_at_top_and_bottom() {
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

        assert_eq!(shell.widget().first_child(), Some(content.clone().upcast()));
        assert_eq!(
            shell.widget().last_child(),
            Some(shell.bar_block.clone().upcast())
        );
        assert_eq!(shell.bar_block.width(), shell.widget().width());

        shell.set_position(PlayerBarPosition::Top);
        wait_for_layout();
        assert_eq!(
            shell.widget().first_child(),
            Some(shell.bar_block.clone().upcast())
        );
        assert_eq!(shell.widget().last_child(), Some(content.clone().upcast()));
        assert_eq!(shell.bar_block.width(), shell.widget().width());

        shell.set_position(PlayerBarPosition::Bottom);
        shell.set_position(PlayerBarPosition::Bottom);
        assert_eq!(
            shell.bar_block.parent(),
            Some(shell.widget().clone().upcast())
        );
        assert_eq!(
            shell.widget().last_child(),
            Some(shell.bar_block.clone().upcast())
        );
        window.close();
    }
}
