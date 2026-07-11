//! Builds the main application window: a libadwaita `ToolbarView` with a
//! header bar (search entry + scan button) over a placeholder body. Wired to
//! real data in later tasks (search in Task 9, scanning in Task 10).

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use rusqlite::Connection;

use super::strings;

const DEFAULT_WIDTH: i32 = 1280;
const DEFAULT_HEIGHT: i32 = 800;
const MIN_WIDTH: i32 = 900;
const MIN_HEIGHT: i32 = 600;

/// Environment variable that, when set to any value, arms a one-shot timer
/// that closes the window (and thereby quits the app, since it's the only
/// window) a few seconds after it is shown. This is a standing, permanent
/// headless-verification hook — not a temporary hack — used to confirm in CI
/// or over `xvfb-run` that the app starts, builds its window, and exits
/// cleanly without a human present or a real display driving interaction.
///
/// Usage: `REPRISE_SMOKE_QUIT=1 xvfb-run -a cargo run`.
const SMOKE_QUIT_ENV_VAR: &str = "REPRISE_SMOKE_QUIT";
const SMOKE_QUIT_DELAY_SECS: u32 = 3;

/// Builds and presents the main window for `app`. `conn` is the shared,
/// already-migrated database connection; the UI layer owns it single-threaded
/// (via `Rc<RefCell<_>>`) and will read/write through it once search (Task 9)
/// and scanning (Task 10) are wired up. Unused for now, hence the leading
/// underscore.
pub fn build(app: &adw::Application, _conn: Rc<RefCell<Connection>>) {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(strings::APP_NAME)
        .default_width(DEFAULT_WIDTH)
        .default_height(DEFAULT_HEIGHT)
        .width_request(MIN_WIDTH)
        .height_request(MIN_HEIGHT)
        .build();

    let window_title = adw::WindowTitle::new(strings::APP_NAME, "");

    let search_entry = gtk4::SearchEntry::builder()
        .placeholder_text(strings::SEARCH_PLACEHOLDER)
        .build();

    // Not yet wired to library::scanner (Task 10) — disabled until then.
    let scan_button = gtk4::Button::with_label(strings::SCAN_FOLDER);
    scan_button.set_sensitive(false);

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&window_title));
    header.pack_start(&search_entry);
    header.pack_end(&scan_button);

    // Not yet wired to library::scanner / queries (Tasks 8/10) — placeholder
    // body shown until the library view exists.
    let status_page = adw::StatusPage::builder()
        .icon_name("folder-music-symbolic")
        .title(strings::EMPTY_LIBRARY_TITLE)
        .description(strings::EMPTY_LIBRARY_DESCRIPTION)
        .vexpand(true)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&status_page));

    window.set_content(Some(&toolbar_view));

    if std::env::var(SMOKE_QUIT_ENV_VAR).is_ok() {
        tracing::info!(
            delay_secs = SMOKE_QUIT_DELAY_SECS,
            "{} set: arming headless smoke-quit timer",
            SMOKE_QUIT_ENV_VAR
        );
        let smoke_window = window.clone();
        glib::timeout_add_seconds_local(SMOKE_QUIT_DELAY_SECS, move || {
            tracing::info!("smoke-quit timer fired: closing main window");
            smoke_window.close();
            glib::ControlFlow::Break
        });
    }

    tracing::info!("main window built");
    window.present();
}
