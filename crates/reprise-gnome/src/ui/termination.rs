//! START-5: ending the process by a termination request keeps the session.
//!
//! Logging out, `systemctl --user stop` and a harness restart end Reprise with
//! SIGTERM (or SIGHUP, or SIGINT from a terminal) instead of closing the
//! window, so the window's `close_request` never runs. This listens for those
//! signals, saves the session through the same [`SessionSaver`] the close
//! handler uses, and then closes the window like any other close.
//!
//! The save and quit are an orderly answer, but the process still ends the way
//! the signal would have ended it: once the application has run and been torn
//! down, [`finish`] re-raises the handled signal with its default action, so a
//! shell sees death by that signal (128 plus its number) and a service manager
//! sees the stop it asked for. SIGTERM, SIGHUP and SIGINT count as a clean stop
//! to systemd; a plain exit status of 143 would not.
//!
//! Two safety nets keep the listener from making a stuck process unkillable:
//! a repeat signal ends the process once the first request has had its grace,
//! and a watchdog ends it if the main loop never reads the first request at all
//! (see `reprise_platform_linux::termination`).

use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_platform_linux::termination::{self, Relay};

use crate::ui::session_restore::live_geometry;
use crate::ui::session_save::SessionSaver;

/// Arms the listener for `window`. Before this runs, a termination request
/// keeps its default disposition: the process ends and there is no session to
/// save yet. A signal the process inherited as ignored stays ignored.
pub(super) fn wire(window: &adw::ApplicationWindow, saver: Rc<SessionSaver>) {
    let relay = match termination::start() {
        Ok(Some(relay)) => relay.clone(),
        Ok(None) => return,
        Err(error) => {
            tracing::warn!(%error, "could not listen for termination signals; the session is saved on window close only");
            return;
        }
    };
    glib::spawn_future_local(serve(relay, window.downgrade(), saver));
}

/// The main-loop half: waits for the first request and acts on it.
async fn serve(
    relay: Relay,
    window: glib::WeakRef<adw::ApplicationWindow>,
    saver: Rc<SessionSaver>,
) {
    let Some(signal) = relay.take_request().await else {
        return;
    };
    tracing::info!(signal, "termination requested; saving the session");
    handle_request(window.upgrade().as_ref(), &saver);
}

/// Tells the relay the application has stopped running (see
/// [`Relay::release`]).
pub(crate) fn release() {
    if let Some(relay) = termination::running() {
        relay.release();
    }
}

/// The last thing the process does, after the application has run and been
/// torn down (see [`Relay::finish`]).
pub(crate) fn finish() {
    if let Some(relay) = termination::running() {
        relay.finish();
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

#[cfg(test)]
#[path = "termination_tests.rs"]
mod tests;
