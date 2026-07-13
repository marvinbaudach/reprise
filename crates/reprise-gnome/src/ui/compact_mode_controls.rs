//! Composition wiring for every Library/Compact mode entry point.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::library::settings;
use rusqlite::Connection;

use super::compact_player::CompactPlayer;
use super::compact_player_layouts::layout_from_token;
use super::first_run::FirstRunDecision;
use super::minimal_view::{self, MinimalView, ViewTransition};
use super::strings;

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
    full_root: &gtk4::Widget,
    compact: Option<&CompactPlayer>,
    conn: &Rc<RefCell<Connection>>,
    initial: ViewTransition,
    toast_overlay: &adw::ToastOverlay,
) -> Rc<MinimalView> {
    let toast_overlay = toast_overlay.clone();
    MinimalView::new(
        window,
        full_root,
        compact,
        conn.clone(),
        initial,
        Rc::new(move |message| toast_overlay.add_toast(adw::Toast::new(message))),
    )
}

pub(super) fn install(
    header: &adw::HeaderBar,
    mode: &Rc<MinimalView>,
    compact: Option<&CompactPlayer>,
    on_preferences: Rc<dyn Fn()>,
) -> gtk4::Button {
    let open = gtk4::Button::builder()
        .icon_name("view-grid-symbolic")
        .tooltip_text(strings::text(strings::OPEN_COMPACT_VIEW))
        .build();
    open.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::OPEN_COMPACT_VIEW,
    ))]);
    let weak = Rc::downgrade(mode);
    open.connect_clicked(move |_| {
        if let Some(mode) = weak.upgrade() {
            mode.toggle();
        }
    });
    open.set_sensitive(compact.is_some());
    header.pack_end(&open);

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
    }

    arm_smoke_layout(mode);
    open
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

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn header_and_restore_buttons_switch_one_application_window_in_one_activation() {
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
        let mode = MinimalView::new(
            &window,
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
        let open = install(&header, &mode, Some(&compact), Rc::new(|| {}));
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}
        let same_window = window.clone();

        open.emit_clicked();
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert_eq!(compact.layout(), CompactLayout::Card);
        assert!(compact.widget().is_ancestor(&window));
        assert_eq!(window, same_window);
        assert!(window.is_visible());

        let restore = find_button(compact.widget().upcast_ref(), "Return to Library").unwrap();
        restore.emit_clicked();

        assert_eq!(window.content().as_ref(), Some(full_root.upcast_ref()));
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

    fn find_button(root: &gtk4::Widget, tooltip: &str) -> Option<gtk4::Button> {
        let mut child = root.first_child();
        while let Some(widget) = child {
            if let Ok(button) = widget.clone().downcast::<gtk4::Button>() {
                if button.tooltip_text().as_deref() == Some(tooltip) {
                    return Some(button);
                }
            }
            if let Some(button) = find_button(&widget, tooltip) {
                return Some(button);
            }
            child = widget.next_sibling();
        }
        None
    }
}
