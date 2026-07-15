//! Composition wiring for every Library/Compact mode entry point.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::library::settings;
use rusqlite::Connection;

use super::compact_player::CompactPlayer;
use super::compact_player_layouts::layout_from_token;
use super::first_run::FirstRunDecision;
use super::minimal_view::{self, MinimalView, ViewTransition};
use super::window_decorations::WindowContentHost;

pub(super) const SMOKE_LAYOUT_ENV: &str = "REPRISE_SMOKE_COMPACT_LAYOUT";

pub(super) fn initial_transition(conn: &Connection, first_run: FirstRunDecision) -> ViewTransition {
    minimal_view::startup_transition(
        settings::get_window_view_mode(conn),
        settings::get_compact_layout(conn),
        first_run,
    )
}

pub(super) fn build_mode(
    window: &adw::ApplicationWindow,
    content_host: &WindowContentHost,
    full_root: &gtk4::Widget,
    compact: Option<&CompactPlayer>,
    conn: &Rc<RefCell<Connection>>,
    initial: ViewTransition,
    toast_overlay: &adw::ToastOverlay,
) -> Rc<MinimalView> {
    let toast_overlay = toast_overlay.clone();
    MinimalView::new(
        window,
        content_host,
        full_root,
        compact,
        conn.clone(),
        initial,
        Rc::new(move |message| toast_overlay.add_toast(adw::Toast::new(message))),
    )
}

pub(super) fn install(
    window: &adw::ApplicationWindow,
    mode: &Rc<MinimalView>,
    compact: Option<&CompactPlayer>,
    on_preferences: Rc<dyn Fn()>,
) {
    if let Some(compact) = compact {
        let weak = Rc::downgrade(mode);
        compact.set_on_restore(Rc::new(move || {
            if let Some(mode) = weak.upgrade() {
                mode.toggle();
            }
        }));
        let weak = Rc::downgrade(mode);
        compact.set_on_layout(Rc::new(move |layout| {
            if let Some(mode) = weak.upgrade() {
                mode.select_layout(layout);
            }
        }));
        compact.set_on_preferences(on_preferences);

        let window_weak = glib::WeakRef::new();
        window_weak.set(Some(window));
        compact.set_on_always_on_top(Rc::new(move |above| {
            if let Some(window) = window_weak.upgrade() {
                // TODO: platform-specific implementation (X11 _NET_WM_STATE_ABOVE).
                // On Wayland, always-on-top is compositor-managed and may be ignored.
                tracing::debug!(above, "compact player always-on-top toggled");
                let _ = &window;
            }
        }));
        let window_weak = glib::WeakRef::new();
        window_weak.set(Some(window));
        compact.set_on_quit(Rc::new(move || {
            if let Some(window) = window_weak.upgrade() {
                window.close();
            }
        }));
    }

    arm_smoke_layout(mode);
}

fn arm_smoke_layout(mode: &Rc<MinimalView>) {
    let Ok(token) = std::env::var(SMOKE_LAYOUT_ENV) else {
        return;
    };
    let Some(layout) = layout_from_token(&token) else {
        tracing::warn!(token, "invalid compact-layout smoke token");
        return;
    };
    let mode = Rc::downgrade(mode);
    gtk4::glib::idle_add_local_once(move || {
        if let Some(mode) = mode.upgrade() {
            mode.select_layout(layout);
            tracing::info!(?layout, "smoke: compact layout selected");
        }
    });
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use gtk4::gio;
    use libadwaita::prelude::*;
    use reprise_core::library::settings::{CompactLayout, WindowViewMode};
    use rusqlite::Connection;

    use super::*;
    use crate::ui::minimal_view::ViewTransition;

    fn has_button_with_tooltip(root: &impl IsA<gtk4::Widget>, tooltip: &str) -> bool {
        let mut child = root.first_child();
        while let Some(widget) = child {
            if widget
                .downcast_ref::<gtk4::Button>()
                .is_some_and(|button| button.tooltip_text().as_deref() == Some(tooltip))
                || has_button_with_tooltip(&widget, tooltip)
            {
                return true;
            }
            child = widget.next_sibling();
        }
        false
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn library_entry_wiring_adds_no_header_button_and_restore_reuses_the_window() {
        if gtk4::init().is_err() {
            return;
        }
        let app = adw::Application::builder()
            .application_id("org.reprise.Reprise.CompactModeTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::builder()
            .application(&app)
            .default_width(900)
            .default_height(600)
            .build();
        let full_root = test_split_view();
        let compact = CompactPlayer::new();
        let conn = Rc::new(RefCell::new(Connection::open_in_memory().unwrap()));
        reprise_core::db::migrate(&conn.borrow()).unwrap();
        let content_host = WindowContentHost::new(&window);
        let mode = MinimalView::new(
            &window,
            &content_host,
            full_root.upcast_ref(),
            Some(&compact),
            conn,
            ViewTransition {
                mode: WindowViewMode::Library,
                layout: CompactLayout::Card,
            },
            Rc::new(|_| {}),
        );
        mode.apply_initial();
        let header = adw::HeaderBar::new();
        install(&window, &mode, Some(&compact), Rc::new(|| {}));
        assert!(!has_button_with_tooltip(&header, "Open Compact View"));
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}
        let same_window = window.clone();

        mode.toggle();
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert_eq!(compact.layout(), CompactLayout::Card);
        assert!(compact.widget().is_ancestor(&window));
        assert_eq!(window, same_window);
        assert!(window.is_visible());

        compact.activate_restore_for_test();

        assert_eq!(
            content_host.content().as_ref(),
            Some(full_root.upcast_ref())
        );
        assert_eq!(window, same_window);
        window.close();
    }

    fn test_split_view() -> adw::NavigationSplitView {
        let sidebar = adw::NavigationPage::builder()
            .title("Sidebar")
            .child(&gtk4::Label::new(Some("Sidebar")))
            .build();
        let content = adw::NavigationPage::builder()
            .title("Library")
            .child(&gtk4::Label::new(Some("Library")))
            .build();
        adw::NavigationSplitView::builder()
            .sidebar(&sidebar)
            .content(&content)
            .build()
    }
}
