use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::AdwApplicationWindowExt;
use reprise_core::library::settings::PlayerBarPosition;

use super::window_bootstrap::{MIN_HEIGHT, MIN_WIDTH};

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn style_5_player_bar_survives_the_minimum_window() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    adw::init().unwrap();
    crate::ui::style::install();

    let window = adw::ApplicationWindow::builder()
        .default_width(MIN_WIDTH)
        .default_height(MIN_HEIGHT)
        .width_request(MIN_WIDTH)
        .height_request(MIN_HEIGHT)
        .build();
    let sidebar =
        crate::ui::sidebar::Sidebar::new(Rc::new(crate::test_db::open().unwrap()), &window, || 0);
    let sidebar_page = adw::NavigationPage::builder()
        .title("Library")
        .child(sidebar.widget())
        .build();
    let content = adw::NavigationPage::builder()
        .title("Music")
        .child(&gtk4::Label::new(Some("Tracks")))
        .build();
    let split = adw::OverlaySplitView::builder()
        .sidebar(&sidebar_page)
        .content(&content)
        .show_sidebar(true)
        .collapsed(false)
        .pin_sidebar(true)
        .build();
    let player = crate::ui::player_bar::PlayerBar::new();
    let natural_bar_height = player
        .widget()
        .measure(gtk4::Orientation::Vertical, MIN_WIDTH)
        .1;
    let shell = crate::ui::library_player_bar::LibraryPlayerBarShell::new(
        &split,
        Some(player.widget().upcast_ref()),
        PlayerBarPosition::Bottom,
    );
    let header = adw::HeaderBar::new();
    let search = gtk4::SearchEntry::new();
    let chrome = super::library_chrome::build(&header, shell.widget(), &search, &window);
    window.set_content(Some(&chrome.root));
    window.present();
    drain_display_events();

    let bar_bounds = player
        .widget()
        .compute_bounds(&window)
        .expect("the player bar is allocated in the window");
    assert!(
        bar_bounds.y() >= 0.0
            && bar_bounds.y() + bar_bounds.height() <= window.height() as f32
            && bar_bounds.height() >= natural_bar_height as f32,
        "player bar must keep its natural height inside the {MIN_WIDTH}x{MIN_HEIGHT} window: bounds={bar_bounds:?}, natural={natural_bar_height}, window={}",
        window.height()
    );

    let library_floor = sidebar
        .shared
        .listbox
        .row_at_index(5)
        .expect("the Library block has a heading and five rows");
    let floor_bounds = library_floor
        .compute_bounds(&window)
        .expect("the last Library row is allocated in the window");
    assert!(
        floor_bounds.y() >= 0.0 && floor_bounds.y() + floor_bounds.height() <= bar_bounds.y(),
        "the complete Library block must remain above the player bar: row={floor_bounds:?}, bar={bar_bounds:?}"
    );

    window.close();
}

fn drain_display_events() {
    while gtk4::glib::MainContext::default().iteration(false) {}
}
