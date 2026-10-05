//! START-5: ending the process by a termination request keeps the session.
//!
//! Logging out, `systemctl --user stop` and a harness restart end Reprise with
//! SIGTERM (or SIGHUP, or SIGINT from a terminal) instead of closing the
//! window, so the window's `close_request` never runs. This listens for those
//! signals, saves the session through the same [`SessionSaver`] the close
//! handler uses, and then closes the window like any other close.

use std::io;
use std::rc::Rc;
use std::thread;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use signal_hook::low_level::emulate_default_handler;

use crate::ui::session_restore::live_geometry;
use crate::ui::session_save::SessionSaver;

const TERMINATION_SIGNALS: [i32; 3] = [SIGTERM, SIGHUP, SIGINT];
const LISTENER_THREAD_NAME: &str = "termination-signals";

/// Arms the listener for `window`. Before this runs, a termination request
/// keeps its default disposition: the process ends and there is no session to
/// save yet.
pub(super) fn wire(window: &adw::ApplicationWindow, saver: Rc<SessionSaver>) {
    let received = match listen() {
        Ok(received) => received,
        Err(error) => {
            tracing::warn!(%error, "could not listen for termination signals; the session is saved on window close only");
            return;
        }
    };
    let window = window.downgrade();
    glib::spawn_future_local(async move {
        let Ok(signal) = received.recv().await else {
            return;
        };
        tracing::info!(signal, "termination requested; saving the session");
        if let Some(window) = window.upgrade() {
            handle(&window, &saver);
        }
    });
}

/// What a termination request does once it reaches the main loop: save, take
/// the same shutdown a window close takes, then quit the application.
///
/// The quit is the backstop. An open modal dialog (the first-run wizard) can
/// keep the window from closing, and a request to end the process must end it
/// whatever is on screen; the session is already on disk by then.
pub(super) fn handle(window: &adw::ApplicationWindow, saver: &SessionSaver) {
    saver.save_once(live_geometry(window));
    // Compact mode never presents the library window, and GTK 4.22 reads a
    // closing window's GdkSurface while removing it from the application, so
    // give it one first (the compact close chain does the same).
    if !window.is_realized() {
        gtk4::prelude::WidgetExt::realize(window);
    }
    window.close();
    if let Some(application) = window.application() {
        application.quit();
    }
}

/// Forwards the first termination signal to the main loop. Signal handlers
/// cannot touch GTK, so a thread turns the signal into a channel message.
///
/// Every later signal ends the process the way the signal normally would: if
/// the orderly shutdown is wedged, a second SIGTERM must still work.
fn listen() -> io::Result<async_channel::Receiver<i32>> {
    let mut signals = Signals::new(TERMINATION_SIGNALS)?;
    let (sender, receiver) = async_channel::bounded(1);
    thread::Builder::new()
        .name(LISTENER_THREAD_NAME.into())
        .spawn(move || {
            let mut forwarded = false;
            for signal in signals.forever() {
                if forwarded || sender.try_send(signal).is_err() {
                    if let Err(error) = emulate_default_handler(signal) {
                        tracing::error!(%error, signal, "could not end the process on a repeated termination signal");
                    }
                }
                forwarded = true;
            }
        })?;
    Ok(receiver)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use gtk4::glib;
    use reprise_core::browser::BrowserPlace;
    use reprise_core::library::session;
    use reprise_core::view_source::ViewSource;

    use super::*;
    use crate::ui::nav_history::{NavHistory, NavPlace};
    use crate::ui::track_list::TrackList;

    const REMEMBERED_SEARCH: &str = "remember me";

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn start_5_a_termination_request_saves_the_visible_place_and_closes_the_window() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let app = adw::Application::builder()
            .application_id("io.github.marvinbaudach.Reprise.TerminationSaveTest")
            .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gtk4::gio::Cancellable>).unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let track_list = Rc::new(TrackList::new(
            conn.clone(),
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            crate::ui::track_list::queue_sections::QueueViewModel::default,
            crate::ui::cover_download_worker::setup_for_test(),
        ));
        let mut place = BrowserPlace::from(ViewSource::Library);
        place.track_state_mut().unwrap().search = REMEMBERED_SEARCH.into();
        track_list.restore_browser_place(&place);
        let nav_history = Rc::new(NavHistory::default());
        nav_history.record_route(&NavPlace::browser(place.clone()));
        let loaded = session::SessionState::default();
        let geometry = Rc::new(Cell::new((900, 600, false)));
        let window = adw::ApplicationWindow::builder()
            .application(&app)
            .default_width(900)
            .default_height(600)
            .content(track_list.widget())
            .build();
        window.present();
        crate::ui::test_settle::settle_until_mapped(track_list.widget());
        let saver = Rc::new(SessionSaver::new(
            &conn,
            &track_list,
            None,
            &loaded,
            &geometry,
            &nav_history,
        ));
        let saver_for_close = saver.clone();
        window.connect_close_request(move |window| {
            saver_for_close.save_once(live_geometry(window));
            glib::Propagation::Proceed
        });

        handle(&window, &saver);
        crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
            !window.is_visible()
        });

        assert!(
            !window.is_visible(),
            "the window closed like a normal close"
        );
        let restored = session::load(&conn);
        let mut saved_place = restored
            .browser_place
            .expect("the termination request saved a browser place");
        assert_eq!(saved_place.view_source(), ViewSource::Library);
        assert_eq!(
            saved_place
                .track_state_mut()
                .map(|state| state.search.clone()),
            Some(REMEMBERED_SEARCH.to_owned())
        );
    }
}
