use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::AdwApplicationWindowExt;

use super::{widget_contains_focus, wire_focus_search};
use crate::ui::test_settle::{settle_until, DISPLAY_TEST_TIMEOUT};
use crate::ui::window::search_popover::SearchPopover;

fn fixture() -> (
    adw::ApplicationWindow,
    gtk4::ToggleButton,
    gtk4::SearchEntry,
    SearchPopover,
) {
    gtk4::init().unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.marvinbaudach.Reprise.SearchCtrlFCloseTest")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&app);
    let entry = gtk4::SearchEntry::new();
    let lens = gtk4::ToggleButton::new();
    let search = SearchPopover::new(&lens, &entry);
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.append(&lens);
    window.set_content(Some(&content));
    wire_focus_search(&app, &window, search.downgrade(), Rc::new(|| true));
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}
    (window, lens, entry, search)
}

fn send_ctrl_f_key() {
    let status = std::process::Command::new("xdotool")
        .args(["key", "--clearmodifiers", "ctrl+f"])
        .status()
        .expect("xdotool is required by the X11 display test");
    assert!(status.success(), "xdotool could not send Ctrl+F");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_6_ctrl_f_closes_the_open_popover_while_its_entry_has_focus() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let (window, lens, entry, search) = fixture();
    // Control arm: the same real key opens the closed popover from the lens,
    // so a failure below is the open popover not closing, not the key failing
    // to arrive.
    lens.grab_focus();
    send_ctrl_f_key();
    assert!(
        settle_until(DISPLAY_TEST_TIMEOUT, || search.is_open()),
        "a real Ctrl+F opens the closed popover"
    );
    entry.set_text("falling");
    assert!(
        settle_until(DISPLAY_TEST_TIMEOUT, || {
            widget_contains_focus(&window, entry.upcast_ref())
        }),
        "the entry holds the keyboard focus"
    );

    send_ctrl_f_key();
    assert!(
        settle_until(DISPLAY_TEST_TIMEOUT, || !search.is_open()),
        "a real Ctrl+F closes the popover"
    );
    assert_eq!(entry.text(), "falling", "closing keeps the query");
    window.close();
}
