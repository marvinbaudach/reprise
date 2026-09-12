use std::cell::Cell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;

use super::wire_window_lifecycle;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ctrl_q_uses_the_window_close_path() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.marvinbaudach.Reprise.QuitShortcutTest")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::builder().application(&app).build();
    let close_seen = Rc::new(Cell::new(false));
    window.connect_close_request({
        let close_seen = close_seen.clone();
        move |_| {
            close_seen.set(true);
            gtk4::glib::Propagation::Proceed
        }
    });
    wire_window_lifecycle(&app, &window);
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    gtk4::prelude::ActionGroupExt::activate_action(&app, "quit", None);

    assert!(close_seen.get(), "Ctrl+Q bypassed the window close path");
}
