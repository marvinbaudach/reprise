//! Composition wiring for every Library/Compact mode entry point.

use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{gio, glib};
use libadwaita as adw;
use reprise_core::db::Db;
use reprise_core::library::settings;

use super::compact_player::CompactPlayer;
use super::file_open::StartupOpenIntent;
use super::first_run::FirstRunDecision;
use super::minimal_view::{self, MinimalView, ViewTransition};
#[cfg(test)]
use super::window_decorations::WindowContentHost;

pub(in crate::ui) fn initial_transition(
    db: &Db,
    first_run: FirstRunDecision,
    intent: StartupOpenIntent,
) -> ViewTransition {
    minimal_view::startup_transition(
        settings::get_window_view_mode(db),
        settings::get_compact_layout(db),
        first_run,
        intent,
    )
}

pub(in crate::ui) fn build_mode(
    window: &adw::ApplicationWindow,
    compact: Option<&CompactPlayer>,
    conn: &Rc<Db>,
    initial: ViewTransition,
    toast_overlay: &adw::ToastOverlay,
) -> Rc<MinimalView> {
    let toast_overlay = toast_overlay.clone();
    MinimalView::new(
        window,
        compact,
        conn.clone(),
        initial,
        Rc::new(move |message| {
            toast_overlay.add_toast(crate::ui::toasts::plain(message));
        }),
    )
}

/// Returns `true` if the current GDK display is X11 (always-on-top is
/// supported). On Wayland the menu item is hidden.
pub(in crate::ui) fn is_x11() -> bool {
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
    _window: &adw::ApplicationWindow,
    mode: &Rc<MinimalView>,
    compact: Option<&CompactPlayer>,
    conn: &Rc<Db>,
    on_preferences: Rc<dyn Fn()>,
) {
    if let Some(compact) = compact {
        let compact_window = mode
            .compact_window()
            .expect("a compact player always has a compact window");
        let toggle =
            gio::SimpleAction::new(crate::ui::primary_menu::ACTION_TOGGLE_MINIMAL_VIEW, None);
        {
            let mode = Rc::downgrade(mode);
            toggle.connect_activate(move |_, _| {
                if let Some(mode) = mode.upgrade() {
                    mode.toggle();
                }
            });
        }
        compact_window.add_action(&toggle);

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
            let above = settings::get_compact_always_on_top(conn);
            if above {
                compact.set_always_on_top_active(true);
                let window_weak = glib::WeakRef::new();
                window_weak.set(Some(&compact_window));
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
        window_weak.set(Some(&compact_window));
        compact.set_on_always_on_top(Rc::new(move |above| {
            if let Some(window) = window_weak.upgrade() {
                set_always_on_top(&window, above);
            }
            if let Some(conn) = conn_weak.upgrade() {
                if let Err(e) = settings::set_compact_always_on_top(&conn, above) {
                    tracing::warn!(%e, "failed to persist always-on-top");
                }
            }
        }));

        let window_weak = glib::WeakRef::new();
        window_weak.set(Some(&compact_window));
        compact.set_on_quit(Rc::new(move || {
            if let Some(window) = window_weak.upgrade() {
                window.close();
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use gtk4::gio;
    use reprise_core::library::settings::{CompactLayout, WindowViewMode};

    use super::*;
    use crate::ui::minimal_view::ViewTransition;

    #[test]
    fn mini_always_on_top_hidden_wayland_visible_x11() {
        // X11 supports _NET_WM_STATE_ABOVE → the item is offered (MINI-3).
        assert!(always_on_top_available(true));
        // Wayland exposes no keep-above → hidden entirely, not shown disabled.
        assert!(!always_on_top_available(false));
    }

    #[test]
    fn mini_6_file_open_startup_does_not_persist_compact_mode() {
        let db = crate::test_db::open().unwrap();
        settings::set_window_view_mode(&db, WindowViewMode::Library).unwrap();

        let transition = initial_transition(
            &db,
            FirstRunDecision::AlreadyCompleted,
            StartupOpenIntent::CompactPlayback,
        );

        assert_eq!(transition.mode, WindowViewMode::Compact);
        assert_eq!(settings::get_window_view_mode(&db), WindowViewMode::Library);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mini_6_apply_initial_does_not_persist_automatic_compact_mode() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        if gtk4::init().is_err() {
            return;
        }
        let app = adw::Application::builder()
            .application_id("io.github.marvinbaudach.Reprise.FileOpenCompactModeTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::builder().application(&app).build();
        let full_root = test_split_view();
        let compact = CompactPlayer::new();
        let conn = Rc::new(crate::test_db::open().unwrap());
        settings::set_window_view_mode(&conn, WindowViewMode::Library).unwrap();
        let content_host = WindowContentHost::new(&window);
        content_host.set_content(&full_root);
        let mode = MinimalView::new(
            &window,
            Some(&compact),
            conn.clone(),
            ViewTransition {
                mode: WindowViewMode::Compact,
                layout: CompactLayout::Card,
            },
            Rc::new(|_| {}),
        );

        mode.apply_initial();

        assert_eq!(
            settings::get_window_view_mode(&conn),
            WindowViewMode::Library
        );
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
    fn library_entry_wiring_adds_no_header_button_and_uses_a_transient_compact_window() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        if gtk4::init().is_err() {
            return;
        }
        let app = adw::Application::builder()
            .application_id("io.github.marvinbaudach.Reprise.CompactModeTest")
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
        let conn = Rc::new(crate::test_db::open().unwrap());
        let content_host = WindowContentHost::new(&window);
        content_host.set_content(&full_root);
        let mode = MinimalView::new(
            &window,
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
        assert_eq!(app.windows().len(), 2);
        assert!(mode
            .compact_window()
            .unwrap()
            .lookup_action(crate::ui::primary_menu::ACTION_TOGGLE_MINIMAL_VIEW)
            .is_some());
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}
        mode.toggle();
        while gtk4::glib::MainContext::default().iteration(false) {}

        let compact_window = compact
            .handle()
            .root()
            .and_downcast::<adw::ApplicationWindow>()
            .expect("the compact card has its own application window");
        assert_ne!(compact_window, window);
        assert_eq!(
            compact_window.transient_for().as_ref(),
            Some(window.upcast_ref())
        );
        assert!(!window.is_visible());
        assert!(compact_window.is_visible());
        assert_eq!(
            content_host.content().as_ref(),
            Some(full_root.upcast_ref()),
            "the Library tree remains mounted while compact mode is visible"
        );

        compact.activate_restore_for_test();

        assert_eq!(
            content_host.content().as_ref(),
            Some(full_root.upcast_ref())
        );
        assert!(window.is_visible());
        assert!(!compact_window.is_visible());
        window.close();
    }

    fn wait_for(label: &str, mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !condition() && Instant::now() < deadline {
            while glib::MainContext::default().iteration(false) {}
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(condition(), "window manager did not reach: {label}");
    }

    fn x11_window_id(window: &adw::ApplicationWindow) -> String {
        let surface = window
            .surface()
            .unwrap()
            .downcast::<gdk4_x11::X11Surface>()
            .unwrap();
        unsafe { gdk4_x11::ffi::gdk_x11_surface_get_xid(surface.as_ptr() as *mut _).to_string() }
    }

    fn xdotool(args: &[&str]) -> String {
        let output = Command::new("xdotool")
            .args(args)
            .output()
            .expect("the display regression needs xdotool");
        assert!(output.status.success(), "xdotool command failed: {args:?}");
        String::from_utf8(output.stdout).unwrap()
    }

    fn x11_geometry(window: &adw::ApplicationWindow) -> (i32, i32, i32, i32, bool) {
        let output = xdotool(&["getwindowgeometry", "--shell", &x11_window_id(window)]);
        let field = |name: &str| {
            output
                .lines()
                .find_map(|line| line.strip_prefix(&format!("{name}=")))
                .unwrap()
                .parse::<i32>()
                .unwrap()
        };
        (
            field("X"),
            field("Y"),
            field("WIDTH"),
            field("HEIGHT"),
            window.is_maximized(),
        )
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mode_switch_preserves_library_window_geometry() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let app = adw::Application::builder()
            .application_id("io.github.marvinbaudach.Reprise.ModeGeometryTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::builder()
            .application(&app)
            .default_width(900)
            .default_height(600)
            .build();
        WindowContentHost::new(&window).set_content(&test_split_view());
        let compact = CompactPlayer::new();
        let mode = MinimalView::new(
            &window,
            Some(&compact),
            Rc::new(crate::test_db::open().unwrap()),
            ViewTransition {
                mode: WindowViewMode::Library,
                layout: CompactLayout::Card,
            },
            Rc::new(|_| {}),
        );
        mode.apply_initial();
        wait_for("library mapped", || {
            window.is_mapped() && window.width() > 0
        });
        let xid = x11_window_id(&window);
        xdotool(&["windowsize", "--sync", &xid, "987", "654"]);
        xdotool(&["windowmove", "--sync", &xid, "137", "91"]);
        wait_for("library positioned", || {
            let geometry = x11_geometry(&window);
            geometry.0 == 137 && geometry.1 == 91
        });

        let restored_geometry = x11_geometry(&window);
        mode.toggle();
        wait_for("compact visible", || !window.is_visible());
        mode.toggle();
        wait_for("library restored", || window.is_visible());
        wait_for("library geometry restored", || {
            x11_geometry(&window) == restored_geometry
        });

        let mut window_manager = Command::new("openbox")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the maximize regression needs the test window manager");
        std::thread::sleep(Duration::from_millis(250));
        window.maximize();
        wait_for("library maximized", || window.is_maximized());
        mode.toggle();
        wait_for("compact visible from maximized", || !window.is_visible());
        mode.toggle();
        wait_for("maximized library restored", || window.is_visible());
        assert!(window.is_maximized());
        let _ = window_manager.kill();
        let _ = window_manager.wait();
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
