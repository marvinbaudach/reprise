//! START-5: ending the process by a termination request keeps the session.
//!
//! Logging out, `systemctl --user stop` and a harness restart end Reprise with
//! SIGTERM (or SIGHUP, or SIGINT from a terminal) instead of closing the
//! window, so the window's `close_request` never runs. This listens for those
//! signals, saves the session through the same [`SessionSaver`] the close
//! handler uses, and then closes the window like any other close.
//!
//! The exit status after a handled request is 0, not 128 plus the signal. The
//! request is answered by an orderly save and quit, and a service manager
//! counts an exit code of 143 after SIGTERM as a failed stop unless the unit
//! declares it a success, whereas dying from the signal itself is clean.

use std::io;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use signal_hook::low_level::emulate_default_handler;

use crate::ui::session_restore::live_geometry;
use crate::ui::session_save::SessionSaver;
use crate::ui::termination_relay::{self, Shared};

const TERMINATION_SIGNALS: [i32; 3] = [SIGTERM, SIGHUP, SIGINT];

/// The one listener of the process; signal dispositions are process-wide.
static LISTENER: OnceLock<Listener> = OnceLock::new();

struct Listener {
    shared: Arc<Shared>,
    received: async_channel::Receiver<i32>,
}

/// Arms the listener for `window`. Before this runs, a termination request
/// keeps its default disposition: the process ends and there is no session to
/// save yet. A signal the process inherited as ignored stays ignored.
pub(super) fn wire(window: &adw::ApplicationWindow, saver: Rc<SessionSaver>) {
    let listener = match start() {
        Ok(Some(listener)) => listener,
        Ok(None) => return,
        Err(error) => {
            tracing::warn!(%error, "could not listen for termination signals; the session is saved on window close only");
            return;
        }
    };
    glib::spawn_future_local(serve(
        listener.received.clone(),
        listener.shared.clone(),
        window.downgrade(),
        saver,
    ));
}

/// The main-loop half: waits for the first request and acts on it.
async fn serve(
    received: async_channel::Receiver<i32>,
    shared: Arc<Shared>,
    window: glib::WeakRef<adw::ApplicationWindow>,
    saver: Rc<SessionSaver>,
) {
    let Ok(signal) = received.recv().await else {
        return;
    };
    shared.mark_taken();
    tracing::info!(signal, "termination requested; saving the session");
    handle_request(window.upgrade().as_ref(), &saver);
}

/// Stops acting on termination requests once the application has stopped
/// running: nothing is left to save, so a signal during teardown ends the
/// process as it did before START-5, including one still waiting unread.
pub(crate) fn release() {
    let Some(listener) = LISTENER.get() else {
        return;
    };
    listener.shared.release();
    if let Ok(signal) = listener.received.try_recv() {
        end_process(signal);
    }
}

/// A termination request reaching the main loop. Without a window there is
/// nothing to save, but the request must still end the application.
fn handle_request(window: Option<&adw::ApplicationWindow>, saver: &SessionSaver) {
    match window {
        Some(window) => handle(window, saver),
        None => quit_application(),
    }
}

fn quit_application() {
    if let Some(application) = gio::Application::default() {
        application.quit();
    }
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
    // Read before closing: a window that closes drops its application link,
    // and the quit below must still reach the application.
    let application = window.application();
    window.close();
    if let Some(application) = application {
        application.quit();
    }
}

fn start() -> io::Result<Option<&'static Listener>> {
    let signals = termination_relay::armed(&TERMINATION_SIGNALS, termination_relay::is_ignored);
    if signals.is_empty() {
        tracing::info!("every termination signal was inherited as ignored; leaving them ignored");
        return Ok(None);
    }
    let mut signals = Signals::new(signals)?;
    let (sender, received) = async_channel::bounded(1);
    let shared = Arc::new(Shared::default());
    termination_relay::spawn(
        move |deliver| signals.forever().for_each(deliver),
        shared.clone(),
        sender,
        end_process,
    )?;
    Ok(Some(LISTENER.get_or_init(|| Listener { shared, received })))
}

/// Ends the process the way `signal` normally would.
fn end_process(signal: i32) {
    if let Err(error) = emulate_default_handler(signal) {
        tracing::error!(%error, signal, "could not end the process on a termination signal");
    }
}

#[cfg(test)]
#[path = "termination_tests.rs"]
mod tests;
