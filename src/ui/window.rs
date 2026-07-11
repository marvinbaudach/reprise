//! Builds the main application window: a libadwaita `ToolbarView` with a
//! header bar (search entry + scan button) over the track list, and the
//! player bar as the bottom bar. Scanning (the disabled header button) is
//! wired up in Task 10.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use rusqlite::Connection;

use super::player_controller::PlayerController;
use super::strings;
use super::track_list::{OnActivate, TrackList};

/// Debounce delay between the last keystroke in the search entry and the
/// track-list reload it triggers, so fast typing doesn't fire a query per
/// keystroke.
const SEARCH_DEBOUNCE_MS: u32 = 200;

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
/// (via `Rc<RefCell<_>>`) and reads through it via `track_list::TrackList`.
/// Scanning (Task 10) will write through the same connection.
pub fn build(app: &adw::Application, conn: Rc<RefCell<Connection>>) {
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

    // The player is created eagerly at window build (not lazily on first
    // activation): construction is cheap (one playbin, no I/O), the
    // `REPRISE_AUDIO_SINK` override keeps headless environments working, and
    // eager creation means the bottom bar exists — greyed out — from the
    // first frame. If GStreamer is unavailable the app degrades to a library
    // browser: error logged, no player bar, activations warn (fault
    // tolerance: never crash over a missing subsystem).
    let player = match PlayerController::new() {
        Ok(controller) => Some(controller),
        Err(error) => {
            tracing::error!(%error, "player unavailable: playback disabled");
            None
        }
    };

    let on_activate: OnActivate = {
        let player = player.clone();
        Box::new(move |track| match &player {
            Some(player) => player.play_track(track),
            None => {
                tracing::warn!(path = %track.path, "player unavailable; ignoring activation");
            }
        })
    };
    let track_list = TrackList::new(conn, on_activate);

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(track_list.widget()));
    if let Some(player) = &player {
        toolbar_view.add_bottom_bar(player.bar_widget());
    }

    window.set_content(Some(&toolbar_view));

    wire_search(&search_entry, track_list);

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

/// Wires the header's `SearchEntry` to `track_list`: every `search-changed`
/// emission (GTK already coalesces pure text-composition events for us, but
/// not typing speed) restarts a 200 ms debounce timer, canceling any timer
/// still pending, before reloading the track list with the current text as
/// the filter. `track_list` is moved in and lives for as long as the timer
/// closure — the window itself owns no other reference to it, so this is
/// also what keeps it alive for the lifetime of the widget tree.
fn wire_search(search_entry: &gtk4::SearchEntry, track_list: TrackList) {
    let track_list = Rc::new(track_list);
    let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

    search_entry.connect_search_changed(move |entry| {
        if let Some(previous) = pending.borrow_mut().take() {
            previous.remove();
        }
        let text = entry.text().to_string();
        let track_list = track_list.clone();
        let pending_for_timeout = pending.clone();
        let source_id = glib::timeout_add_local(
            std::time::Duration::from_millis(u64::from(SEARCH_DEBOUNCE_MS)),
            move || {
                track_list.set_filter(&text);
                // The timer fired: nothing left to cancel next time.
                pending_for_timeout.borrow_mut().take();
                glib::ControlFlow::Break
            },
        );
        *pending.borrow_mut() = Some(source_id);
    });
}
