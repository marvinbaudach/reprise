//! Full-window Library glass shell.
//!
//! Library content remains the full-size main child. Header, revealed search,
//! and the movable player bar are clipped glass overlays; allocated overlay
//! heights become scroll-end insets so the first and last rows stay reachable.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::library::settings::PlayerBarPosition;

use crate::ui::glass::{GlassEdge, GlassSurface, SafeInsetApplier, SafeInsets};

#[derive(Clone)]
pub(in crate::ui) struct LibraryPlayerBarShell {
    root: gtk4::Overlay,
    top_controls: gtk4::Box,
    bottom_controls: gtk4::Box,
    bar_box: gtk4::Box,
    bottom_surface: gtk4::Overlay,
    position: Rc<Cell<PlayerBarPosition>>,
    has_player: bool,
    insets: Rc<Cell<SafeInsets>>,
    inset_applier: Rc<SafeInsetApplier>,
}

impl LibraryPlayerBarShell {
    pub(in crate::ui) fn new(
        content: &impl IsA<gtk4::Widget>,
        top_controls: &gtk4::Box,
        player_bar: Option<&gtk4::Widget>,
        position: PlayerBarPosition,
    ) -> Self {
        let initial_position = position;
        let root = gtk4::Overlay::new();
        content.set_hexpand(true);
        content.set_vexpand(true);
        root.set_child(Some(content));

        let bar_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        bar_box.set_hexpand(true);
        if let Some(player_bar) = player_bar {
            bar_box.append(player_bar);
        }
        let has_player = player_bar.is_some();

        let bottom_controls = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        bottom_controls.set_hexpand(true);

        let top_glass = GlassSurface::new(content, top_controls, GlassEdge::Bottom);
        let top_surface = top_glass.root().clone();
        top_surface.set_halign(gtk4::Align::Fill);
        top_surface.set_valign(gtk4::Align::Start);
        root.add_overlay(&top_surface);

        let bottom_glass = GlassSurface::new(content, &bottom_controls, GlassEdge::Top);
        let bottom_surface = bottom_glass.root().clone();
        bottom_surface.set_halign(gtk4::Align::Fill);
        bottom_surface.set_valign(gtk4::Align::End);
        root.add_overlay(&bottom_surface);

        let insets = Rc::new(Cell::new(SafeInsets::default()));
        let inset_applier = Rc::new(SafeInsetApplier::discover(content));
        {
            let insets = insets.clone();
            let inset_applier = inset_applier.clone();
            top_glass.set_on_allocate(Rc::new(move |_, height| {
                let updated = SafeInsets {
                    top: height.max(0),
                    ..insets.get()
                };
                insets.set(updated);
                inset_applier.apply(updated);
            }));
        }
        {
            let bottom_insets = insets.clone();
            let bottom_inset_applier = inset_applier.clone();
            let position = Rc::new(Cell::new(position));
            let position_for_shell = position.clone();
            bottom_glass.set_on_allocate(Rc::new(move |_, height| {
                let bottom = match position.get() {
                    PlayerBarPosition::Top => 0,
                    PlayerBarPosition::Bottom => height.max(0),
                };
                let updated = SafeInsets {
                    bottom,
                    ..bottom_insets.get()
                };
                bottom_insets.set(updated);
                bottom_inset_applier.apply(updated);
            }));

            let shell = Self {
                root,
                top_controls: top_controls.clone(),
                bottom_controls,
                bar_box,
                bottom_surface,
                position: position_for_shell,
                has_player,
                insets,
                inset_applier,
            };
            shell.set_position(initial_position);
            shell
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Overlay {
        &self.root
    }

    pub(in crate::ui) fn set_position(&self, position: PlayerBarPosition) {
        if let Some(parent) = self.bar_box.parent().and_downcast::<gtk4::Box>() {
            parent.remove(&self.bar_box);
        }
        match position {
            PlayerBarPosition::Top => {
                self.top_controls.append(&self.bar_box);
                self.bottom_surface.set_visible(false);
            }
            PlayerBarPosition::Bottom => {
                self.bottom_controls.append(&self.bar_box);
                self.bottom_surface.set_visible(self.has_player);
            }
        }
        self.position.set(position);
        let updated = SafeInsets {
            bottom: 0,
            ..self.insets.get()
        };
        self.insets.set(updated);
        self.inset_applier.apply(updated);
        self.root.queue_allocate();
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
    fn play_7a_player_bar_is_a_global_overlay_at_bottom_and_top() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        if gtk4::init().is_err() {
            return;
        }
        let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        rows.append(&gtk4::Label::new(Some("First")));
        rows.append(&gtk4::Label::new(Some("Last")));
        let content = gtk4::ScrolledWindow::builder().child(&rows).build();
        content.set_hexpand(true);
        content.set_vexpand(true);
        let top_controls = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        top_controls.append(&gtk4::Label::new(Some("Header")));
        let player = gtk4::ActionBar::new();
        player.set_center_widget(Some(&gtk4::Label::new(Some("Player"))));
        let shell = LibraryPlayerBarShell::new(
            &content,
            &top_controls,
            Some(player.upcast_ref()),
            PlayerBarPosition::Bottom,
        );
        let _: &gtk4::Overlay = shell.widget();
        let window = gtk4::Window::builder()
            .default_width(1_000)
            .default_height(600)
            .child(shell.widget())
            .build();
        window.present();
        wait_for_layout();

        assert!(shell.widget().is::<gtk4::Overlay>());
        // The content remains the full-size main child while the bar is an
        // overlay. Safe scroll insets, rather than structural height, keep
        // both ends reachable.
        assert_eq!(shell.widget().child().as_ref(), Some(content.upcast_ref()));
        assert!(top_controls.is_ancestor(shell.widget()));
        assert!(shell.bottom_controls.is_ancestor(shell.widget()));
        assert_eq!(
            shell.bar_box.parent(),
            Some(shell.bottom_controls.clone().upcast())
        );
        assert!(shell.bar_box.height() > 0);
        assert_eq!(content.height(), shell.widget().height());
        assert_eq!(shell.bar_box.width(), shell.widget().width());
        assert_eq!(rows.margin_top(), top_controls.height());
        assert_eq!(rows.margin_bottom(), shell.bottom_surface.height());

        shell.set_position(PlayerBarPosition::Top);
        wait_for_layout();
        assert_eq!(shell.bar_box.parent(), Some(top_controls.clone().upcast()));
        assert_eq!(rows.margin_top(), top_controls.height());
        assert_eq!(rows.margin_bottom(), 0);

        shell.set_position(PlayerBarPosition::Bottom);
        assert_eq!(
            shell.bar_box.parent(),
            Some(shell.bottom_controls.clone().upcast())
        );
        // Calling set_position twice is a no-op: the bar remains in the
        // bottom overlay controls.
        shell.set_position(PlayerBarPosition::Bottom);
        assert_eq!(
            shell.bar_box.parent().as_ref(),
            Some(shell.bottom_controls.upcast_ref::<gtk4::Widget>())
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
        let top_controls = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        top_controls.append(&gtk4::Label::new(Some("Header")));
        let player = gtk4::ActionBar::new();
        let shell = LibraryPlayerBarShell::new(
            &content,
            &top_controls,
            Some(player.upcast_ref()),
            PlayerBarPosition::Top,
        );

        assert_eq!(shell.bar_box.parent(), Some(top_controls.clone().upcast()));
        assert_eq!(shell.widget().child(), Some(content.clone().upcast()));
    }
}
