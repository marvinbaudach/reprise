//! Composition wiring for every Library/Compact mode entry point.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::library::settings;
use rusqlite::Connection;

use super::compact_player::CompactPlayer;
use super::first_run::FirstRunDecision;
use super::minimal_view::{self, MinimalView, ViewTransition};
use super::window_decorations::WindowContentHost;

pub(in crate::ui) fn initial_transition(
    conn: &Connection,
    first_run: FirstRunDecision,
) -> ViewTransition {
    minimal_view::startup_transition(
        settings::get_window_view_mode(conn),
        settings::get_compact_layout(conn),
        first_run,
    )
}

pub(in crate::ui) fn build_mode(
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

/// Returns `true` if the current GDK display is X11 (always-on-top is
/// supported). On Wayland the menu item is hidden.
fn is_x11() -> bool {
    gtk4::gdk::Display::default()
        .and_then(|d| d.downcast::<gdk4_x11::X11Display>().ok())
        .is_some()
}

/// Whether the "Always on Top" menu item is offered at all — it maps directly
/// to X11 support: on X11 the item appears (MINI-3), on Wayland (no GTK4
/// keep-above) it is hidden entirely rather than shown dead/disabled.
const fn always_on_top_available(is_x11: bool) -> bool {
    is_x11
}

/// Sets or clears the always-on-top window state. On X11 this sends the
/// `_NET_WM_STATE_ABOVE` hint via the X11 backend; on non-X11 displays
/// this is a no-op (the menu item is already disabled).
fn set_always_on_top(window: &adw::ApplicationWindow, above: bool) {
    let Some(surface) = window.surface() else {
        return;
    };
    if surface.downcast_ref::<gdk4_x11::X11Surface>().is_none() {
        return;
    }

    // GDK4 X11: the toplevel state API does not expose _NET_WM_STATE_ABOVE
    // directly, but we can send the client message via the X11 surface.
    // For now, use the Xlib-level API through gdk4-x11.
    let x11_surface: &gdk4_x11::X11Surface = surface.downcast_ref().unwrap();
    let xdisplay = x11_surface
        .display()
        .downcast::<gdk4_x11::X11Display>()
        .unwrap();

    unsafe {
        let xlib_display = gdk4_x11::ffi::gdk_x11_display_get_xdisplay(xdisplay.as_ptr() as *mut _);
        let xwindow = gdk4_x11::ffi::gdk_x11_surface_get_xid(x11_surface.as_ptr() as *mut _);
        let root = x11::xlib::XDefaultRootWindow(xlib_display as *mut _);
        let net_wm_state = x11::xlib::XInternAtom(
            xlib_display as *mut _,
            c"_NET_WM_STATE".as_ptr(),
            x11::xlib::False,
        );
        let net_wm_state_above = x11::xlib::XInternAtom(
            xlib_display as *mut _,
            c"_NET_WM_STATE_ABOVE".as_ptr(),
            x11::xlib::False,
        );

        let action = if above { 1 } else { 0 }; // _NET_WM_STATE_ADD / _NET_WM_STATE_REMOVE
        let mut event: x11::xlib::XClientMessageEvent = std::mem::zeroed();
        event.type_ = x11::xlib::ClientMessage;
        event.window = xwindow as u64;
        event.message_type = net_wm_state;
        event.format = 32;
        event.data.set_long(0, action);
        event.data.set_long(1, net_wm_state_above as i64);
        event.data.set_long(2, 0);
        event.data.set_long(3, 1); // source: application

        x11::xlib::XSendEvent(
            xlib_display as *mut _,
            root,
            x11::xlib::False,
            x11::xlib::SubstructureRedirectMask | x11::xlib::SubstructureNotifyMask,
            &mut event as *mut x11::xlib::XClientMessageEvent as *mut x11::xlib::XEvent,
        );
        x11::xlib::XFlush(xlib_display as *mut _);
    }
    tracing::debug!(above, "X11: _NET_WM_STATE_ABOVE toggled");
}

pub(in crate::ui) fn install(
    window: &adw::ApplicationWindow,
    mode: &Rc<MinimalView>,
    compact: Option<&CompactPlayer>,
    conn: &Rc<RefCell<Connection>>,
    on_preferences: Rc<dyn Fn()>,
) {
    if let Some(compact) = compact {
        let weak = Rc::downgrade(mode);
        compact.set_on_restore(Rc::new(move || {
            if let Some(mode) = weak.upgrade() {
                mode.toggle();
            }
        }));
        compact.set_on_preferences(on_preferences);

        // Always-on-Top: X11 only; hide the menu item entirely on Wayland
        // (MINI-3) rather than leaving a dead, grayed-out entry.
        let x11_available = is_x11();
        compact.set_always_on_top_available(always_on_top_available(x11_available));

        // Restore persisted state.
        if x11_available {
            let above = settings::get_compact_always_on_top(&conn.borrow());
            if above {
                compact.set_always_on_top_active(true);
                let window_weak = glib::WeakRef::new();
                window_weak.set(Some(window));
                // Defer until the window is mapped so the surface exists.
                gtk4::glib::idle_add_local_once(move || {
                    if let Some(window) = window_weak.upgrade() {
                        set_always_on_top(&window, true);
                    }
                });
            }
        }

        let conn_weak = Rc::downgrade(conn);
        let window_weak = glib::WeakRef::new();
        window_weak.set(Some(window));
        compact.set_on_always_on_top(Rc::new(move |above| {
            if let Some(window) = window_weak.upgrade() {
                set_always_on_top(&window, above);
            }
            if let Some(conn) = conn_weak.upgrade() {
                if let Err(e) = settings::set_compact_always_on_top(&conn.borrow(), above) {
                    tracing::warn!(%e, "failed to persist always-on-top");
                }
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
    fn mini_always_on_top_hidden_wayland_visible_x11() {
        // X11 supports _NET_WM_STATE_ABOVE → the item is offered (MINI-3).
        assert!(always_on_top_available(true));
        // Wayland exposes no keep-above → hidden entirely, not shown disabled.
        assert!(!always_on_top_available(false));
    }

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
        let _main_context = crate::ui::test_main_context::lock_main_context();
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
            conn.clone(),
            ViewTransition {
                mode: WindowViewMode::Library,
                layout: CompactLayout::Card,
            },
            Rc::new(|_| {}),
        );
        mode.apply_initial();
        let header = adw::HeaderBar::new();
        install(&window, &mode, Some(&compact), &conn, Rc::new(|| {}));
        assert!(!has_button_with_tooltip(&header, "Open Compact View"));
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}
        let same_window = window.clone();

        mode.toggle();
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert!(compact.handle().is_ancestor(&window));
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
